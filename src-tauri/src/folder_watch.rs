//! Surveillance de dossiers PC → file Import docs (même revue que PDF / compagnon).
//!
//! Défaut : `Documents/SasCompta-import`. Plusieurs chemins possibles (liste
//! persistée). Détecte PDF/images **récursivement** (sous-dossiers inclus),
//! copie dans les pièces workspace, enfile `pending_review`. Idempotent via
//! empreinte SHA-256. Ignore les dossiers parasites courants (`.git`,
//! `node_modules`, `__MACOSX`, `.DS_Store`).

use crate::db::{self, AppState};
use notify::{Config, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager, State};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(700);
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const STABLE_WAIT: Duration = Duration::from_millis(400);
const PREFS_FILE: &str = "folder-watch.json";

const WATCH_EXTS: &[&str] = &["pdf", "png", "jpg", "jpeg", "webp", "tif", "tiff"];

/// Dossiers à ne pas parcourir (scan + poll récursifs).
const SKIP_DIR_NAMES: &[&str] = &[".git", "node_modules", "__MACOSX", ".DS_Store"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderWatchStatus {
    pub running: bool,
    /// Chemins surveillés (ou mémorisés si arrêté).
    pub paths: Vec<String>,
    /// Premier chemin (compat UI / API 0.6.7).
    pub path: Option<String>,
    pub default_path: String,
    pub received_count: u64,
    pub skipped_count: u64,
    pub last_error: Option<String>,
}

impl FolderWatchStatus {
    fn idle(default_path: String, paths: Vec<String>) -> Self {
        let path = paths.first().cloned();
        Self {
            running: false,
            paths,
            path,
            default_path,
            received_count: 0,
            skipped_count: 0,
            last_error: None,
        }
    }
}

/// Fichier de préférences (migration depuis `"path"` unique).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
struct FolderWatchPrefs {
    #[serde(default)]
    paths: Vec<String>,
    /// Ancien format mono-dossier.
    #[serde(default, skip_serializing)]
    path: Option<String>,
}

impl FolderWatchPrefs {
    fn normalized_paths(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = self
            .paths
            .iter()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect();
        if out.is_empty() {
            if let Some(p) = self.path.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
                out.push(PathBuf::from(p));
            }
        }
        dedupe_paths(out)
    }
}

fn dedupe_paths(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for p in paths {
        let key = p.display().to_string();
        if seen.insert(key) {
            out.push(p);
        }
    }
    out
}

fn prefs_file(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Dossier app data inaccessible: {e}"))?;
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join(PREFS_FILE))
}

fn load_prefs(app: &AppHandle) -> FolderWatchPrefs {
    let Ok(path) = prefs_file(app) else {
        return FolderWatchPrefs::default();
    };
    if !path.exists() {
        return FolderWatchPrefs::default();
    }
    let Ok(raw) = fs::read_to_string(&path) else {
        return FolderWatchPrefs::default();
    };
    if raw.trim().is_empty() {
        return FolderWatchPrefs::default();
    }
    serde_json::from_str(&raw).unwrap_or_default()
}

fn save_prefs(app: &AppHandle, paths: &[PathBuf]) -> Result<(), String> {
    let file = prefs_file(app)?;
    let prefs = FolderWatchPrefs {
        paths: paths.iter().map(|p| p.display().to_string()).collect(),
        path: None,
    };
    let json = serde_json::to_string_pretty(&prefs).map_err(|e| e.to_string())?;
    fs::write(file, json).map_err(|e| e.to_string())?;
    Ok(())
}

#[derive(Clone)]
struct LiveConfig {
    paths: Vec<PathBuf>,
    received_count: u64,
    skipped_count: u64,
    last_error: Option<String>,
}

struct Running {
    stop: Arc<AtomicBool>,
    join: Option<JoinHandle<()>>,
    config: Arc<Mutex<LiveConfig>>,
}

pub struct FolderWatchHub {
    inner: Mutex<Option<Running>>,
    /// Liste mémorisée (session + disque).
    preferred_paths: Mutex<Vec<PathBuf>>,
}

impl FolderWatchHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
            preferred_paths: Mutex::new(Vec::new()),
        }
    }

    fn ensure_loaded(&self, app: &AppHandle) {
        let mut pref = self
            .preferred_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if !pref.is_empty() {
            return;
        }
        let loaded = load_prefs(app).normalized_paths();
        if !loaded.is_empty() {
            *pref = loaded;
        }
    }

    fn set_preferred(&self, paths: Vec<PathBuf>) {
        let mut pref = self
            .preferred_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *pref = dedupe_paths(paths);
    }

    fn preferred_clone(&self) -> Vec<PathBuf> {
        self.preferred_paths
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    fn status_locked(
        running: &Option<Running>,
        preferred: &[PathBuf],
        default_path: String,
    ) -> FolderWatchStatus {
        let paths_str = |list: &[PathBuf]| -> Vec<String> {
            list.iter().map(|p| p.display().to_string()).collect()
        };
        let Some(r) = running else {
            return FolderWatchStatus::idle(default_path, paths_str(preferred));
        };
        let cfg = match r.config.lock() {
            Ok(c) => c.clone(),
            Err(_) => return FolderWatchStatus::idle(default_path, paths_str(preferred)),
        };
        let paths = paths_str(&cfg.paths);
        let path = paths.first().cloned();
        FolderWatchStatus {
            running: true,
            paths,
            path,
            default_path,
            received_count: cfg.received_count,
            skipped_count: cfg.skipped_count,
            last_error: cfg.last_error,
        }
    }

    pub fn status(&self, app: &AppHandle) -> FolderWatchStatus {
        self.ensure_loaded(app);
        let default_path = default_watch_dir().display().to_string();
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let pref = self.preferred_clone();
        Self::status_locked(&guard, &pref, default_path)
    }

    pub fn stop(&self) -> FolderWatchStatus {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut running) = guard.take() {
            running.stop.store(true, Ordering::SeqCst);
            if let Some(join) = running.join.take() {
                let _ = join.join();
            }
        }
        let default_path = default_watch_dir().display().to_string();
        let pref = self.preferred_clone();
        Self::status_locked(&None, &pref, default_path)
    }

    fn stop_inner(&self) {
        let mut guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(mut running) = guard.take() {
            running.stop.store(true, Ordering::SeqCst);
            if let Some(join) = running.join.take() {
                let _ = join.join();
            }
        }
    }

    fn is_running(&self) -> bool {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some()
    }
}

/// Dossier par défaut : Documents/SasCompta-import (créé à la demande).
pub fn default_watch_dir() -> PathBuf {
    let base = dirs::document_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Documents")
    });
    base.join("SasCompta-import")
}

fn is_watchable(path: &Path) -> bool {
    if !path.is_file() {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    WATCH_EXTS.iter().any(|e| *e == ext)
}

fn wait_until_stable(path: &Path) -> bool {
    let mut last = None;
    for _ in 0..8 {
        let meta = match fs::metadata(path) {
            Ok(m) => m,
            Err(_) => return false,
        };
        let size = meta.len();
        if last == Some(size) {
            return true;
        }
        last = Some(size);
        thread::sleep(STABLE_WAIT);
    }
    last.is_some()
}

/// Ingère un fichier vers la file import (idempotent). Renvoie `(doc, was_new)`.
pub fn ingest_watched_file(
    state: &AppState,
    source_path: &Path,
) -> Result<(crate::models::DocumentImport, bool), String> {
    if !is_watchable(source_path) {
        return Err("Type de fichier non pris en charge (PDF ou image).".into());
    }
    if !wait_until_stable(source_path) {
        return Err("Fichier encore en cours d’écriture.".into());
    }
    let fingerprint = db::documents::file_fingerprint(source_path).map_err(|e| e.to_string())?;
    let existing = db::with_conn(state, |c| {
        db::documents::find_active_by_fingerprint(c, &fingerprint)
    })
    .map_err(|e| e.to_string())?;
    if let Some(doc) = existing {
        return Ok((doc, false));
    }
    let doc = db::with_conn(state, |c| {
        let path_guard = state.path.lock().map_err(|e| e.to_string())?;
        let workspace = path_guard
            .as_ref()
            .ok_or_else(|| "Aucun workspace ouvert".to_string())?
            .clone();
        drop(path_guard);
        db::documents::analyze_document_idempotent(c, &workspace, source_path, None, true)
    })
    .map_err(|e| e.to_string())?;
    Ok((doc, true))
}

fn should_skip_dir_name(name: &str) -> bool {
    SKIP_DIR_NAMES
        .iter()
        .any(|skip| skip.eq_ignore_ascii_case(name))
}

/// Liste récursive des PDF/images sous `dir` (sous-dossiers inclus).
/// Ignore les dossiers parasites (`SKIP_DIR_NAMES`).
fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    scan_dir_into(dir, &mut out);
    out.sort();
    out
}

fn scan_dir_into(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else {
        return;
    };
    for entry in rd.flatten() {
        let p = entry.path();
        let is_dir = entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false);
        if is_dir {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if should_skip_dir_name(&name) {
                continue;
            }
            scan_dir_into(&p, out);
            continue;
        }
        if is_watchable(&p) {
            out.push(p);
        }
    }
}

fn scan_dirs(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in dirs {
        out.extend(scan_dir(d));
    }
    out.sort();
    out.dedup();
    out
}

fn process_path(
    app: Option<&AppHandle>,
    state: &AppState,
    config: &Arc<Mutex<LiveConfig>>,
    path: &Path,
    seen_fps: &mut HashSet<String>,
) {
    match ingest_watched_file(state, path) {
        Ok((doc, was_new)) => {
            if let Ok(fp) = db::documents::file_fingerprint(path) {
                seen_fps.insert(fp);
            }
            if let Ok(mut cfg) = config.lock() {
                if was_new {
                    cfg.received_count = cfg.received_count.saturating_add(1);
                    cfg.last_error = None;
                } else {
                    cfg.skipped_count = cfg.skipped_count.saturating_add(1);
                }
            }
            if was_new {
                if let Some(app) = app {
                    let _ = app.emit(
                        "folder-watch-document-received",
                        json!({
                            "id": doc.id,
                            "sourceFileName": doc.source_file_name,
                            "docType": doc.doc_type,
                            "status": doc.status,
                            "duplicate": false,
                        }),
                    );
                }
            }
        }
        Err(e) => {
            if let Ok(mut cfg) = config.lock() {
                cfg.last_error = Some(e);
            }
        }
    }
}

fn spawn_watcher(
    app: Option<AppHandle>,
    state: AppState,
    watch_paths: Vec<PathBuf>,
    stop: Arc<AtomicBool>,
    config: Arc<Mutex<LiveConfig>>,
) -> JoinHandle<()> {
    thread::spawn(move || {
        let (tx, rx) = mpsc::channel();
        let mut watcher = match RecommendedWatcher::new(
            move |res: Result<notify::Event, notify::Error>| {
                let _ = tx.send(res);
            },
            Config::default(),
        ) {
            Ok(w) => w,
            Err(e) => {
                if let Ok(mut cfg) = config.lock() {
                    cfg.last_error = Some(format!("Watcher: {e}"));
                }
                run_poll_loop(app.as_ref(), &state, &watch_paths, &stop, &config);
                return;
            }
        };

        for watch_path in &watch_paths {
            if let Err(e) = watcher.watch(watch_path, RecursiveMode::Recursive) {
                if let Ok(mut cfg) = config.lock() {
                    cfg.last_error = Some(format!(
                        "Surveillance ({}): {e}",
                        watch_path.display()
                    ));
                }
            }
        }

        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
        let mut seen_fps: HashSet<String> = HashSet::new();
        for p in scan_dirs(&watch_paths) {
            process_path(app.as_ref(), &state, &config, &p, &mut seen_fps);
        }

        let mut last_poll = Instant::now();
        while !stop.load(Ordering::SeqCst) {
            match rx.recv_timeout(Duration::from_millis(250)) {
                Ok(Ok(event)) => {
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_) | EventKind::Any
                    ) {
                        for p in event.paths {
                            if is_watchable(&p) {
                                pending.insert(p, Instant::now());
                            }
                        }
                    }
                }
                Ok(Err(e)) => {
                    if let Ok(mut cfg) = config.lock() {
                        cfg.last_error = Some(e.to_string());
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }

            let now = Instant::now();
            let ready: Vec<PathBuf> = pending
                .iter()
                .filter(|(_, t)| now.duration_since(**t) >= WATCH_DEBOUNCE)
                .map(|(p, _)| p.clone())
                .collect();
            for p in ready {
                pending.remove(&p);
                process_path(app.as_ref(), &state, &config, &p, &mut seen_fps);
            }

            if last_poll.elapsed() >= POLL_INTERVAL {
                last_poll = Instant::now();
                for p in scan_dirs(&watch_paths) {
                    let fp = match db::documents::file_fingerprint(&p) {
                        Ok(f) => f,
                        Err(_) => continue,
                    };
                    if seen_fps.contains(&fp) {
                        continue;
                    }
                    process_path(app.as_ref(), &state, &config, &p, &mut seen_fps);
                }
            }
        }
        drop(watcher);
    })
}

fn run_poll_loop(
    app: Option<&AppHandle>,
    state: &AppState,
    watch_paths: &[PathBuf],
    stop: &AtomicBool,
    config: &Arc<Mutex<LiveConfig>>,
) {
    let mut seen_fps: HashSet<String> = HashSet::new();
    while !stop.load(Ordering::SeqCst) {
        for p in scan_dirs(watch_paths) {
            let fp = match db::documents::file_fingerprint(&p) {
                Ok(f) => f,
                Err(_) => continue,
            };
            if seen_fps.contains(&fp) {
                continue;
            }
            process_path(app, state, config, &p, &mut seen_fps);
        }
        thread::sleep(POLL_INTERVAL);
    }
}

fn resolve_start_paths(
    hub: &FolderWatchHub,
    paths: Option<Vec<String>>,
    path: Option<String>,
) -> Vec<PathBuf> {
    if let Some(list) = paths {
        let parsed: Vec<PathBuf> = list
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect();
        let parsed = dedupe_paths(parsed);
        if !parsed.is_empty() {
            return parsed;
        }
    }
    if let Some(p) = path {
        let t = p.trim();
        if !t.is_empty() {
            return vec![PathBuf::from(t)];
        }
    }
    let pref = hub.preferred_clone();
    if !pref.is_empty() {
        return pref;
    }
    vec![default_watch_dir()]
}

fn ensure_dirs(paths: &[PathBuf]) -> Result<(), String> {
    for watch_path in paths {
        fs::create_dir_all(watch_path).map_err(|e| {
            format!(
                "Impossible de créer le dossier surveillé ({}): {e}",
                watch_path.display()
            )
        })?;
    }
    Ok(())
}

fn start_watching(
    app: AppHandle,
    state: &AppState,
    hub: &FolderWatchHub,
    watch_paths: Vec<PathBuf>,
) -> Result<FolderWatchStatus, String> {
    {
        let path_guard = state.path.lock().map_err(|e| e.to_string())?;
        if path_guard.is_none() {
            return Err(
                "Ouvrez un workspace avant d’activer la surveillance de dossier.".into(),
            );
        }
    }

    ensure_dirs(&watch_paths)?;
    hub.set_preferred(watch_paths.clone());
    let _ = save_prefs(&app, &watch_paths);

    hub.stop_inner();

    let stop = Arc::new(AtomicBool::new(false));
    let config = Arc::new(Mutex::new(LiveConfig {
        paths: watch_paths.clone(),
        received_count: 0,
        skipped_count: 0,
        last_error: None,
    }));

    let app_state = AppState {
        conn: state.conn.clone(),
        path: state.path.clone(),
    };
    let join = spawn_watcher(
        Some(app.clone()),
        app_state,
        watch_paths,
        stop.clone(),
        config.clone(),
    );

    let mut guard = hub.inner.lock().unwrap_or_else(|e| e.into_inner());
    *guard = Some(Running {
        stop,
        join: Some(join),
        config,
    });
    let default_path = default_watch_dir().display().to_string();
    let pref = hub.preferred_clone();
    Ok(FolderWatchHub::status_locked(&guard, &pref, default_path))
}

#[tauri::command]
pub fn folder_watch_default_path() -> String {
    default_watch_dir().display().to_string()
}

#[tauri::command]
pub fn folder_watch_status(
    app: AppHandle,
    hub: State<'_, FolderWatchHub>,
) -> FolderWatchStatus {
    hub.status(&app)
}

#[tauri::command]
pub fn folder_watch_stop(hub: State<'_, FolderWatchHub>) -> FolderWatchStatus {
    hub.stop()
}

#[tauri::command]
pub fn folder_watch_start(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: State<'_, FolderWatchHub>,
    paths: Option<Vec<String>>,
    path: Option<String>,
) -> Result<FolderWatchStatus, String> {
    hub.ensure_loaded(&app);
    let watch_paths = resolve_start_paths(&hub, paths, path);
    start_watching(app, &state, &hub, watch_paths)
}

#[tauri::command]
pub fn folder_watch_set_paths(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: State<'_, FolderWatchHub>,
    paths: Vec<String>,
) -> Result<FolderWatchStatus, String> {
    hub.ensure_loaded(&app);
    let watch_paths = dedupe_paths(
        paths
            .into_iter()
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from)
            .collect(),
    );
    if watch_paths.is_empty() {
        hub.set_preferred(Vec::new());
        let _ = save_prefs(&app, &[]);
        if hub.is_running() {
            return Ok(hub.stop());
        }
        return Ok(hub.status(&app));
    }
    ensure_dirs(&watch_paths)?;
    hub.set_preferred(watch_paths.clone());
    save_prefs(&app, &watch_paths)?;
    if hub.is_running() {
        return start_watching(app, &state, &hub, watch_paths);
    }
    Ok(hub.status(&app))
}

#[tauri::command]
pub fn folder_watch_add_path(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: State<'_, FolderWatchHub>,
    path: String,
) -> Result<FolderWatchStatus, String> {
    hub.ensure_loaded(&app);
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Chemin de dossier vide.".into());
    }
    let mut list = hub.preferred_clone();
    if list.is_empty() {
        // Migration / premier ajout : garder le défaut si déjà présent en session idle.
        // (liste vide = pas encore choisi)
    }
    list.push(PathBuf::from(trimmed));
    list = dedupe_paths(list);
    ensure_dirs(&list)?;
    hub.set_preferred(list.clone());
    save_prefs(&app, &list)?;
    if hub.is_running() {
        return start_watching(app, &state, &hub, list);
    }
    Ok(hub.status(&app))
}

#[tauri::command]
pub fn folder_watch_remove_path(
    app: AppHandle,
    state: State<'_, AppState>,
    hub: State<'_, FolderWatchHub>,
    path: String,
) -> Result<FolderWatchStatus, String> {
    hub.ensure_loaded(&app);
    let target = path.trim();
    let mut list = hub.preferred_clone();
    list.retain(|p| p.display().to_string() != target);
    hub.set_preferred(list.clone());
    save_prefs(&app, &list)?;
    if list.is_empty() {
        if hub.is_running() {
            return Ok(hub.stop());
        }
        return Ok(hub.status(&app));
    }
    if hub.is_running() {
        return start_watching(app, &state, &hub, list);
    }
    Ok(hub.status(&app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::io::Write;
    use tempfile::tempdir;

    fn write_tiny_pdf(path: &Path, label: &str) {
        let mut f = fs::File::create(path).unwrap();
        writeln!(f, "%PDF-1.4").unwrap();
        writeln!(f, "% {label}").unwrap();
        writeln!(f, "ticket 12,50 EUR").unwrap();
    }

    #[test]
    fn default_dir_ends_with_sascompta_import() {
        let p = default_watch_dir();
        assert!(p
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .eq_ignore_ascii_case("SasCompta-import"));
    }

    #[test]
    fn prefs_migrate_single_path() {
        let raw = r#"{"path":"/tmp/old-watch"}"#;
        let prefs: FolderWatchPrefs = serde_json::from_str(raw).unwrap();
        let paths = prefs.normalized_paths();
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], PathBuf::from("/tmp/old-watch"));
    }

    #[test]
    fn prefs_paths_win_over_legacy() {
        let raw = r#"{"paths":["/a","/b"],"path":"/legacy"}"#;
        let prefs: FolderWatchPrefs = serde_json::from_str(raw).unwrap();
        let paths = prefs.normalized_paths();
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0], PathBuf::from("/a"));
        assert_eq!(paths[1], PathBuf::from("/b"));
    }

    #[test]
    fn dedupe_keeps_order() {
        let out = dedupe_paths(vec![
            PathBuf::from("/a"),
            PathBuf::from("/b"),
            PathBuf::from("/a"),
        ]);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], PathBuf::from("/a"));
        assert_eq!(out[1], PathBuf::from("/b"));
    }

    #[test]
    fn ingest_and_skip_duplicate() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();

        let state = AppState::new();
        *state.conn.lock().unwrap() = Some(conn);
        *state.path.lock().unwrap() = Some(PathBuf::from(&info.path));

        let watch = dir.path().join("watch");
        fs::create_dir_all(&watch).unwrap();
        let pdf = watch.join("note.pdf");
        write_tiny_pdf(&pdf, "folder-watch-1");

        let (a, new_a) = ingest_watched_file(&state, &pdf).unwrap();
        assert!(new_a);
        assert_eq!(a.status, "pending_review");

        let (b, new_b) = ingest_watched_file(&state, &pdf).unwrap();
        assert!(!new_b);
        assert_eq!(a.id, b.id);

        let copy = watch.join("note-copy.pdf");
        fs::copy(&pdf, &copy).unwrap();
        let (c, new_c) = ingest_watched_file(&state, &copy).unwrap();
        assert!(!new_c);
        assert_eq!(a.id, c.id);
    }

    #[test]
    fn rejects_unsupported_extension() {
        let dir = tempdir().unwrap();
        let ws = dir.path().join("demo.sascompta");
        let info = db::create_demo_workspace(&ws).unwrap();
        let (conn, _) = db::open_workspace(&info.path, db::DEMO_PASSWORD).unwrap();
        let state = AppState::new();
        *state.conn.lock().unwrap() = Some(conn);
        *state.path.lock().unwrap() = Some(PathBuf::from(&info.path));

        let txt = dir.path().join("notes.txt");
        fs::write(&txt, "hello").unwrap();
        let err = ingest_watched_file(&state, &txt).unwrap_err();
        assert!(err.contains("non pris en charge"));
    }

    #[test]
    fn status_exposes_multiple_paths() {
        let hub = FolderWatchHub::new();
        hub.set_preferred(vec![
            PathBuf::from("/tmp/w1"),
            PathBuf::from("/tmp/w2"),
        ]);
        let default_path = default_watch_dir().display().to_string();
        let s = FolderWatchHub::status_locked(&None, &hub.preferred_clone(), default_path);
        assert!(!s.running);
        assert_eq!(s.paths.len(), 2);
        assert_eq!(s.path.as_deref(), Some("/tmp/w1"));
    }

    #[test]
    fn scan_dir_finds_nested_pdfs() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let nested = root.join("2024").join("mars");
        fs::create_dir_all(&nested).unwrap();
        write_tiny_pdf(&root.join("root.pdf"), "root");
        write_tiny_pdf(&nested.join("nested.pdf"), "nested");
        fs::write(root.join("ignore.txt"), "nope").unwrap();

        let found = scan_dir(root);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("root.pdf")));
        assert!(found.iter().any(|p| p.ends_with("nested.pdf")));
    }

    #[test]
    fn scan_dir_skips_junk_directories() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        for junk in [".git", "node_modules", "__MACOSX", ".DS_Store"] {
            let junk_dir = root.join(junk);
            fs::create_dir_all(&junk_dir).unwrap();
            write_tiny_pdf(&junk_dir.join("hidden.pdf"), junk);
        }
        let keep = root.join("factures").join("fournisseurs");
        fs::create_dir_all(&keep).unwrap();
        write_tiny_pdf(&keep.join("ok.pdf"), "keep");

        let found = scan_dir(root);
        assert_eq!(found.len(), 1);
        assert!(found[0].ends_with("ok.pdf"));
    }

    #[test]
    fn scan_dirs_covers_each_root_recursively() {
        let dir = tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b").join("deep");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        write_tiny_pdf(&a.join("a.pdf"), "a");
        write_tiny_pdf(&b.join("b.pdf"), "b");

        let found = scan_dirs(&[a.clone(), dir.path().join("b")]);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|p| p.ends_with("a.pdf")));
        assert!(found.iter().any(|p| p.ends_with("b.pdf")));
    }

    #[test]
    fn should_skip_known_junk_names() {
        assert!(should_skip_dir_name(".git"));
        assert!(should_skip_dir_name("node_modules"));
        assert!(should_skip_dir_name("__MACOSX"));
        assert!(should_skip_dir_name(".DS_Store"));
        assert!(should_skip_dir_name("Node_Modules"));
        assert!(!should_skip_dir_name("factures"));
        assert!(!should_skip_dir_name("2024"));
    }
}

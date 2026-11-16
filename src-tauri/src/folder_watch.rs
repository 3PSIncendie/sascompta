//! Surveillance d’un dossier PC → file Import docs (même revue que PDF / compagnon).
//!
//! Défaut : `Documents/SasCompta-import`. Détecte PDF/images, copie dans les
//! pièces workspace, enfile `pending_review` (classifieurs multi-types).
//! Idempotent via empreinte SHA-256 (ne réimporte pas le même contenu).

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
use tauri::{AppHandle, Emitter, State};

const WATCH_DEBOUNCE: Duration = Duration::from_millis(700);
const POLL_INTERVAL: Duration = Duration::from_secs(2);
const STABLE_WAIT: Duration = Duration::from_millis(400);

const WATCH_EXTS: &[&str] = &["pdf", "png", "jpg", "jpeg", "webp", "tif", "tiff"];

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderWatchStatus {
    pub running: bool,
    pub path: Option<String>,
    pub default_path: String,
    pub received_count: u64,
    pub skipped_count: u64,
    pub last_error: Option<String>,
}

impl FolderWatchStatus {
    fn idle(default_path: String) -> Self {
        Self {
            running: false,
            path: None,
            default_path,
            received_count: 0,
            skipped_count: 0,
            last_error: None,
        }
    }
}

#[derive(Clone)]
struct LiveConfig {
    path: PathBuf,
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
    /// Dernier chemin choisi (session), même arrêté.
    preferred_path: Mutex<Option<PathBuf>>,
}

impl FolderWatchHub {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(None),
            preferred_path: Mutex::new(None),
        }
    }

    fn status_locked(
        running: &Option<Running>,
        preferred: &Option<PathBuf>,
        default_path: String,
    ) -> FolderWatchStatus {
        let Some(r) = running else {
            let mut s = FolderWatchStatus::idle(default_path);
            if let Some(p) = preferred {
                s.path = Some(p.display().to_string());
            }
            return s;
        };
        let cfg = match r.config.lock() {
            Ok(c) => c.clone(),
            Err(_) => return FolderWatchStatus::idle(default_path),
        };
        FolderWatchStatus {
            running: true,
            path: Some(cfg.path.display().to_string()),
            default_path,
            received_count: cfg.received_count,
            skipped_count: cfg.skipped_count,
            last_error: cfg.last_error,
        }
    }

    pub fn status(&self) -> FolderWatchStatus {
        let default_path = default_watch_dir().display().to_string();
        let guard = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        let pref = self
            .preferred_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
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
        let pref = self
            .preferred_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        Self::status_locked(&None, &pref, default_path)
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

fn scan_dir(dir: &Path) -> Vec<PathBuf> {
    let Ok(rd) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in rd.flatten() {
        let p = entry.path();
        if is_watchable(&p) {
            out.push(p);
        }
    }
    out.sort();
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
    watch_path: PathBuf,
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
                // Fallback : poll seul.
                run_poll_loop(app.as_ref(), &state, &watch_path, &stop, &config);
                return;
            }
        };
        if let Err(e) = watcher.watch(&watch_path, RecursiveMode::NonRecursive) {
            if let Ok(mut cfg) = config.lock() {
                cfg.last_error = Some(format!("Surveillance: {e}"));
            }
        }

        let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
        let mut seen_fps: HashSet<String> = HashSet::new();
        // Scan initial (fichiers déjà présents).
        for p in scan_dir(&watch_path) {
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
                for p in scan_dir(&watch_path) {
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
    watch_path: &Path,
    stop: &AtomicBool,
    config: &Arc<Mutex<LiveConfig>>,
) {
    let mut seen_fps: HashSet<String> = HashSet::new();
    while !stop.load(Ordering::SeqCst) {
        for p in scan_dir(watch_path) {
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

#[tauri::command]
pub fn folder_watch_default_path() -> String {
    default_watch_dir().display().to_string()
}

#[tauri::command]
pub fn folder_watch_status(hub: State<'_, FolderWatchHub>) -> FolderWatchStatus {
    hub.status()
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
    path: Option<String>,
) -> Result<FolderWatchStatus, String> {
    {
        let path_guard = state.path.lock().map_err(|e| e.to_string())?;
        if path_guard.is_none() {
            return Err("Ouvrez un workspace avant d’activer la surveillance de dossier.".into());
        }
    }

    let watch_path = match path {
        Some(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
        _ => {
            let pref = hub
                .preferred_path
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .clone();
            pref.unwrap_or_else(default_watch_dir)
        }
    };

    fs::create_dir_all(&watch_path).map_err(|e| {
        format!(
            "Impossible de créer le dossier surveillé ({}): {e}",
            watch_path.display()
        )
    })?;

    {
        let mut pref = hub
            .preferred_path
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *pref = Some(watch_path.clone());
    }

    hub.stop();

    let stop = Arc::new(AtomicBool::new(false));
    let config = Arc::new(Mutex::new(LiveConfig {
        path: watch_path.clone(),
        received_count: 0,
        skipped_count: 0,
        last_error: None,
    }));

    let app_state = AppState {
        conn: state.conn.clone(),
        path: state.path.clone(),
    };
    let join = spawn_watcher(
        Some(app),
        app_state,
        watch_path,
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
    let pref = hub
        .preferred_path
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    Ok(FolderWatchHub::status_locked(&guard, &pref, default_path))
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
}

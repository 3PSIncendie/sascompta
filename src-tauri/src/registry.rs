//! Registre local des sociétés (fichiers .sascompta déjà ouverts / créés).
//! Les fichiers restent mono-SAS ; le chemin est géré sous le dossier app data.

use crate::models::KnownWorkspace;
use chrono::Utc;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};
use thiserror::Error;

const REGISTRY_FILE: &str = "workspaces-registry.json";
const WORKSPACES_DIR: &str = "workspaces";

#[derive(Debug, Error)]
pub enum RegistryError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl From<&str> for RegistryError {
    fn from(s: &str) -> Self {
        RegistryError::Msg(s.to_string())
    }
}

impl From<String> for RegistryError {
    fn from(s: String) -> Self {
        RegistryError::Msg(s)
    }
}

pub type RegistryResult<T> = Result<T, RegistryError>;

fn app_data_dir(app: &AppHandle) -> RegistryResult<PathBuf> {
    app.path()
        .app_data_dir()
        .map_err(|e| RegistryError::Msg(format!("Dossier app data inaccessible: {e}")))
}

pub fn workspaces_dir(app: &AppHandle) -> RegistryResult<PathBuf> {
    let dir = app_data_dir(app)?.join(WORKSPACES_DIR);
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

fn registry_path(app: &AppHandle) -> RegistryResult<PathBuf> {
    let dir = app_data_dir(app)?;
    fs::create_dir_all(&dir)?;
    Ok(dir.join(REGISTRY_FILE))
}

fn load_raw(app: &AppHandle) -> RegistryResult<Vec<KnownWorkspace>> {
    let path = registry_path(app)?;
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = fs::read_to_string(&path)?;
    if raw.trim().is_empty() {
        return Ok(Vec::new());
    }
    let list: Vec<KnownWorkspace> = serde_json::from_str(&raw)?;
    Ok(list)
}

fn save_raw(app: &AppHandle, list: &[KnownWorkspace]) -> RegistryResult<()> {
    let path = registry_path(app)?;
    let json = serde_json::to_string_pretty(list)?;
    fs::write(path, json)?;
    Ok(())
}

/// Liste les sociétés connues (fichiers absents retirés du registre).
pub fn list_known(app: &AppHandle) -> RegistryResult<Vec<KnownWorkspace>> {
    let mut list = load_raw(app)?;
    let before = list.len();
    list.retain(|w| Path::new(&w.path).exists());
    if list.len() != before {
        save_raw(app, &list)?;
    }
    list.sort_by(|a, b| b.last_opened_at.cmp(&a.last_opened_at));
    Ok(list)
}

pub fn upsert_known(
    app: &AppHandle,
    path: &str,
    name: &str,
    siren: &str,
) -> RegistryResult<KnownWorkspace> {
    let path_norm = PathBuf::from(path)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(path));
    let path_str = path_norm.display().to_string();
    let now = Utc::now().to_rfc3339();
    let mut list = load_raw(app)?;
    if let Some(existing) = list.iter_mut().find(|w| {
        Path::new(&w.path) == path_norm.as_path() || w.path == path || w.path == path_str
    }) {
        existing.name = name.to_string();
        existing.siren = siren.to_string();
        existing.path = path_str.clone();
        existing.last_opened_at = now.clone();
        let entry = existing.clone();
        save_raw(app, &list)?;
        return Ok(entry);
    }
    let entry = KnownWorkspace {
        id: format!("{:x}", simple_hash(&path_str)),
        name: name.to_string(),
        siren: siren.to_string(),
        path: path_str,
        last_opened_at: now,
    };
    list.push(entry.clone());
    save_raw(app, &list)?;
    Ok(entry)
}

pub fn forget_known(app: &AppHandle, path: &str) -> RegistryResult<()> {
    let mut list = load_raw(app)?;
    let path_buf = PathBuf::from(path);
    list.retain(|w| Path::new(&w.path) != path_buf.as_path() && w.path != path);
    save_raw(app, &list)?;
    Ok(())
}

/// Slug fichier pour une nouvelle société sous le dossier workspaces.
pub fn default_file_path(dir: &Path, org_name: &str, siren: &str) -> PathBuf {
    let base = slugify(org_name);
    let siren_clean: String = siren.chars().filter(|c| c.is_ascii_digit()).collect();
    let stem = if siren_clean.is_empty() {
        base
    } else {
        format!("{base}-{siren_clean}")
    };
    let mut candidate = dir.join(format!("{stem}.sascompta"));
    let mut n = 2u32;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{n}.sascompta"));
        n += 1;
    }
    candidate
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for c in input.chars() {
        let lower = c.to_ascii_lowercase();
        let mapped = match lower {
            'a'..='z' | '0'..='9' => Some(lower),
            'à' | 'á' | 'â' | 'ä' | 'ã' => Some('a'),
            'é' | 'è' | 'ê' | 'ë' => Some('e'),
            'ì' | 'í' | 'î' | 'ï' => Some('i'),
            'ò' | 'ó' | 'ô' | 'ö' | 'õ' => Some('o'),
            'ù' | 'ú' | 'û' | 'ü' => Some('u'),
            'ç' => Some('c'),
            'ñ' => Some('n'),
            ' ' | '-' | '_' | '.' => Some('-'),
            _ => None,
        };
        if let Some(ch) = mapped {
            if ch == '-' {
                if !out.is_empty() && !prev_dash {
                    out.push('-');
                    prev_dash = true;
                }
            } else {
                out.push(ch);
                prev_dash = false;
            }
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "societe".into()
    } else {
        trimmed.chars().take(48).collect()
    }
}

fn simple_hash(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn slugify_basic() {
        assert_eq!(slugify("Atelier Lumière SAS"), "atelier-lumiere-sas");
        assert_eq!(slugify("???"), "societe");
    }

    #[test]
    fn default_path_unique() {
        let dir = tempdir().unwrap();
        let p1 = default_file_path(dir.path(), "Demo SAS", "123456782");
        assert_eq!(
            p1.file_name().unwrap().to_str().unwrap(),
            "demo-sas-123456782.sascompta"
        );
        fs::write(&p1, b"x").unwrap();
        let p2 = default_file_path(dir.path(), "Demo SAS", "123456782");
        assert_eq!(
            p2.file_name().unwrap().to_str().unwrap(),
            "demo-sas-123456782-2.sascompta"
        );
    }
}

pub mod assets;
pub mod chart;
pub mod closing;
pub mod ledger;
pub mod pcg;
pub mod reporting;
pub mod statements;
pub mod tax;

use crate::models::*;
use crate::security::{hash_password, verify_password};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

pub const SCHEMA_VERSION: i32 = 6;
pub const DEMO_PASSWORD: &str = "SecretPass1";

#[derive(Debug, Error)]
pub enum DbError {
    #[error("{0}")]
    Msg(String),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Security(#[from] crate::security::SecurityError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<String> for DbError {
    fn from(s: String) -> Self {
        DbError::Msg(s)
    }
}

impl From<&str> for DbError {
    fn from(s: &str) -> Self {
        DbError::Msg(s.to_string())
    }
}

pub type DbResult<T> = Result<T, DbError>;

pub struct AppState {
    pub conn: Mutex<Option<Connection>>,
    pub path: Mutex<Option<PathBuf>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            conn: Mutex::new(None),
            path: Mutex::new(None),
        }
    }
}

pub fn open_connection(path: &Path) -> DbResult<Connection> {
    let conn = Connection::open(path)?;
    conn.execute_batch(
        "PRAGMA foreign_keys = ON;
         PRAGMA journal_mode = WAL;",
    )?;
    Ok(conn)
}

pub fn migrate(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS local_profile (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            display_name TEXT NOT NULL,
            password_hash TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS organization (
            id INTEGER PRIMARY KEY CHECK (id = 1),
            name TEXT NOT NULL,
            siren TEXT NOT NULL,
            legal_form TEXT NOT NULL DEFAULT 'SAS',
            vat_regime TEXT NOT NULL DEFAULT 'réel normal',
            address TEXT,
            capital_cents INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS fiscal_year (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            label TEXT NOT NULL,
            start_date TEXT NOT NULL,
            end_date TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'open'
                CHECK (status IN ('open','closing','closed'))
        );

        CREATE TABLE IF NOT EXISTS journal (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            journal_type TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS account (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            number TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            class INTEGER NOT NULL,
            account_type TEXT NOT NULL,
            is_auxiliary INTEGER NOT NULL DEFAULT 0,
            is_custom INTEGER NOT NULL DEFAULT 0,
            active INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS vat_code (
            code TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            rate_bp INTEGER NOT NULL,
            direction TEXT NOT NULL CHECK (direction IN ('collected','deductible','exempt')),
            active INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS third_party (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            name TEXT NOT NULL,
            kind TEXT NOT NULL CHECK (kind IN ('supplier','customer')),
            account_number TEXT NOT NULL,
            active INTEGER NOT NULL DEFAULT 1
        );

        CREATE TABLE IF NOT EXISTS entry (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            journal_id INTEGER NOT NULL REFERENCES journal(id),
            entry_date TEXT NOT NULL,
            piece_ref TEXT NOT NULL,
            label TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','posted')),
            fiscal_year_id INTEGER NOT NULL REFERENCES fiscal_year(id),
            ecriture_num TEXT,
            piece_date TEXT,
            ecriture_lettre TEXT,
            ecriture_date_lettre TEXT,
            validation_date TEXT,
            reversal_of_id INTEGER REFERENCES entry(id),
            created_at TEXT NOT NULL,
            posted_at TEXT
        );

        CREATE TABLE IF NOT EXISTS entry_line (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
            line_no INTEGER NOT NULL,
            account_id INTEGER NOT NULL REFERENCES account(id),
            label TEXT,
            debit_cents INTEGER NOT NULL DEFAULT 0 CHECK (debit_cents >= 0),
            credit_cents INTEGER NOT NULL DEFAULT 0 CHECK (credit_cents >= 0),
            vat_code TEXT REFERENCES vat_code(code),
            third_party_id INTEGER REFERENCES third_party(id),
            comp_aux_type TEXT,
            comp_aux_num TEXT,
            CHECK (debit_cents = 0 OR credit_cents = 0)
        );

        CREATE TABLE IF NOT EXISTS attachment (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            entry_id INTEGER NOT NULL REFERENCES entry(id) ON DELETE CASCADE,
            file_name TEXT NOT NULL,
            file_path TEXT NOT NULL,
            mime_type TEXT
        );
        "#,
    )?;

    assets::migrate_assets(conn)?;
    closing::migrate_closing(conn)?;

    let version: i32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if version < SCHEMA_VERSION {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
    }

    Ok(())
}

pub fn ensure_seeds(conn: &Connection) -> DbResult<()> {
    pcg::seed_journals(conn)?;
    pcg::seed_accounts(conn)?;
    tax::seed_vat_codes(conn)?;
    Ok(())
}

fn validate_siren(siren: &str) -> DbResult<()> {
    let digits: Vec<u32> = siren.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 9 {
        return Err("SIREN invalide (9 chiffres requis)".into());
    }
    // Luhn
    let mut sum = 0u32;
    for (i, d) in digits.iter().rev().enumerate() {
        let mut v = *d;
        if i % 2 == 1 {
            v *= 2;
            if v > 9 {
                v -= 9;
            }
        }
        sum += v;
    }
    if sum % 10 != 0 {
        return Err("SIREN invalide (clé de contrôle)".into());
    }
    Ok(())
}

pub fn create_workspace(input: &CreateWorkspaceInput) -> DbResult<WorkspaceInfo> {
    validate_siren(&input.siren)?;
    if input.password.len() < 8 {
        return Err("Mot de passe trop court (8 caractères min.)".into());
    }
    let path = PathBuf::from(&input.path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        return Err("Le fichier workspace existe déjà".into());
    }

    let conn = open_connection(&path)?;
    migrate(&conn)?;
    ensure_seeds(&conn)?;

    let hash = hash_password(&input.password)?;
    conn.execute(
        "INSERT INTO local_profile(id, display_name, password_hash) VALUES (1, ?1, ?2)",
        params![input.display_name, hash],
    )?;
    conn.execute(
        "INSERT INTO organization(id, name, siren, legal_form, vat_regime, capital_cents)
         VALUES (1, ?1, ?2, 'SAS', 'réel normal', 5000000)",
        params![input.org_name, input.siren],
    )?;
    conn.execute(
        "INSERT INTO fiscal_year(label, start_date, end_date, status) VALUES (?1, ?2, ?3, 'open')",
        params![input.fiscal_year_label, input.start_date, input.end_date],
    )?;

    load_workspace_info(&conn, &path)
}

pub fn open_workspace(path: &str, password: &str) -> DbResult<(Connection, WorkspaceInfo)> {
    let path_buf = PathBuf::from(path);
    if !path_buf.exists() {
        return Err("Fichier workspace introuvable".into());
    }
    let conn = open_connection(&path_buf)?;
    migrate(&conn)?;
    ensure_seeds(&conn)?;

    let hash: String = conn.query_row(
        "SELECT password_hash FROM local_profile WHERE id = 1",
        [],
        |r| r.get(0),
    )?;
    verify_password(password, &hash)?;
    let info = load_workspace_info(&conn, &path_buf)?;
    Ok((conn, info))
}

pub fn load_workspace_info(conn: &Connection, path: &Path) -> DbResult<WorkspaceInfo> {
    let organization = conn.query_row(
        "SELECT id, name, siren, legal_form, vat_regime, address, capital_cents FROM organization WHERE id = 1",
        [],
        |r| {
            Ok(Organization {
                id: r.get(0)?,
                name: r.get(1)?,
                siren: r.get(2)?,
                legal_form: r.get(3)?,
                vat_regime: r.get(4)?,
                address: r.get(5)?,
                capital_cents: r.get(6)?,
            })
        },
    )?;
    let fiscal_year = conn.query_row(
        "SELECT id, label, start_date, end_date, status FROM fiscal_year ORDER BY id DESC LIMIT 1",
        [],
        |r| {
            Ok(FiscalYear {
                id: r.get(0)?,
                label: r.get(1)?,
                start_date: r.get(2)?,
                end_date: r.get(3)?,
                status: r.get(4)?,
            })
        },
    )?;
    let profile = conn.query_row(
        "SELECT id, display_name FROM local_profile WHERE id = 1",
        [],
        |r| {
            Ok(LocalProfile {
                id: r.get(0)?,
                display_name: r.get(1)?,
            })
        },
    )?;
    Ok(WorkspaceInfo {
        path: path.display().to_string(),
        organization,
        fiscal_year,
        profile,
    })
}

pub fn create_demo_workspace(path: &Path) -> DbResult<WorkspaceInfo> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let input = CreateWorkspaceInput {
        path: path.display().to_string(),
        password: DEMO_PASSWORD.to_string(),
        org_name: "Demo SAS".to_string(),
        siren: "123456782".to_string(),
        fiscal_year_label: "2026".to_string(),
        start_date: "2026-01-01".to_string(),
        end_date: "2026-12-31".to_string(),
        display_name: "Dirigeant démo".to_string(),
    };
    let _info = create_workspace(&input)?;
    let conn = open_connection(path)?;
    tax::seed_demo_third_parties(&conn)?;
    ledger::seed_demo_entries(&conn)?;
    assets::seed_demo_assets(&conn)?;
    // Dotation 2026 via plan linéaire → OD posted (remplace l'ancien OD-01 hardcodé)
    let _ = assets::generate_period_depreciation(&conn, "2026", "2026-01-01", "2026-12-31")?;
    load_workspace_info(&conn, path)
}

pub fn with_conn<F, T>(state: &AppState, f: F) -> DbResult<T>
where
    F: FnOnce(&Connection) -> DbResult<T>,
{
    let guard = state.conn.lock().map_err(|e| e.to_string())?;
    let conn = guard.as_ref().ok_or("Aucun workspace ouvert")?;
    f(conn)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn workspace_create_open() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("test.sascompta");
        let input = CreateWorkspaceInput {
            path: path.display().to_string(),
            password: "SecretPass1".into(),
            org_name: "Demo SAS".into(),
            siren: "123456782".into(),
            fiscal_year_label: "2026".into(),
            start_date: "2026-01-01".into(),
            end_date: "2026-12-31".into(),
            display_name: "Test".into(),
        };
        let info = create_workspace(&input).unwrap();
        assert_eq!(info.organization.name, "Demo SAS");
        let (_conn, opened) = open_workspace(&path.display().to_string(), "SecretPass1").unwrap();
        assert_eq!(opened.organization.siren, "123456782");
        assert!(open_workspace(&path.display().to_string(), "bad").is_err());
    }

    #[test]
    fn demo_seed_has_ten_posted() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("demo.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM entry WHERE status = 'posted'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(n >= 10, "expected >=10 posted, got {n}");
    }
}

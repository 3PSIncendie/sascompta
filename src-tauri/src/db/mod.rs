pub mod assets;
pub mod billing;
pub mod chart;
pub mod closing;
pub mod einvoice;
pub mod hr;
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

pub const SCHEMA_VERSION: i32 = 12;
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
            direction TEXT NOT NULL CHECK (direction IN ('collected','deductible','exempt','reverse')),
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
    migrate_organization_siret(conn)?;
    billing::migrate_billing(conn)?;
    hr::migrate_hr(conn)?;
    einvoice::migrate_einvoice(conn)?;

    let version: i32 = conn
        .query_row(
            "SELECT value FROM meta WHERE key = 'schema_version'",
            [],
            |r| r.get::<_, String>(0),
        )
        .optional()?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    // v7 = SIRET (PR #5). v8 = direction reverse (autoliquidation BTP).
    if version < 8 {
        migrate_vat_code_reverse(conn)?;
    }
    // v9 = commercial (devis/factures) + tiers enrichis — via billing::migrate_billing
    // v10 = RH (salariés / contrats / congés / bulletins) — via hr::migrate_hr
    // v11 = e-facture (profil, queue, e-reporting) — via einvoice::migrate_einvoice
    // v12 = index unique numéro facture + API numérotation (document_sequence)

    if version < SCHEMA_VERSION {
        conn.execute(
            "INSERT INTO meta(key, value) VALUES('schema_version', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![SCHEMA_VERSION.to_string()],
        )?;
    }

    Ok(())
}

fn migrate_organization_siret(conn: &Connection) -> DbResult<()> {
    let mut stmt = conn.prepare("PRAGMA table_info(organization)")?;
    let cols: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .filter_map(|r| r.ok())
        .collect();
    if !cols.iter().any(|c| c == "siret") {
        conn.execute("ALTER TABLE organization ADD COLUMN siret TEXT", [])?;
    }
    Ok(())
}

/// Élargit la contrainte direction pour accepter `reverse` (autoliquidation BTP).
fn migrate_vat_code_reverse(conn: &Connection) -> DbResult<()> {
    // entry_line référence vat_code : il faut désactiver les FK le temps du rebuild.
    conn.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;
        CREATE TABLE vat_code_v8 (
            code TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            rate_bp INTEGER NOT NULL,
            direction TEXT NOT NULL CHECK (direction IN ('collected','deductible','exempt','reverse')),
            active INTEGER NOT NULL DEFAULT 1
        );
        INSERT INTO vat_code_v8(code, label, rate_bp, direction, active)
          SELECT code, label, rate_bp, direction, active FROM vat_code;
        DROP TABLE vat_code;
        ALTER TABLE vat_code_v8 RENAME TO vat_code;
        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

pub fn ensure_seeds(conn: &Connection) -> DbResult<()> {
    pcg::seed_journals(conn)?;
    pcg::seed_accounts(conn)?;
    tax::seed_vat_codes(conn)?;
    Ok(())
}

fn luhn_ok(digits: &[u32]) -> bool {
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
    sum % 10 == 0
}

fn validate_siren(siren: &str) -> DbResult<()> {
    let digits: Vec<u32> = siren.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 9 {
        return Err("SIREN invalide (9 chiffres requis)".into());
    }
    if !luhn_ok(&digits) {
        return Err("SIREN invalide (clé de contrôle)".into());
    }
    Ok(())
}

fn normalize_digits(raw: &str) -> String {
    raw.chars().filter(|c| c.is_ascii_digit()).collect()
}

fn validate_siret(siret: &str, siren: &str) -> DbResult<()> {
    let digits: Vec<u32> = siret.chars().filter_map(|c| c.to_digit(10)).collect();
    if digits.len() != 14 {
        return Err("SIRET invalide (14 chiffres requis)".into());
    }
    if !luhn_ok(&digits) {
        return Err("SIRET invalide (clé de contrôle)".into());
    }
    let siren_digits = normalize_digits(siren);
    let siret_siren: String = digits.iter().take(9).map(|d| char::from_digit(*d, 10).unwrap()).collect();
    if siret_siren != siren_digits {
        return Err("Le SIRET doit commencer par le SIREN saisi".into());
    }
    Ok(())
}

const ALLOWED_LEGAL_FORMS: &[&str] = &["SAS", "SASU"];
const ALLOWED_VAT_REGIMES: &[&str] = &["reel_mensuel", "reel_trimestriel", "franchise"];

fn validate_date_ymd(label: &str, value: &str) -> DbResult<()> {
    let parts: Vec<&str> = value.split('-').collect();
    if parts.len() != 3 {
        return Err(format!("{label} : format AAAA-MM-JJ attendu").into());
    }
    let y: i32 = parts[0]
        .parse()
        .map_err(|_| format!("{label} : année invalide"))?;
    let m: u32 = parts[1]
        .parse()
        .map_err(|_| format!("{label} : mois invalide"))?;
    let d: u32 = parts[2]
        .parse()
        .map_err(|_| format!("{label} : jour invalide"))?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || y < 1990 || y > 2100 {
        return Err(format!("{label} : date hors plage raisonnable").into());
    }
    Ok(())
}

pub fn create_workspace(input: &CreateWorkspaceInput) -> DbResult<WorkspaceInfo> {
    validate_siren(&input.siren)?;
    if input.password.len() < 8 {
        return Err("Mot de passe trop court (8 caractères min.)".into());
    }
    let path = match input.path.as_ref().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        Some(p) => PathBuf::from(p),
        None => {
            return Err(
                "Chemin workspace manquant (le backend doit résoudre le dossier app data)".into(),
            );
        }
    };
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
        "INSERT INTO organization(id, name, siren, siret, legal_form, vat_regime, capital_cents)
         VALUES (1, ?1, ?2, NULL, 'SAS', 'reel_mensuel', 5000000)",
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

fn map_organization(r: &rusqlite::Row<'_>) -> rusqlite::Result<Organization> {
    Ok(Organization {
        id: r.get(0)?,
        name: r.get(1)?,
        siren: r.get(2)?,
        siret: r.get(3)?,
        legal_form: r.get(4)?,
        vat_regime: r.get(5)?,
        address: r.get(6)?,
        capital_cents: r.get(7)?,
    })
}

pub fn load_organization(conn: &Connection) -> DbResult<Organization> {
    conn.query_row(
        "SELECT id, name, siren, siret, legal_form, vat_regime, address, capital_cents
         FROM organization WHERE id = 1",
        [],
        map_organization,
    )
    .map_err(|e| e.into())
}

pub fn load_workspace_info(conn: &Connection, path: &Path) -> DbResult<WorkspaceInfo> {
    let organization = load_organization(conn)?;
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

/// Met à jour la fiche société, le profil local et (si ouvert) l’exercice courant.
pub fn update_organization(
    conn: &Connection,
    path: &Path,
    input: &UpdateOrganizationInput,
) -> DbResult<WorkspaceInfo> {
    let name = input.name.trim();
    if name.len() < 2 {
        return Err("Indiquez la raison sociale (au moins 2 caractères)".into());
    }
    let siren = normalize_digits(&input.siren);
    validate_siren(&siren)?;

    let siret = match input.siret.as_ref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        Some(raw) => {
            let digits = normalize_digits(raw);
            validate_siret(&digits, &siren)?;
            Some(digits)
        }
        None => None,
    };

    let legal_form = input.legal_form.trim().to_uppercase();
    if !ALLOWED_LEGAL_FORMS.contains(&legal_form.as_str()) {
        return Err("Forme juridique : choisissez SAS ou SASU".into());
    }

    let vat_regime = input.vat_regime.trim().to_lowercase();
    if !ALLOWED_VAT_REGIMES.contains(&vat_regime.as_str()) {
        return Err(
            "Régime TVA : reel_mensuel, reel_trimestriel ou franchise".into(),
        );
    }

    if input.capital_cents < 0 {
        return Err("Le capital ne peut pas être négatif".into());
    }

    let display_name = input.display_name.trim();
    if display_name.len() < 2 {
        return Err("Indiquez un profil local (au moins 2 caractères)".into());
    }

    let address = input
        .address
        .as_ref()
        .map(|a| a.trim().to_string())
        .filter(|a| !a.is_empty());

    let fy_label = input.fiscal_year_label.trim();
    if fy_label.is_empty() {
        return Err("Indiquez le libellé d’exercice".into());
    }
    validate_date_ymd("Date de début", &input.start_date)?;
    validate_date_ymd("Date de fin", &input.end_date)?;
    if input.start_date > input.end_date {
        return Err("La date de début doit précéder la date de fin".into());
    }

    let current = load_workspace_info(conn, path)?;

    conn.execute(
        "UPDATE organization SET
            name = ?1,
            siren = ?2,
            siret = ?3,
            legal_form = ?4,
            vat_regime = ?5,
            address = ?6,
            capital_cents = ?7
         WHERE id = 1",
        params![
            name,
            siren,
            siret,
            legal_form,
            vat_regime,
            address,
            input.capital_cents
        ],
    )?;

    conn.execute(
        "UPDATE local_profile SET display_name = ?1 WHERE id = 1",
        params![display_name],
    )?;

    if current.fiscal_year.status == "open" {
        let n = conn.execute(
            "UPDATE fiscal_year SET label = ?1, start_date = ?2, end_date = ?3
             WHERE id = ?4 AND status = 'open'",
            params![
                fy_label,
                input.start_date,
                input.end_date,
                current.fiscal_year.id
            ],
        )?;
        if n == 0 {
            return Err("Exercice courant introuvable ou non ouvert".into());
        }
    } else if fy_label != current.fiscal_year.label
        || input.start_date != current.fiscal_year.start_date
        || input.end_date != current.fiscal_year.end_date
    {
        return Err(
            "L’exercice n’est plus ouvert : les dates ne peuvent pas être modifiées".into(),
        );
    }

    load_workspace_info(conn, path)
}

pub fn create_demo_workspace(path: &Path) -> DbResult<WorkspaceInfo> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    let input = CreateWorkspaceInput {
        path: Some(path.display().to_string()),
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
    billing::seed_demo_quotes(&conn)?;
    hr::seed_demo_hr(&conn)?;
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
            path: Some(path.display().to_string()),
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

    #[test]
    fn update_organization_fields() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("org.sascompta");
        let input = CreateWorkspaceInput {
            path: Some(path.display().to_string()),
            password: "SecretPass1".into(),
            org_name: "Demo SAS".into(),
            siren: "123456782".into(),
            fiscal_year_label: "2026".into(),
            start_date: "2026-01-01".into(),
            end_date: "2026-12-31".into(),
            display_name: "Test".into(),
        };
        create_workspace(&input).unwrap();
        let conn = open_connection(&path).unwrap();
        let updated = update_organization(
            &conn,
            &path,
            &UpdateOrganizationInput {
                name: "Atelier Lumière SAS".into(),
                siren: "123456782".into(),
                siret: Some("12345678200002".into()),
                legal_form: "SAS".into(),
                vat_regime: "reel_trimestriel".into(),
                address: Some("12 rue des Lilas\n75011 Paris".into()),
                capital_cents: 10_000_00,
                display_name: "Marie".into(),
                fiscal_year_label: "2026".into(),
                start_date: "2026-01-01".into(),
                end_date: "2026-12-31".into(),
            },
        )
        .unwrap();
        assert_eq!(updated.organization.name, "Atelier Lumière SAS");
        assert_eq!(
            updated.organization.siret.as_deref(),
            Some("12345678200002")
        );
        assert_eq!(updated.organization.vat_regime, "reel_trimestriel");
        assert_eq!(updated.organization.capital_cents, 10_000_00);
        assert_eq!(updated.profile.display_name, "Marie");
        assert!(updated.organization.address.as_ref().unwrap().contains("Lilas"));

        let bad = update_organization(
            &conn,
            &path,
            &UpdateOrganizationInput {
                name: "X".into(),
                siren: "123456782".into(),
                siret: Some("12345678200099".into()), // bad Luhn
                legal_form: "SAS".into(),
                vat_regime: "reel_mensuel".into(),
                address: None,
                capital_cents: 100,
                display_name: "Marie".into(),
                fiscal_year_label: "2026".into(),
                start_date: "2026-01-01".into(),
                end_date: "2026-12-31".into(),
            },
        );
        assert!(bad.is_err());
    }
}

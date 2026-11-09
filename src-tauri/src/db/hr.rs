//! Module RH — salariés, contrats, congés, bulletins simplifiés, OD paie (M8/M9).

use super::ledger;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::json;

pub const SCHEMA_HR_VERSION: i32 = 10;

/// Jours / heures stockés en « bp » : 100 = 1,00 (ex. 250 = 2,50 j ; 3500 = 35,00 h).
const DEFAULT_ACQUIRED_DAYS_BP: i64 = 2_500; // 25 j
const DEFAULT_EMPLOYEE_RATE_BP: i32 = 2_200; // ~22 % cotis. salariales agrégées
const DEFAULT_EMPLOYER_RATE_BP: i32 = 4_200; // ~42 % cotis. patronales agrégées

const CONTRACT_KINDS: &[&str] = &["cdi", "cdd", "apprentice"];
const CONTRACT_STATUSES: &[&str] = &["draft", "active", "ended"];
const LEAVE_STATUSES: &[&str] = &["pending", "approved", "refused", "cancelled"];

pub fn migrate_hr(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS employee (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            first_name TEXT NOT NULL,
            last_name TEXT NOT NULL,
            birth_date TEXT,
            address TEXT,
            email TEXT,
            iban_ref TEXT,
            active INTEGER NOT NULL DEFAULT 1,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS employment_contract (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id INTEGER NOT NULL REFERENCES employee(id) ON DELETE CASCADE,
            kind TEXT NOT NULL CHECK (kind IN ('cdi','cdd','apprentice')),
            start_date TEXT NOT NULL,
            end_date TEXT,
            job_title TEXT NOT NULL,
            weekly_hours_bp INTEGER NOT NULL DEFAULT 3500 CHECK (weekly_hours_bp > 0),
            gross_monthly_cents INTEGER NOT NULL CHECK (gross_monthly_cents >= 0),
            convention_ref TEXT,
            status TEXT NOT NULL DEFAULT 'active'
                CHECK (status IN ('draft','active','ended')),
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS leave_balance (
            employee_id INTEGER NOT NULL REFERENCES employee(id) ON DELETE CASCADE,
            year INTEGER NOT NULL,
            acquired_days_bp INTEGER NOT NULL DEFAULT 0,
            taken_days_bp INTEGER NOT NULL DEFAULT 0,
            PRIMARY KEY (employee_id, year)
        );

        CREATE TABLE IF NOT EXISTS leave_request (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id INTEGER NOT NULL REFERENCES employee(id) ON DELETE CASCADE,
            from_date TEXT NOT NULL,
            to_date TEXT NOT NULL,
            days_bp INTEGER NOT NULL CHECK (days_bp > 0),
            status TEXT NOT NULL DEFAULT 'pending'
                CHECK (status IN ('pending','approved','refused','cancelled')),
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS social_rate_table (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            valid_from TEXT NOT NULL UNIQUE,
            payload_json TEXT NOT NULL,
            label TEXT NOT NULL DEFAULT 'Barème agrégé',
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS payslip (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            employee_id INTEGER NOT NULL REFERENCES employee(id),
            contract_id INTEGER REFERENCES employment_contract(id),
            period_ym TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','validated')),
            gross_cents INTEGER NOT NULL DEFAULT 0,
            employee_charges_cents INTEGER NOT NULL DEFAULT 0,
            employer_charges_cents INTEGER NOT NULL DEFAULT 0,
            net_cents INTEGER NOT NULL DEFAULT 0,
            pas_rate_bp INTEGER,
            pas_cents INTEGER NOT NULL DEFAULT 0,
            entry_id INTEGER REFERENCES entry(id),
            pdf_path TEXT,
            notes TEXT,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(employee_id, period_ym)
        );

        CREATE TABLE IF NOT EXISTS payslip_line (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            payslip_id INTEGER NOT NULL REFERENCES payslip(id) ON DELETE CASCADE,
            line_no INTEGER NOT NULL,
            code TEXT NOT NULL,
            label TEXT NOT NULL,
            base_cents INTEGER NOT NULL DEFAULT 0,
            rate_bp INTEGER NOT NULL DEFAULT 0,
            employee_cents INTEGER NOT NULL DEFAULT 0,
            employer_cents INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS dsn_export_run (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            period_ym TEXT NOT NULL,
            status TEXT NOT NULL DEFAULT 'draft'
                CHECK (status IN ('draft','exported','checklist_only')),
            path TEXT,
            checklist_json TEXT NOT NULL,
            created_at TEXT NOT NULL
        );
        "#,
    )?;
    ensure_default_social_rates(conn)?;
    Ok(())
}

fn now_iso() -> String {
    Utc::now().to_rfc3339()
}

fn ensure_default_social_rates(conn: &Connection) -> DbResult<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM social_rate_table", [], |r| r.get(0))?;
    if count > 0 {
        return Ok(());
    }
    let payload = json!({
        "employee_rate_bp": DEFAULT_EMPLOYEE_RATE_BP,
        "employer_rate_bp": DEFAULT_EMPLOYER_RATE_BP,
        "disclaimer": "Barème agrégé local — pas un moteur Urssaf. À mettre à jour manuellement."
    });
    conn.execute(
        "INSERT INTO social_rate_table(valid_from, payload_json, label, created_at)
         VALUES ('2026-01-01', ?1, 'Barème agrégé MVP 2026', ?2)",
        params![payload.to_string(), now_iso()],
    )?;
    Ok(())
}

fn validate_code(code: &str) -> DbResult<()> {
    let t = code.trim();
    if t.is_empty() {
        return Err("Le code salarié est obligatoire".into());
    }
    if t.len() > 32 {
        return Err("Code salarié trop long (32 max)".into());
    }
    Ok(())
}

fn validate_period_ym(period_ym: &str) -> DbResult<()> {
    if period_ym.len() != 7 || period_ym.as_bytes().get(4) != Some(&b'-') {
        return Err("Période attendue au format AAAA-MM".into());
    }
    let y: i32 = period_ym[0..4]
        .parse()
        .map_err(|_| "Année de période invalide")?;
    let m: i32 = period_ym[5..7]
        .parse()
        .map_err(|_| "Mois de période invalide")?;
    if !(1..=12).contains(&m) || y < 2000 {
        return Err("Période AAAA-MM invalide".into());
    }
    Ok(())
}

fn parse_social_rates(payload: &str) -> DbResult<(i32, i32)> {
    let v: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| format!("Barème JSON invalide: {e}"))?;
    let emp = v
        .get("employee_rate_bp")
        .and_then(|x| x.as_i64())
        .unwrap_or(DEFAULT_EMPLOYEE_RATE_BP as i64) as i32;
    let pat = v
        .get("employer_rate_bp")
        .and_then(|x| x.as_i64())
        .unwrap_or(DEFAULT_EMPLOYER_RATE_BP as i64) as i32;
    Ok((emp.max(0), pat.max(0)))
}

fn apply_rate(base_cents: i64, rate_bp: i32) -> i64 {
    (base_cents * i64::from(rate_bp)) / 10_000
}

// —— Employees ————————————————————————————————————————————————————————————

fn map_employee(row: &rusqlite::Row<'_>) -> rusqlite::Result<Employee> {
    Ok(Employee {
        id: row.get(0)?,
        code: row.get(1)?,
        first_name: row.get(2)?,
        last_name: row.get(3)?,
        birth_date: row.get(4)?,
        address: row.get(5)?,
        email: row.get(6)?,
        iban_ref: row.get(7)?,
        active: row.get::<_, i32>(8)? != 0,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

pub fn list_employees(conn: &Connection) -> DbResult<Vec<Employee>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, first_name, last_name, birth_date, address, email, iban_ref,
                active, created_at, updated_at
         FROM employee ORDER BY last_name, first_name, code",
    )?;
    let rows = stmt.query_map([], map_employee)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_employee(conn: &Connection, id: i64) -> DbResult<Employee> {
    conn.query_row(
        "SELECT id, code, first_name, last_name, birth_date, address, email, iban_ref,
                active, created_at, updated_at
         FROM employee WHERE id = ?1",
        params![id],
        map_employee,
    )
    .map_err(|_| format!("Salarié #{id} introuvable").into())
}

pub fn create_employee(conn: &Connection, input: &CreateEmployeeInput) -> DbResult<Employee> {
    validate_code(&input.code)?;
    if input.first_name.trim().is_empty() || input.last_name.trim().is_empty() {
        return Err("Prénom et nom sont obligatoires".into());
    }
    let now = now_iso();
    conn.execute(
        "INSERT INTO employee(code, first_name, last_name, birth_date, address, email, iban_ref,
            active, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,1,?8,?8)",
        params![
            input.code.trim(),
            input.first_name.trim(),
            input.last_name.trim(),
            input.birth_date,
            input.address,
            input.email,
            input.iban_ref,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    // Solde congés année courante (simplifié)
    let year: i32 = Utc::now().format("%Y").to_string().parse().unwrap_or(2026);
    conn.execute(
        "INSERT OR IGNORE INTO leave_balance(employee_id, year, acquired_days_bp, taken_days_bp)
         VALUES (?1, ?2, ?3, 0)",
        params![id, year, DEFAULT_ACQUIRED_DAYS_BP],
    )?;
    get_employee(conn, id)
}

pub fn update_employee(
    conn: &Connection,
    id: i64,
    input: &CreateEmployeeInput,
) -> DbResult<Employee> {
    let _ = get_employee(conn, id)?;
    validate_code(&input.code)?;
    if input.first_name.trim().is_empty() || input.last_name.trim().is_empty() {
        return Err("Prénom et nom sont obligatoires".into());
    }
    conn.execute(
        "UPDATE employee SET code = ?1, first_name = ?2, last_name = ?3, birth_date = ?4,
         address = ?5, email = ?6, iban_ref = ?7, active = ?8, updated_at = ?9
         WHERE id = ?10",
        params![
            input.code.trim(),
            input.first_name.trim(),
            input.last_name.trim(),
            input.birth_date,
            input.address,
            input.email,
            input.iban_ref,
            if input.active { 1 } else { 0 },
            now_iso(),
            id
        ],
    )?;
    get_employee(conn, id)
}

pub fn delete_employee(conn: &Connection, id: i64) -> DbResult<()> {
    let slips: i64 = conn.query_row(
        "SELECT COUNT(*) FROM payslip WHERE employee_id = ?1 AND status = 'validated'",
        params![id],
        |r| r.get(0),
    )?;
    if slips > 0 {
        return Err(
            "Impossible de supprimer : des bulletins validés existent (désactivez le salarié)"
                .into(),
        );
    }
    let n = conn.execute("DELETE FROM employee WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(format!("Salarié #{id} introuvable").into());
    }
    Ok(())
}

// —— Contracts ————————————————————————————————————————————————————————————

fn map_contract(row: &rusqlite::Row<'_>) -> rusqlite::Result<EmploymentContract> {
    Ok(EmploymentContract {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        employee_code: row.get(2)?,
        employee_name: row.get(3)?,
        kind: row.get(4)?,
        start_date: row.get(5)?,
        end_date: row.get(6)?,
        job_title: row.get(7)?,
        weekly_hours_bp: row.get(8)?,
        gross_monthly_cents: row.get(9)?,
        convention_ref: row.get(10)?,
        status: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

const CONTRACT_SELECT: &str = "
    SELECT c.id, c.employee_id, e.code, e.first_name || ' ' || e.last_name,
           c.kind, c.start_date, c.end_date, c.job_title, c.weekly_hours_bp,
           c.gross_monthly_cents, c.convention_ref, c.status, c.created_at, c.updated_at
    FROM employment_contract c
    JOIN employee e ON e.id = c.employee_id";

pub fn list_contracts(
    conn: &Connection,
    employee_id: Option<i64>,
) -> DbResult<Vec<EmploymentContract>> {
    if let Some(eid) = employee_id {
        let mut stmt = conn.prepare(&format!(
            "{CONTRACT_SELECT} WHERE c.employee_id = ?1 ORDER BY c.start_date DESC, c.id DESC"
        ))?;
        let rows = stmt.query_map(params![eid], map_contract)?;
        return Ok(rows.filter_map(|r| r.ok()).collect());
    }
    let mut stmt = conn.prepare(&format!(
        "{CONTRACT_SELECT} ORDER BY e.last_name, c.start_date DESC"
    ))?;
    let rows = stmt.query_map([], map_contract)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn get_contract(conn: &Connection, id: i64) -> DbResult<EmploymentContract> {
    conn.query_row(
        &format!("{CONTRACT_SELECT} WHERE c.id = ?1"),
        params![id],
        map_contract,
    )
    .map_err(|_| format!("Contrat #{id} introuvable").into())
}

fn validate_contract_input(input: &CreateContractInput) -> DbResult<()> {
    if !CONTRACT_KINDS.contains(&input.kind.as_str()) {
        return Err(format!("Type de contrat invalide: {}", input.kind).into());
    }
    if input.job_title.trim().is_empty() {
        return Err("Intitulé de poste obligatoire".into());
    }
    if input.weekly_hours_bp <= 0 {
        return Err("Horaires hebdomadaires invalides".into());
    }
    if input.gross_monthly_cents < 0 {
        return Err("Salaire brut mensuel invalide".into());
    }
    if let Some(end) = &input.end_date {
        if end.as_str() < input.start_date.as_str() {
            return Err("La date de fin doit être ≥ date de début".into());
        }
    }
    if input.kind != "cdi" && input.end_date.is_none() {
        return Err("Un CDD / apprentissage nécessite une date de fin".into());
    }
    Ok(())
}

pub fn create_contract(
    conn: &Connection,
    input: &CreateContractInput,
) -> DbResult<EmploymentContract> {
    let _ = get_employee(conn, input.employee_id)?;
    validate_contract_input(input)?;
    let status = if CONTRACT_STATUSES.contains(&input.status.as_str()) {
        input.status.as_str()
    } else {
        "active"
    };
    let now = now_iso();
    conn.execute(
        "INSERT INTO employment_contract(
            employee_id, kind, start_date, end_date, job_title, weekly_hours_bp,
            gross_monthly_cents, convention_ref, status, created_at, updated_at)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?10)",
        params![
            input.employee_id,
            input.kind,
            input.start_date,
            input.end_date,
            input.job_title.trim(),
            input.weekly_hours_bp,
            input.gross_monthly_cents,
            input.convention_ref,
            status,
            now
        ],
    )?;
    get_contract(conn, conn.last_insert_rowid())
}

pub fn update_contract(
    conn: &Connection,
    id: i64,
    input: &CreateContractInput,
) -> DbResult<EmploymentContract> {
    let existing = get_contract(conn, id)?;
    if existing.employee_id != input.employee_id {
        return Err("Changement de salarié interdit sur un contrat".into());
    }
    validate_contract_input(input)?;
    let status = if CONTRACT_STATUSES.contains(&input.status.as_str()) {
        input.status.as_str()
    } else {
        existing.status.as_str()
    };
    conn.execute(
        "UPDATE employment_contract SET kind = ?1, start_date = ?2, end_date = ?3,
         job_title = ?4, weekly_hours_bp = ?5, gross_monthly_cents = ?6,
         convention_ref = ?7, status = ?8, updated_at = ?9 WHERE id = ?10",
        params![
            input.kind,
            input.start_date,
            input.end_date,
            input.job_title.trim(),
            input.weekly_hours_bp,
            input.gross_monthly_cents,
            input.convention_ref,
            status,
            now_iso(),
            id
        ],
    )?;
    get_contract(conn, id)
}

pub fn delete_contract(conn: &Connection, id: i64) -> DbResult<()> {
    let used: i64 = conn.query_row(
        "SELECT COUNT(*) FROM payslip WHERE contract_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err("Contrat lié à des bulletins — marquez-le terminé plutôt".into());
    }
    let n = conn.execute("DELETE FROM employment_contract WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(format!("Contrat #{id} introuvable").into());
    }
    Ok(())
}

pub fn active_contract_for(
    conn: &Connection,
    employee_id: i64,
    on_date: &str,
) -> DbResult<Option<EmploymentContract>> {
    let mut stmt = conn.prepare(&format!(
        "{CONTRACT_SELECT}
         WHERE c.employee_id = ?1 AND c.status = 'active'
           AND c.start_date <= ?2
           AND (c.end_date IS NULL OR c.end_date >= ?2)
         ORDER BY c.start_date DESC LIMIT 1"
    ))?;
    Ok(stmt.query_row(params![employee_id, on_date], map_contract).optional()?)
}

// —— Leave ————————————————————————————————————————————————————————————————

fn map_leave_balance(row: &rusqlite::Row<'_>) -> rusqlite::Result<LeaveBalance> {
    let acquired: i64 = row.get(4)?;
    let taken: i64 = row.get(5)?;
    Ok(LeaveBalance {
        employee_id: row.get(0)?,
        employee_code: row.get(1)?,
        employee_name: row.get(2)?,
        year: row.get(3)?,
        acquired_days_bp: acquired,
        taken_days_bp: taken,
        remaining_days_bp: acquired - taken,
    })
}

pub fn list_leave_balances(conn: &Connection, year: Option<i32>) -> DbResult<Vec<LeaveBalance>> {
    let y = year.unwrap_or_else(|| {
        Utc::now()
            .format("%Y")
            .to_string()
            .parse()
            .unwrap_or(2026)
    });
    // Ensure balances exist for active employees
    let employees = list_employees(conn)?;
    for e in employees.iter().filter(|e| e.active) {
        conn.execute(
            "INSERT OR IGNORE INTO leave_balance(employee_id, year, acquired_days_bp, taken_days_bp)
             VALUES (?1, ?2, ?3, 0)",
            params![e.id, y, DEFAULT_ACQUIRED_DAYS_BP],
        )?;
    }
    let mut stmt = conn.prepare(
        "SELECT b.employee_id, e.code, e.first_name || ' ' || e.last_name,
                b.year, b.acquired_days_bp, b.taken_days_bp
         FROM leave_balance b
         JOIN employee e ON e.id = b.employee_id
         WHERE b.year = ?1
         ORDER BY e.last_name, e.first_name",
    )?;
    let rows = stmt.query_map(params![y], map_leave_balance)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn upsert_leave_balance(
    conn: &Connection,
    employee_id: i64,
    year: i32,
    acquired_days_bp: i64,
) -> DbResult<LeaveBalance> {
    let _ = get_employee(conn, employee_id)?;
    if acquired_days_bp < 0 {
        return Err("Jours acquis invalides".into());
    }
    conn.execute(
        "INSERT INTO leave_balance(employee_id, year, acquired_days_bp, taken_days_bp)
         VALUES (?1, ?2, ?3, 0)
         ON CONFLICT(employee_id, year) DO UPDATE SET acquired_days_bp = excluded.acquired_days_bp",
        params![employee_id, year, acquired_days_bp],
    )?;
    list_leave_balances(conn, Some(year))?
        .into_iter()
        .find(|b| b.employee_id == employee_id)
        .ok_or_else(|| "Solde introuvable après upsert".into())
}

fn map_leave_request(row: &rusqlite::Row<'_>) -> rusqlite::Result<LeaveRequest> {
    Ok(LeaveRequest {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        employee_code: row.get(2)?,
        employee_name: row.get(3)?,
        from_date: row.get(4)?,
        to_date: row.get(5)?,
        days_bp: row.get(6)?,
        status: row.get(7)?,
        notes: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

const LEAVE_SELECT: &str = "
    SELECT r.id, r.employee_id, e.code, e.first_name || ' ' || e.last_name,
           r.from_date, r.to_date, r.days_bp, r.status, r.notes, r.created_at, r.updated_at
    FROM leave_request r
    JOIN employee e ON e.id = r.employee_id";

pub fn list_leave_requests(
    conn: &Connection,
    employee_id: Option<i64>,
) -> DbResult<Vec<LeaveRequest>> {
    if let Some(eid) = employee_id {
        let mut stmt = conn.prepare(&format!(
            "{LEAVE_SELECT} WHERE r.employee_id = ?1 ORDER BY r.from_date DESC, r.id DESC"
        ))?;
        let rows = stmt.query_map(params![eid], map_leave_request)?;
        return Ok(rows.filter_map(|r| r.ok()).collect());
    }
    let mut stmt = conn.prepare(&format!(
        "{LEAVE_SELECT} ORDER BY r.from_date DESC, r.id DESC"
    ))?;
    let rows = stmt.query_map([], map_leave_request)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_leave_request(
    conn: &Connection,
    input: &CreateLeaveRequestInput,
) -> DbResult<LeaveRequest> {
    let _ = get_employee(conn, input.employee_id)?;
    if input.days_bp <= 0 {
        return Err("Nombre de jours invalide".into());
    }
    if input.to_date.as_str() < input.from_date.as_str() {
        return Err("La date de fin doit être ≥ date de début".into());
    }
    let now = now_iso();
    conn.execute(
        "INSERT INTO leave_request(employee_id, from_date, to_date, days_bp, status, notes,
            created_at, updated_at)
         VALUES (?1,?2,?3,?4,'pending',?5,?6,?6)",
        params![
            input.employee_id,
            input.from_date,
            input.to_date,
            input.days_bp,
            input.notes,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    conn.query_row(
        &format!("{LEAVE_SELECT} WHERE r.id = ?1"),
        params![id],
        map_leave_request,
    )
    .map_err(|e| e.into())
}

pub fn set_leave_status(
    conn: &Connection,
    id: i64,
    status: &str,
) -> DbResult<LeaveRequest> {
    if !LEAVE_STATUSES.contains(&status) {
        return Err(format!("Statut congé invalide: {status}").into());
    }
    let current: (i64, i64, String, String) = conn.query_row(
        "SELECT employee_id, days_bp, status, from_date FROM leave_request WHERE id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )?;
    let (employee_id, days_bp, old_status, from_date) = current;
    if old_status == status {
        return conn
            .query_row(
                &format!("{LEAVE_SELECT} WHERE r.id = ?1"),
                params![id],
                map_leave_request,
            )
            .map_err(|e| e.into());
    }

    let year: i32 = from_date
        .get(0..4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2026);
    conn.execute(
        "INSERT OR IGNORE INTO leave_balance(employee_id, year, acquired_days_bp, taken_days_bp)
         VALUES (?1, ?2, ?3, 0)",
        params![employee_id, year, DEFAULT_ACQUIRED_DAYS_BP],
    )?;

    // Adjust taken days when entering/leaving approved
    if old_status == "approved" && status != "approved" {
        conn.execute(
            "UPDATE leave_balance SET taken_days_bp = MAX(0, taken_days_bp - ?1)
             WHERE employee_id = ?2 AND year = ?3",
            params![days_bp, employee_id, year],
        )?;
    } else if old_status != "approved" && status == "approved" {
        let bal: (i64, i64) = conn.query_row(
            "SELECT acquired_days_bp, taken_days_bp FROM leave_balance
             WHERE employee_id = ?1 AND year = ?2",
            params![employee_id, year],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;
        if bal.1 + days_bp > bal.0 {
            return Err(format!(
                "Solde insuffisant : reste {:.2} j, demandé {:.2} j",
                (bal.0 - bal.1) as f64 / 100.0,
                days_bp as f64 / 100.0
            )
            .into());
        }
        conn.execute(
            "UPDATE leave_balance SET taken_days_bp = taken_days_bp + ?1
             WHERE employee_id = ?2 AND year = ?3",
            params![days_bp, employee_id, year],
        )?;
    }

    conn.execute(
        "UPDATE leave_request SET status = ?1, updated_at = ?2 WHERE id = ?3",
        params![status, now_iso(), id],
    )?;
    conn.query_row(
        &format!("{LEAVE_SELECT} WHERE r.id = ?1"),
        params![id],
        map_leave_request,
    )
    .map_err(|e| e.into())
}

pub fn delete_leave_request(conn: &Connection, id: i64) -> DbResult<()> {
    let status: String = conn
        .query_row(
            "SELECT status FROM leave_request WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )
        .map_err(|_| format!("Demande #{id} introuvable"))?;
    if status == "approved" {
        set_leave_status(conn, id, "cancelled")?;
    }
    conn.execute("DELETE FROM leave_request WHERE id = ?1", params![id])?;
    Ok(())
}

// —— Social rates / payslips ——————————————————————————————————————————————

fn map_social_rate(row: &rusqlite::Row<'_>) -> rusqlite::Result<SocialRateTable> {
    Ok(SocialRateTable {
        id: row.get(0)?,
        valid_from: row.get(1)?,
        payload_json: row.get(2)?,
        label: row.get(3)?,
        created_at: row.get(4)?,
    })
}

pub fn list_social_rates(conn: &Connection) -> DbResult<Vec<SocialRateTable>> {
    let mut stmt = conn.prepare(
        "SELECT id, valid_from, payload_json, label, created_at
         FROM social_rate_table ORDER BY valid_from DESC",
    )?;
    let rows = stmt.query_map([], map_social_rate)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn current_social_rates(conn: &Connection, as_of: &str) -> DbResult<(i32, i32)> {
    let payload: String = conn
        .query_row(
            "SELECT payload_json FROM social_rate_table
             WHERE valid_from <= ?1 ORDER BY valid_from DESC LIMIT 1",
            params![as_of],
            |r| r.get(0),
        )
        .optional()?
        .unwrap_or_else(|| {
            json!({
                "employee_rate_bp": DEFAULT_EMPLOYEE_RATE_BP,
                "employer_rate_bp": DEFAULT_EMPLOYER_RATE_BP
            })
            .to_string()
        });
    parse_social_rates(&payload)
}

pub fn upsert_social_rate(
    conn: &Connection,
    valid_from: &str,
    employee_rate_bp: i32,
    employer_rate_bp: i32,
    label: &str,
) -> DbResult<SocialRateTable> {
    if employee_rate_bp < 0 || employer_rate_bp < 0 {
        return Err("Taux négatifs interdits".into());
    }
    let payload = json!({
        "employee_rate_bp": employee_rate_bp,
        "employer_rate_bp": employer_rate_bp,
        "disclaimer": "Barème agrégé local — pas un moteur Urssaf."
    });
    conn.execute(
        "INSERT INTO social_rate_table(valid_from, payload_json, label, created_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(valid_from) DO UPDATE SET
           payload_json = excluded.payload_json,
           label = excluded.label",
        params![valid_from, payload.to_string(), label, now_iso()],
    )?;
    conn.query_row(
        "SELECT id, valid_from, payload_json, label, created_at
         FROM social_rate_table WHERE valid_from = ?1",
        params![valid_from],
        map_social_rate,
    )
    .map_err(|e| e.into())
}

fn map_payslip_line(row: &rusqlite::Row<'_>) -> rusqlite::Result<PayslipLine> {
    Ok(PayslipLine {
        id: row.get(0)?,
        payslip_id: row.get(1)?,
        line_no: row.get(2)?,
        code: row.get(3)?,
        label: row.get(4)?,
        base_cents: row.get(5)?,
        rate_bp: row.get(6)?,
        employee_cents: row.get(7)?,
        employer_cents: row.get(8)?,
    })
}

fn load_payslip_lines(conn: &Connection, payslip_id: i64) -> DbResult<Vec<PayslipLine>> {
    let mut stmt = conn.prepare(
        "SELECT id, payslip_id, line_no, code, label, base_cents, rate_bp,
                employee_cents, employer_cents
         FROM payslip_line WHERE payslip_id = ?1 ORDER BY line_no",
    )?;
    let rows = stmt.query_map(params![payslip_id], map_payslip_line)?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

fn map_payslip(row: &rusqlite::Row<'_>) -> rusqlite::Result<Payslip> {
    Ok(Payslip {
        id: row.get(0)?,
        employee_id: row.get(1)?,
        employee_code: row.get(2)?,
        employee_name: row.get(3)?,
        contract_id: row.get(4)?,
        period_ym: row.get(5)?,
        status: row.get(6)?,
        gross_cents: row.get(7)?,
        employee_charges_cents: row.get(8)?,
        employer_charges_cents: row.get(9)?,
        net_cents: row.get(10)?,
        pas_rate_bp: row.get(11)?,
        pas_cents: row.get(12)?,
        entry_id: row.get(13)?,
        pdf_path: row.get(14)?,
        notes: row.get(15)?,
        created_at: row.get(16)?,
        updated_at: row.get(17)?,
        lines: Vec::new(),
    })
}

const PAYSLIP_SELECT: &str = "
    SELECT p.id, p.employee_id, e.code, e.first_name || ' ' || e.last_name,
           p.contract_id, p.period_ym, p.status, p.gross_cents, p.employee_charges_cents,
           p.employer_charges_cents, p.net_cents, p.pas_rate_bp, p.pas_cents,
           p.entry_id, p.pdf_path, p.notes, p.created_at, p.updated_at
    FROM payslip p
    JOIN employee e ON e.id = p.employee_id";

pub fn get_payslip(conn: &Connection, id: i64) -> DbResult<Payslip> {
    let mut p = conn
        .query_row(
            &format!("{PAYSLIP_SELECT} WHERE p.id = ?1"),
            params![id],
            map_payslip,
        )
        .map_err(|_| format!("Bulletin #{id} introuvable"))?;
    p.lines = load_payslip_lines(conn, id)?;
    Ok(p)
}

pub fn list_payslips(
    conn: &Connection,
    period_ym: Option<&str>,
) -> DbResult<Vec<Payslip>> {
    let mut out = Vec::new();
    if let Some(ym) = period_ym {
        validate_period_ym(ym)?;
        let mut stmt = conn.prepare(&format!(
            "{PAYSLIP_SELECT} WHERE p.period_ym = ?1 ORDER BY e.last_name, e.first_name"
        ))?;
        for row in stmt.query_map(params![ym], map_payslip)? {
            let mut p = row?;
            p.lines = load_payslip_lines(conn, p.id)?;
            out.push(p);
        }
    } else {
        let mut stmt = conn.prepare(&format!(
            "{PAYSLIP_SELECT} ORDER BY p.period_ym DESC, e.last_name"
        ))?;
        for row in stmt.query_map([], map_payslip)? {
            let mut p = row?;
            p.lines = load_payslip_lines(conn, p.id)?;
            out.push(p);
        }
    }
    Ok(out)
}

fn replace_payslip_lines(conn: &Connection, payslip_id: i64, lines: &[PayslipLine]) -> DbResult<()> {
    conn.execute(
        "DELETE FROM payslip_line WHERE payslip_id = ?1",
        params![payslip_id],
    )?;
    for (i, line) in lines.iter().enumerate() {
        conn.execute(
            "INSERT INTO payslip_line(payslip_id, line_no, code, label, base_cents, rate_bp,
                employee_cents, employer_cents)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                payslip_id,
                (i + 1) as i32,
                line.code,
                line.label,
                line.base_cents,
                line.rate_bp,
                line.employee_cents,
                line.employer_cents
            ],
        )?;
    }
    Ok(())
}

/// Calcule un bulletin brouillon à partir du contrat actif + barème.
pub fn compute_payslip(
    conn: &Connection,
    employee_id: i64,
    period_ym: &str,
    pas_rate_bp: Option<i32>,
) -> DbResult<Payslip> {
    validate_period_ym(period_ym)?;
    let _emp = get_employee(conn, employee_id)?;
    let as_of = format!("{period_ym}-01");
    let contract = active_contract_for(conn, employee_id, &as_of)?.ok_or_else(|| {
        format!("Aucun contrat actif pour le salarié #{employee_id} sur {period_ym}")
    })?;
    let (emp_rate, pat_rate) = current_social_rates(conn, &as_of)?;
    let gross = contract.gross_monthly_cents;
    let emp_charges = apply_rate(gross, emp_rate);
    let pat_charges = apply_rate(gross, pat_rate);
    let pas_rate = pas_rate_bp.unwrap_or(0).max(0);
    let taxable_net = gross - emp_charges;
    let pas = apply_rate(taxable_net.max(0), pas_rate);
    let net = taxable_net - pas;

    let lines = vec![
        PayslipLine {
            id: 0,
            payslip_id: 0,
            line_no: 1,
            code: "BRUT".into(),
            label: "Salaire brut".into(),
            base_cents: gross,
            rate_bp: 0,
            employee_cents: gross,
            employer_cents: 0,
        },
        PayslipLine {
            id: 0,
            payslip_id: 0,
            line_no: 2,
            code: "COTIS_SAL".into(),
            label: "Cotisations salariales (agrégées)".into(),
            base_cents: gross,
            rate_bp: emp_rate,
            employee_cents: -emp_charges,
            employer_cents: 0,
        },
        PayslipLine {
            id: 0,
            payslip_id: 0,
            line_no: 3,
            code: "COTIS_PAT".into(),
            label: "Cotisations patronales (agrégées)".into(),
            base_cents: gross,
            rate_bp: pat_rate,
            employee_cents: 0,
            employer_cents: pat_charges,
        },
        PayslipLine {
            id: 0,
            payslip_id: 0,
            line_no: 4,
            code: "PAS".into(),
            label: "Prélèvement à la source".into(),
            base_cents: taxable_net.max(0),
            rate_bp: pas_rate,
            employee_cents: -pas,
            employer_cents: 0,
        },
        PayslipLine {
            id: 0,
            payslip_id: 0,
            line_no: 5,
            code: "NET".into(),
            label: "Net à payer".into(),
            base_cents: net,
            rate_bp: 0,
            employee_cents: net,
            employer_cents: 0,
        },
    ];

    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM payslip WHERE employee_id = ?1 AND period_ym = ?2",
            params![employee_id, period_ym],
            |r| r.get(0),
        )
        .optional()?;

    let now = now_iso();
    let id = if let Some(pid) = existing {
        let status: String = conn.query_row(
            "SELECT status FROM payslip WHERE id = ?1",
            params![pid],
            |r| r.get(0),
        )?;
        if status == "validated" {
            return Err("Bulletin déjà validé — recalcul interdit".into());
        }
        // Keep existing entry_id if draft linked — will be rebuilt on OD gen
        conn.execute(
            "UPDATE payslip SET contract_id = ?1, gross_cents = ?2, employee_charges_cents = ?3,
             employer_charges_cents = ?4, net_cents = ?5, pas_rate_bp = ?6, pas_cents = ?7,
             updated_at = ?8 WHERE id = ?9",
            params![
                contract.id,
                gross,
                emp_charges,
                pat_charges,
                net,
                if pas_rate > 0 { Some(pas_rate) } else { None::<i32> },
                pas,
                now,
                pid
            ],
        )?;
        replace_payslip_lines(conn, pid, &lines)?;
        pid
    } else {
        conn.execute(
            "INSERT INTO payslip(
                employee_id, contract_id, period_ym, status, gross_cents,
                employee_charges_cents, employer_charges_cents, net_cents,
                pas_rate_bp, pas_cents, notes, created_at, updated_at)
             VALUES (?1,?2,?3,'draft',?4,?5,?6,?7,?8,?9,NULL,?10,?10)",
            params![
                employee_id,
                contract.id,
                period_ym,
                gross,
                emp_charges,
                pat_charges,
                net,
                if pas_rate > 0 { Some(pas_rate) } else { None::<i32> },
                pas,
                now
            ],
        )?;
        let pid = conn.last_insert_rowid();
        replace_payslip_lines(conn, pid, &lines)?;
        pid
    };
    get_payslip(conn, id)
}

pub fn generate_period_payslips(
    conn: &Connection,
    period_ym: &str,
) -> DbResult<Vec<Payslip>> {
    validate_period_ym(period_ym)?;
    let as_of = format!("{period_ym}-01");
    let mut out = Vec::new();
    for emp in list_employees(conn)?.into_iter().filter(|e| e.active) {
        if active_contract_for(conn, emp.id, &as_of)?.is_some() {
            match compute_payslip(conn, emp.id, period_ym, None) {
                Ok(p) => out.push(p),
                Err(e) => {
                    // skip validated duplicates silently for batch
                    if !e.to_string().contains("déjà validé") {
                        return Err(e);
                    }
                }
            }
        }
    }
    Ok(out)
}

pub fn validate_payslip(conn: &Connection, id: i64) -> DbResult<Payslip> {
    let p = get_payslip(conn, id)?;
    if p.status == "validated" {
        return Ok(p);
    }
    if p.gross_cents <= 0 || p.net_cents < 0 {
        return Err("Bulletin incohérent (brut/net)".into());
    }
    conn.execute(
        "UPDATE payslip SET status = 'validated', updated_at = ?1 WHERE id = ?2",
        params![now_iso(), id],
    )?;
    get_payslip(conn, id)
}

pub fn delete_payslip(conn: &Connection, id: i64) -> DbResult<()> {
    let p = get_payslip(conn, id)?;
    if p.status == "validated" {
        return Err("Bulletin validé — suppression interdite".into());
    }
    if let Some(entry_id) = p.entry_id {
        let status: String = conn.query_row(
            "SELECT status FROM entry WHERE id = ?1",
            params![entry_id],
            |r| r.get(0),
        )?;
        if status == "draft" {
            let _ = ledger::delete_draft(conn, entry_id);
        } else {
            return Err("Écriture liée déjà validée — suppression interdite".into());
        }
    }
    conn.execute("DELETE FROM payslip WHERE id = ?1", params![id])?;
    Ok(())
}

/// OD draft : D 641 brut / C 421 net / C 431 cotis sal. / D 645 + C 431 cotis pat. (+ C 421 PAS si présent agrégé dans net).
/// PAS est déjà déduit du net : on crédite 421 du net à payer uniquement.
pub fn build_payslip_entry_draft(conn: &Connection, payslip_id: i64) -> DbResult<Entry> {
    super::closing::assert_has_open_fy(conn)?;
    let p = get_payslip(conn, payslip_id)?;
    if p.status != "validated" {
        return Err("Validez le bulletin avant de générer l’écriture OD".into());
    }
    if p.gross_cents <= 0 {
        return Err("Brut nul — pas d’écriture".into());
    }

    let piece = format!("PAIE-{}-{}", p.period_ym, p.employee_code);
    let label = format!(
        "Paie {} — {}",
        p.period_ym, p.employee_name
    );
    let entry_date = {
        // dernier jour approximatif du mois
        let y: i32 = p.period_ym[0..4].parse().unwrap_or(2026);
        let m: u32 = p.period_ym[5..7].parse().unwrap_or(1);
        let last = match m {
            2 => {
                if y % 4 == 0 && (y % 100 != 0 || y % 400 == 0) {
                    29
                } else {
                    28
                }
            }
            4 | 6 | 9 | 11 => 30,
            _ => 31,
        };
        format!("{y:04}-{m:02}-{last:02}")
    };

    let mut lines = vec![
        EntryLineInput {
            account_number: "641".into(),
            label: Some(format!("Rémunération {}", p.employee_code)),
            debit_cents: p.gross_cents,
            credit_cents: 0,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        },
        EntryLineInput {
            account_number: "421".into(),
            label: Some(format!("Net à payer {}", p.employee_code)),
            debit_cents: 0,
            credit_cents: p.net_cents + p.pas_cents, // net + PAS (retenue) = brut − cotis sal.
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        },
    ];
    if p.employee_charges_cents > 0 {
        lines.push(EntryLineInput {
            account_number: "431".into(),
            label: Some("Cotisations salariales".into()),
            debit_cents: 0,
            credit_cents: p.employee_charges_cents,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }
    // Employer charges: D 645 / C 431
    if p.employer_charges_cents > 0 {
        lines.push(EntryLineInput {
            account_number: "645".into(),
            label: Some("Charges patronales".into()),
            debit_cents: p.employer_charges_cents,
            credit_cents: 0,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
        lines.push(EntryLineInput {
            account_number: "431".into(),
            label: Some("Cotisations patronales à payer".into()),
            debit_cents: 0,
            credit_cents: p.employer_charges_cents,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }

    // Note: 421 credit = net + pas (= gross - employee_charges). PAS stays in 421 until payment/treasury later.
    let input = CreateEntryInput {
        journal_code: "OD".into(),
        entry_date,
        piece_ref: piece,
        label,
        lines,
    };

    if let Some(entry_id) = p.entry_id {
        let status: String = conn
            .query_row(
                "SELECT status FROM entry WHERE id = ?1",
                params![entry_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "missing".into());
        if status == "posted" {
            return Err(
                "Écriture déjà validée — générez une OD inverse manuellement si besoin".into(),
            );
        }
        if status == "draft" {
            let entry = ledger::update_draft(conn, entry_id, &input)?;
            conn.execute(
                "UPDATE payslip SET entry_id = ?1, updated_at = ?2 WHERE id = ?3",
                params![entry.id, now_iso(), payslip_id],
            )?;
            return Ok(entry);
        }
    }

    let entry = ledger::create_draft(conn, &input)?;
    conn.execute(
        "UPDATE payslip SET entry_id = ?1, updated_at = ?2 WHERE id = ?3",
        params![entry.id, now_iso(), payslip_id],
    )?;
    Ok(entry)
}

/// Export DSN-oriented : CSV plat + checklist (pas de dépôt net-entreprises).
pub fn export_dsn_oriented(
    conn: &Connection,
    period_ym: &str,
    path: &std::path::Path,
) -> DbResult<DsnExportRun> {
    validate_period_ym(period_ym)?;
    let slips = list_payslips(conn, Some(period_ym))?;
    if slips.is_empty() {
        return Err(format!("Aucun bulletin pour {period_ym}").into());
    }
    let validated = slips.iter().filter(|p| p.status == "validated").count();
    let with_od = slips.iter().filter(|p| p.entry_id.is_some()).count();

    let mut csv = String::from(
        "period_ym;employee_code;employee_name;gross_cents;employee_charges_cents;employer_charges_cents;net_cents;pas_cents;status\n",
    );
    for p in &slips {
        csv.push_str(&format!(
            "{};{};{};{};{};{};{};{};{}\n",
            p.period_ym,
            p.employee_code,
            p.employee_name.replace(';', ","),
            p.gross_cents,
            p.employee_charges_cents,
            p.employer_charges_cents,
            p.net_cents,
            p.pas_cents,
            p.status
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, csv.as_bytes())?;

    let checklist = json!([
        {"code": "PERIOD", "ok": true, "label": format!("Période {period_ym}")},
        {"code": "SLIPS", "ok": !slips.is_empty(), "label": format!("{} bulletin(s)", slips.len())},
        {"code": "VALIDATED", "ok": validated == slips.len(), "label": format!("{validated}/{} validé(s)", slips.len())},
        {"code": "OD", "ok": with_od == slips.len(), "label": format!("{with_od}/{} avec OD liée", slips.len())},
        {"code": "DISCLAIMER", "ok": true, "label": "Export assisté — pas un dépôt DSN net-entreprises (jalon M10/M11)"}
    ]);

    let now = now_iso();
    conn.execute(
        "INSERT INTO dsn_export_run(period_ym, status, path, checklist_json, created_at)
         VALUES (?1, 'exported', ?2, ?3, ?4)",
        params![period_ym, path.display().to_string(), checklist.to_string(), now],
    )?;
    let id = conn.last_insert_rowid();
    Ok(DsnExportRun {
        id,
        period_ym: period_ym.to_string(),
        status: "exported".into(),
        path: Some(path.display().to_string()),
        checklist_json: checklist.to_string(),
        created_at: now,
    })
}

pub fn seed_demo_hr(conn: &Connection) -> DbResult<()> {
    let existing: i64 =
        conn.query_row("SELECT COUNT(*) FROM employee WHERE code = 'E001'", [], |r| {
            r.get(0)
        })?;
    if existing > 0 {
        return Ok(());
    }
    let emp = create_employee(
        conn,
        &CreateEmployeeInput {
            code: "E001".into(),
            first_name: "Camille".into(),
            last_name: "Martin".into(),
            birth_date: Some("1992-04-12".into()),
            address: Some("5 rue des Ateliers\n69003 Lyon".into()),
            email: Some("camille.martin@demo.sas".into()),
            iban_ref: Some("FR76****001".into()),
            active: true,
        },
    )?;
    let _ = create_contract(
        conn,
        &CreateContractInput {
            employee_id: emp.id,
            kind: "cdi".into(),
            start_date: "2025-09-01".into(),
            end_date: None,
            job_title: "Technicien détection".into(),
            weekly_hours_bp: 3_500,
            gross_monthly_cents: 280_000,
            convention_ref: Some("Syntec".into()),
            status: "active".into(),
        },
    )?;
    let emp2 = create_employee(
        conn,
        &CreateEmployeeInput {
            code: "E002".into(),
            first_name: "Noah".into(),
            last_name: "Bernard".into(),
            birth_date: Some("2004-11-03".into()),
            address: None,
            email: Some("noah.bernard@demo.sas".into()),
            iban_ref: None,
            active: true,
        },
    )?;
    let _ = create_contract(
        conn,
        &CreateContractInput {
            employee_id: emp2.id,
            kind: "apprentice".into(),
            start_date: "2025-09-01".into(),
            end_date: Some("2027-08-31".into()),
            job_title: "Apprenti installateur".into(),
            weekly_hours_bp: 3_500,
            gross_monthly_cents: 95_000,
            convention_ref: None,
            status: "active".into(),
        },
    )?;
    let leave = create_leave_request(
        conn,
        &CreateLeaveRequestInput {
            employee_id: emp.id,
            from_date: "2026-07-20".into(),
            to_date: "2026-07-24".into(),
            days_bp: 500, // 5 j
            notes: Some("Congés d’été".into()),
        },
    )?;
    let _ = set_leave_status(conn, leave.id, "approved")?;
    let _ = compute_payslip(conn, emp.id, "2026-03", None)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn apply_rate_rounds_down() {
        assert_eq!(apply_rate(280_000, 2_200), 61_600);
        assert_eq!(apply_rate(100_00, 2_200), 2_200);
    }

    #[test]
    fn hr_crud_leave_payslip_od() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("hr.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let version: String = conn
            .query_row(
                "SELECT value FROM meta WHERE key = 'schema_version'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(version, crate::db::SCHEMA_VERSION.to_string());

        let employees = list_employees(&conn).unwrap();
        assert!(employees.len() >= 2);
        let emp = employees.iter().find(|e| e.code == "E001").unwrap();

        let contracts = list_contracts(&conn, Some(emp.id)).unwrap();
        assert_eq!(contracts[0].kind, "cdi");
        assert_eq!(contracts[0].gross_monthly_cents, 280_000);

        let bals = list_leave_balances(&conn, Some(2026)).unwrap();
        let bal = bals.iter().find(|b| b.employee_id == emp.id).unwrap();
        assert!(bal.taken_days_bp >= 500);
        assert!(bal.remaining_days_bp < bal.acquired_days_bp);

        let slip = compute_payslip(&conn, emp.id, "2026-03", Some(0)).unwrap();
        assert_eq!(slip.gross_cents, 280_000);
        assert_eq!(slip.employee_charges_cents, 61_600);
        assert_eq!(slip.net_cents, 280_000 - 61_600);
        assert!(!slip.lines.is_empty());

        let again = compute_payslip(&conn, emp.id, "2026-03", None).unwrap();
        assert_eq!(again.id, slip.id);

        let validated = validate_payslip(&conn, slip.id).unwrap();
        assert_eq!(validated.status, "validated");

        let entry = build_payslip_entry_draft(&conn, slip.id).unwrap();
        assert_eq!(entry.status, "draft");
        assert_eq!(entry.journal_code, "OD");
        let entry2 = build_payslip_entry_draft(&conn, slip.id).unwrap();
        assert_eq!(entry2.id, entry.id);

        let debit: i64 = entry.lines.iter().map(|l| l.debit_cents).sum();
        let credit: i64 = entry.lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(debit, credit);

        let csv_path = dir.path().join("dsn-2026-03.csv");
        let run = export_dsn_oriented(&conn, "2026-03", &csv_path).unwrap();
        assert_eq!(run.status, "exported");
        assert!(csv_path.exists());
    }
}

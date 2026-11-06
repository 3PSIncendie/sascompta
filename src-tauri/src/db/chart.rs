use super::DbResult;
use crate::models::{Account, Journal};
use rusqlite::{params, Connection};

pub fn list_accounts(conn: &Connection) -> DbResult<Vec<Account>> {
    let mut stmt = conn.prepare(
        "SELECT id, number, label, class, account_type, is_auxiliary, is_custom, active
         FROM account ORDER BY number",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Account {
            id: r.get(0)?,
            number: r.get(1)?,
            label: r.get(2)?,
            class: r.get(3)?,
            account_type: r.get(4)?,
            is_auxiliary: r.get::<_, i32>(5)? != 0,
            is_custom: r.get::<_, i32>(6)? != 0,
            active: r.get::<_, i32>(7)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_account(
    conn: &Connection,
    number: &str,
    label: &str,
    account_type: &str,
) -> DbResult<Account> {
    if number.is_empty() || !number.chars().all(|c| c.is_ascii_digit()) {
        return Err("Numéro de compte invalide".into());
    }
    let class: i32 = number
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .ok_or("Classe invalide")? as i32;
    if !(1..=7).contains(&class) {
        return Err("Classe PCG hors 1–7".into());
    }
    conn.execute(
        "INSERT INTO account(number, label, class, account_type, is_auxiliary, is_custom, active)
         VALUES (?1, ?2, ?3, ?4, 0, 1, 1)",
        params![number, label, class, account_type],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Account {
        id,
        number: number.to_string(),
        label: label.to_string(),
        class,
        account_type: account_type.to_string(),
        is_auxiliary: false,
        is_custom: true,
        active: true,
    })
}

pub fn update_account(conn: &Connection, id: i64, label: &str, active: bool) -> DbResult<()> {
    let n = conn.execute(
        "UPDATE account SET label = ?1, active = ?2 WHERE id = ?3",
        params![label, active as i32, id],
    )?;
    if n == 0 {
        return Err("Compte introuvable".into());
    }
    Ok(())
}

pub fn delete_account(conn: &Connection, id: i64) -> DbResult<()> {
    let is_custom: i32 = conn.query_row(
        "SELECT is_custom FROM account WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if is_custom == 0 {
        return Err("Impossible de supprimer un compte PCG standard".into());
    }
    let used: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entry_line WHERE account_id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if used > 0 {
        return Err("Compte utiliséé dans des écritures".into());
    }
    conn.execute("DELETE FROM account WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn list_journals(conn: &Connection) -> DbResult<Vec<Journal>> {
    let mut stmt =
        conn.prepare("SELECT id, code, label, journal_type FROM journal ORDER BY code")?;
    let rows = stmt.query_map([], |r| {
        Ok(Journal {
            id: r.get(0)?,
            code: r.get(1)?,
            label: r.get(2)?,
            journal_type: r.get(3)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn account_id_by_number(conn: &Connection, number: &str) -> DbResult<i64> {
    conn.query_row(
        "SELECT id FROM account WHERE number = ?1",
        params![number],
        |r| r.get(0),
    )
    .map_err(|_| format!("Compte {number} introuvable").into())
}

pub fn journal_id_by_code(conn: &Connection, code: &str) -> DbResult<i64> {
    conn.query_row(
        "SELECT id FROM journal WHERE code = ?1",
        params![code],
        |r| r.get(0),
    )
    .map_err(|_| format!("Journal {code} introuvable").into())
}

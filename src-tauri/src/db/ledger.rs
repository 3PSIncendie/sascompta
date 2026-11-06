use super::chart::{account_id_by_number, journal_id_by_code};
use super::tax;
use super::DbResult;
use crate::models::*;
use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};

fn line_sums(lines: &[EntryLineInput]) -> (i64, i64) {
    let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
    let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
    (d, c)
}

fn assert_balanced(lines: &[EntryLineInput]) -> DbResult<()> {
    if lines.len() < 2 {
        return Err("Une écriture doit avoir au moins 2 lignes".into());
    }
    let (d, c) = line_sums(lines);
    if d != c {
        return Err(format!("Écriture déséquilibrée : débit {d} ≠ crédit {c}").into());
    }
    if d == 0 {
        return Err("Montants nuls".into());
    }
    for l in lines {
        if l.debit_cents < 0 || l.credit_cents < 0 {
            return Err("Montants négatifs interdits".into());
        }
        if l.debit_cents > 0 && l.credit_cents > 0 {
            return Err("Une ligne ne peut être à la fois débit et crédit".into());
        }
        if l.debit_cents == 0 && l.credit_cents == 0 {
            return Err("Ligne sans montant".into());
        }
    }
    Ok(())
}

fn resolve_comp_aux(
    conn: &Connection,
    account_number: &str,
    third_party_id: Option<i64>,
    comp_aux_type: &Option<String>,
    comp_aux_num: &Option<String>,
) -> DbResult<(Option<String>, Option<String>, Option<i64>)> {
    if let Some(tid) = third_party_id {
        let (code, kind, expected_account): (String, String, String) = conn.query_row(
            "SELECT code, kind, account_number FROM third_party WHERE id = ?1",
            params![tid],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if account_number != expected_account
            && account_number != "401"
            && account_number != "411"
        {
            // allow aux on collective accounts only
        }
        if (kind == "supplier" && account_number != "401")
            || (kind == "customer" && account_number != "411")
        {
            return Err(format!(
                "Tiers {code} incompatible avec le compte {account_number}"
            )
            .into());
        }
        let aux_type = if kind == "supplier" {
            "F".to_string()
        } else {
            "C".to_string()
        };
        return Ok((Some(aux_type), Some(code), Some(tid)));
    }
    Ok((comp_aux_type.clone(), comp_aux_num.clone(), None))
}

fn insert_lines(conn: &Connection, entry_id: i64, lines: &[EntryLineInput]) -> DbResult<()> {
    for (i, line) in lines.iter().enumerate() {
        let account_id = account_id_by_number(conn, &line.account_number)?;
        if let Some(ref code) = line.vat_code {
            tax::ensure_vat_code(conn, code)?;
        }
        let (aux_type, aux_num, tid) = resolve_comp_aux(
            conn,
            &line.account_number,
            line.third_party_id,
            &line.comp_aux_type,
            &line.comp_aux_num,
        )?;
        conn.execute(
            "INSERT INTO entry_line(entry_id, line_no, account_id, label, debit_cents, credit_cents,
             vat_code, third_party_id, comp_aux_type, comp_aux_num)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                entry_id,
                (i + 1) as i32,
                account_id,
                line.label,
                line.debit_cents,
                line.credit_cents,
                line.vat_code,
                tid.or(line.third_party_id),
                aux_type,
                aux_num,
            ],
        )?;
    }
    Ok(())
}

pub fn create_draft(conn: &Connection, input: &CreateEntryInput) -> DbResult<Entry> {
    super::closing::assert_has_open_fy(conn)?;
    assert_balanced(&input.lines)?;
    let journal_id = journal_id_by_code(conn, &input.journal_code)?;
    let fy_id: i64 = conn.query_row(
        "SELECT id FROM fiscal_year WHERE status = 'open' ORDER BY id DESC LIMIT 1",
        [],
        |r| r.get(0),
    )?;
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO entry(journal_id, entry_date, piece_ref, label, status, fiscal_year_id, created_at)
         VALUES (?1, ?2, ?3, ?4, 'draft', ?5, ?6)",
        params![
            journal_id,
            input.entry_date,
            input.piece_ref,
            input.label,
            fy_id,
            now
        ],
    )?;
    let id = conn.last_insert_rowid();
    insert_lines(conn, id, &input.lines)?;
    get_entry(conn, id)
}

pub fn update_draft(conn: &Connection, id: i64, input: &CreateEntryInput) -> DbResult<Entry> {
    super::closing::assert_entry_fy_writable(conn, id)?;
    let status: String = conn.query_row(
        "SELECT status FROM entry WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if status != "draft" {
        return Err("Écriture validée non modifiable".into());
    }
    assert_balanced(&input.lines)?;
    let journal_id = journal_id_by_code(conn, &input.journal_code)?;
    conn.execute(
        "UPDATE entry SET journal_id = ?1, entry_date = ?2, piece_ref = ?3, label = ?4 WHERE id = ?5",
        params![
            journal_id,
            input.entry_date,
            input.piece_ref,
            input.label,
            id
        ],
    )?;
    conn.execute("DELETE FROM entry_line WHERE entry_id = ?1", params![id])?;
    insert_lines(conn, id, &input.lines)?;
    get_entry(conn, id)
}

pub fn delete_draft(conn: &Connection, id: i64) -> DbResult<()> {
    super::closing::assert_entry_fy_writable(conn, id)?;
    let status: String = conn.query_row(
        "SELECT status FROM entry WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if status != "draft" {
        return Err("Écriture validée non supprimable".into());
    }
    conn.execute("DELETE FROM attachment WHERE entry_id = ?1", params![id])?;
    conn.execute("DELETE FROM entry_line WHERE entry_id = ?1", params![id])?;
    conn.execute("DELETE FROM entry WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn post_entry(conn: &Connection, id: i64) -> DbResult<Entry> {
    super::closing::assert_entry_fy_writable(conn, id)?;
    let status: String = conn.query_row(
        "SELECT status FROM entry WHERE id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    if status != "draft" {
        return Err("Écriture déjà validée".into());
    }
    let (debit, credit): (i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(debit_cents),0), COALESCE(SUM(credit_cents),0)
         FROM entry_line WHERE entry_id = ?1",
        params![id],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if debit != credit || debit == 0 {
        return Err(format!("Écriture déséquilibrée : {debit} ≠ {credit}").into());
    }
    let journal_code: String = conn.query_row(
        "SELECT j.code FROM entry e JOIN journal j ON j.id = e.journal_id WHERE e.id = ?1",
        params![id],
        |r| r.get(0),
    )?;
    let seq: i64 = conn.query_row(
        "SELECT COUNT(*) + 1 FROM entry e
         JOIN journal j ON j.id = e.journal_id
         WHERE j.code = ?1 AND e.status = 'posted'",
        params![journal_code],
        |r| r.get(0),
    )?;
    let ecriture_num = format!("{journal_code}-{seq:04}");
    let now = Utc::now().to_rfc3339();
    conn.execute(
        "UPDATE entry SET status = 'posted', ecriture_num = ?1, posted_at = ?2,
         validation_date = date('now'), piece_date = entry_date WHERE id = ?3",
        params![ecriture_num, now, id],
    )?;
    get_entry(conn, id)
}

pub fn reverse_entry(conn: &Connection, id: i64) -> DbResult<Entry> {
    super::closing::assert_entry_fy_writable(conn, id)?;
    let entry = get_entry(conn, id)?;
    if entry.status != "posted" {
        return Err("Seule une écriture validée peut être contrepassée".into());
    }
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM entry WHERE reversal_of_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .optional()?;
    if existing.is_some() {
        return Err("Contrepassation déjà effectuée".into());
    }
    let lines: Vec<EntryLineInput> = entry
        .lines
        .iter()
        .map(|l| EntryLineInput {
            account_number: l.account_number.clone(),
            label: l.label.clone(),
            debit_cents: l.credit_cents,
            credit_cents: l.debit_cents,
            vat_code: l.vat_code.clone(),
            third_party_id: l.third_party_id,
            comp_aux_type: l.comp_aux_type.clone(),
            comp_aux_num: l.comp_aux_num.clone(),
        })
        .collect();
    let journal_id = journal_id_by_code(conn, &entry.journal_code)?;
    let now = Utc::now().to_rfc3339();
    let piece = format!("{}-CP", entry.piece_ref);
    let label = format!("Contrepassation — {}", entry.label);
    conn.execute(
        "INSERT INTO entry(journal_id, entry_date, piece_ref, label, status, fiscal_year_id,
         reversal_of_id, created_at)
         VALUES (?1, ?2, ?3, ?4, 'draft', ?5, ?6, ?7)",
        params![
            journal_id,
            entry.entry_date,
            piece,
            label,
            entry.fiscal_year_id,
            id,
            now
        ],
    )?;
    let new_id = conn.last_insert_rowid();
    insert_lines(conn, new_id, &lines)?;
    post_entry(conn, new_id)
}

pub fn add_attachment(
    conn: &Connection,
    entry_id: i64,
    file_name: &str,
    file_path: &str,
    mime_type: Option<String>,
) -> DbResult<Attachment> {
    let status: String = conn.query_row(
        "SELECT status FROM entry WHERE id = ?1",
        params![entry_id],
        |r| r.get(0),
    )?;
    if status != "draft" {
        return Err("Pièces figées après validation".into());
    }
    conn.execute(
        "INSERT INTO attachment(entry_id, file_name, file_path, mime_type) VALUES (?1, ?2, ?3, ?4)",
        params![entry_id, file_name, file_path, mime_type],
    )?;
    let id = conn.last_insert_rowid();
    Ok(Attachment {
        id,
        entry_id,
        file_name: file_name.to_string(),
        file_path: file_path.to_string(),
        mime_type,
    })
}

pub fn list_entries(conn: &Connection) -> DbResult<Vec<Entry>> {
    let mut stmt = conn.prepare(
        "SELECT e.id FROM entry e
         JOIN journal j ON j.id = e.journal_id
         ORDER BY e.entry_date, e.id",
    )?;
    let ids: Vec<i64> = stmt
        .query_map([], |r| r.get(0))?
        .filter_map(|r| r.ok())
        .collect();
    ids.into_iter().map(|id| get_entry(conn, id)).collect()
}

pub fn get_entry(conn: &Connection, id: i64) -> DbResult<Entry> {
    let entry = conn.query_row(
        "SELECT e.id, j.code, e.entry_date, e.piece_ref, e.label, e.status, e.fiscal_year_id,
                e.ecriture_num, e.reversal_of_id, e.created_at, e.posted_at
         FROM entry e JOIN journal j ON j.id = e.journal_id WHERE e.id = ?1",
        params![id],
        |r| {
            Ok(Entry {
                id: r.get(0)?,
                journal_code: r.get(1)?,
                entry_date: r.get(2)?,
                piece_ref: r.get(3)?,
                label: r.get(4)?,
                status: r.get(5)?,
                fiscal_year_id: r.get(6)?,
                ecriture_num: r.get(7)?,
                reversal_of_id: r.get(8)?,
                created_at: r.get(9)?,
                posted_at: r.get(10)?,
                lines: vec![],
                attachments: vec![],
            })
        },
    )?;

    let mut stmt = conn.prepare(
        "SELECT el.id, el.entry_id, el.line_no, a.number, a.label, el.label,
                el.debit_cents, el.credit_cents, el.vat_code, el.third_party_id,
                tp.code, tp.name, el.comp_aux_type, el.comp_aux_num
         FROM entry_line el
         JOIN account a ON a.id = el.account_id
         LEFT JOIN third_party tp ON tp.id = el.third_party_id
         WHERE el.entry_id = ?1 ORDER BY el.line_no",
    )?;
    let lines: Vec<EntryLine> = stmt
        .query_map(params![id], |r| {
            Ok(EntryLine {
                id: r.get(0)?,
                entry_id: r.get(1)?,
                line_no: r.get(2)?,
                account_number: r.get(3)?,
                account_label: r.get(4)?,
                label: r.get(5)?,
                debit_cents: r.get(6)?,
                credit_cents: r.get(7)?,
                vat_code: r.get(8)?,
                third_party_id: r.get(9)?,
                third_party_code: r.get(10)?,
                third_party_name: r.get(11)?,
                comp_aux_type: r.get(12)?,
                comp_aux_num: r.get(13)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    let mut stmt = conn.prepare(
        "SELECT id, entry_id, file_name, file_path, mime_type FROM attachment WHERE entry_id = ?1",
    )?;
    let attachments: Vec<Attachment> = stmt
        .query_map(params![id], |r| {
            Ok(Attachment {
                id: r.get(0)?,
                entry_id: r.get(1)?,
                file_name: r.get(2)?,
                file_path: r.get(3)?,
                mime_type: r.get(4)?,
            })
        })?
        .filter_map(|r| r.ok())
        .collect();

    Ok(Entry {
        lines,
        attachments,
        ..entry
    })
}

fn line(
    account: &str,
    debit: i64,
    credit: i64,
    vat: Option<&str>,
    third_code: Option<&str>,
) -> EntryLineInput {
    EntryLineInput {
        account_number: account.to_string(),
        label: None,
        debit_cents: debit,
        credit_cents: credit,
        vat_code: vat.map(|s| s.to_string()),
        third_party_id: None,
        comp_aux_type: None,
        comp_aux_num: third_code.map(|s| s.to_string()),
    }
}

fn resolve_third_ids(conn: &Connection, lines: &mut [EntryLineInput]) -> DbResult<()> {
    for l in lines.iter_mut() {
        if let Some(ref code) = l.comp_aux_num.clone() {
            let tid: i64 = conn.query_row(
                "SELECT id FROM third_party WHERE code = ?1",
                params![code],
                |r| r.get(0),
            )?;
            l.third_party_id = Some(tid);
            let kind: String = conn.query_row(
                "SELECT kind FROM third_party WHERE id = ?1",
                params![tid],
                |r| r.get(0),
            )?;
            l.comp_aux_type = Some(if kind == "supplier" {
                "F".into()
            } else {
                "C".into()
            });
        }
    }
    Ok(())
}

fn post_seed(conn: &Connection, journal: &str, date: &str, piece: &str, label: &str, mut lines: Vec<EntryLineInput>) -> DbResult<()> {
    resolve_third_ids(conn, &mut lines)?;
    let draft = create_draft(
        conn,
        &CreateEntryInput {
            journal_code: journal.into(),
            entry_date: date.into(),
            piece_ref: piece.into(),
            label: label.into(),
            lines,
        },
    )?;
    post_entry(conn, draft.id)?;
    Ok(())
}

/// 10 écritures démo posted équilibrées avec TVA + tiers
pub fn seed_demo_entries(conn: &Connection) -> DbResult<()> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entry WHERE status = 'posted'",
        [],
        |r| r.get(0),
    )?;
    if count > 0 {
        return Ok(());
    }

    // 1. AN
    post_seed(
        conn,
        "AN",
        "2026-01-01",
        "AN-2026",
        "À-nouveaux capital / banque",
        vec![
            line("512", 5_000_000, 0, None, None),
            line("101", 0, 5_000_000, None, None),
        ],
    )?;

    // 2. ACH FA-1001 — F001
    post_seed(
        conn,
        "ACH",
        "2026-01-15",
        "FA-1001",
        "Achat fournitures Alpha",
        vec![
            line("606", 100_000, 0, Some("T20D"), None),
            line("44566", 20_000, 0, Some("T20D"), None),
            line("401", 0, 120_000, Some("T20D"), Some("F001")),
        ],
    )?;

    // 3. ACH FA-1002 — F002
    post_seed(
        conn,
        "ACH",
        "2026-02-01",
        "FA-1002",
        "Achat marchandises Beta",
        vec![
            line("607", 500_000, 0, Some("T20D"), None),
            line("44566", 100_000, 0, Some("T20D"), None),
            line("401", 0, 600_000, Some("T20D"), Some("F002")),
        ],
    )?;

    // 4. VTE FV-2001 — C001
    post_seed(
        conn,
        "VTE",
        "2026-02-10",
        "FV-2001",
        "Prestation Gamma",
        vec![
            line("411", 360_000, 0, Some("T20C"), Some("C001")),
            line("706", 0, 300_000, Some("T20C"), None),
            line("44571", 0, 60_000, Some("T20C"), None),
        ],
    )?;

    // 5. VTE FV-2002 — C002
    post_seed(
        conn,
        "VTE",
        "2026-03-01",
        "FV-2002",
        "Vente marchandises Delta",
        vec![
            line("411", 1_200_000, 0, Some("T20C"), Some("C002")),
            line("707", 0, 1_000_000, Some("T20C"), None),
            line("44571", 0, 200_000, Some("T20C"), None),
        ],
    )?;

    // 6. BQ VIR-01 — règlement F001
    post_seed(
        conn,
        "BQ",
        "2026-03-05",
        "VIR-01",
        "Règlement fournisseur Alpha",
        vec![
            line("401", 120_000, 0, None, Some("F001")),
            line("512", 0, 120_000, None, None),
        ],
    )?;

    // 7. BQ VIR-02 — encaissement C001
    post_seed(
        conn,
        "BQ",
        "2026-03-10",
        "VIR-02",
        "Encaissement client Gamma",
        vec![
            line("512", 360_000, 0, None, None),
            line("411", 0, 360_000, None, Some("C001")),
        ],
    )?;

    // 8. BQ CB-01
    post_seed(
        conn,
        "BQ",
        "2026-03-15",
        "CB-01",
        "Frais bancaires",
        vec![
            line("627", 2_500, 0, None, None),
            line("512", 0, 2_500, None, None),
        ],
    )?;

    // 9. OD OD-02 — honoraires F001
    // (la 10e écriture = dotation amortissement générée par assets::generate_period_depreciation)
    post_seed(
        conn,
        "OD",
        "2026-04-01",
        "OD-02",
        "Honoraires Alpha",
        vec![
            line("622", 80_000, 0, Some("T20D"), None),
            line("44566", 16_000, 0, Some("T20D"), None),
            line("401", 0, 96_000, Some("T20D"), Some("F001")),
        ],
    )?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn post_and_reverse() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("rev.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let draft = create_draft(
            &conn,
            &CreateEntryInput {
                journal_code: "OD".into(),
                entry_date: "2026-05-01".into(),
                piece_ref: "OD-T".into(),
                label: "Test".into(),
                lines: vec![
                    line("627", 1000, 0, None, None),
                    line("512", 0, 1000, None, None),
                ],
            },
        )
        .unwrap();
        let posted = post_entry(&conn, draft.id).unwrap();
        assert_eq!(posted.status, "posted");
        assert!(update_draft(
            &conn,
            posted.id,
            &CreateEntryInput {
                journal_code: "OD".into(),
                entry_date: "2026-05-01".into(),
                piece_ref: "OD-T".into(),
                label: "X".into(),
                lines: vec![
                    line("627", 1000, 0, None, None),
                    line("512", 0, 1000, None, None),
                ],
            }
        )
        .is_err());
        let rev = reverse_entry(&conn, posted.id).unwrap();
        assert_eq!(rev.status, "posted");
        assert_eq!(rev.reversal_of_id, Some(posted.id));
    }

    #[test]
    fn unbalanced_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bad.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let err = create_draft(
            &conn,
            &CreateEntryInput {
                journal_code: "OD".into(),
                entry_date: "2026-05-01".into(),
                piece_ref: "X".into(),
                label: "Bad".into(),
                lines: vec![
                    line("627", 1000, 0, None, None),
                    line("512", 0, 500, None, None),
                ],
            },
        );
        assert!(err.is_err());
    }
}

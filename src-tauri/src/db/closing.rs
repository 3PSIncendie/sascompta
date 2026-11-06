//! J7 — Clôture d'exercice, à-nouveaux, pack publication ZIP, FEC brouillon.

use super::assets;
use super::ledger;
use super::reporting::{balance_report, write_text_file};
use super::statements;
use super::DbResult;
use crate::models::*;
use chrono::{Datelike, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

pub fn migrate_closing(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS closing_event (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            fiscal_year_id INTEGER NOT NULL REFERENCES fiscal_year(id),
            closed_at TEXT NOT NULL,
            an_entry_id INTEGER REFERENCES entry(id),
            next_fiscal_year_id INTEGER REFERENCES fiscal_year(id),
            pack_path TEXT,
            notes TEXT
        );
        "#,
    )?;
    Ok(())
}

pub fn current_fiscal_year(conn: &Connection) -> DbResult<FiscalYear> {
    conn.query_row(
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
    )
    .map_err(|e| e.into())
}

pub fn fiscal_year_by_id(conn: &Connection, id: i64) -> DbResult<FiscalYear> {
    conn.query_row(
        "SELECT id, label, start_date, end_date, status FROM fiscal_year WHERE id = ?1",
        params![id],
        |r| {
            Ok(FiscalYear {
                id: r.get(0)?,
                label: r.get(1)?,
                start_date: r.get(2)?,
                end_date: r.get(3)?,
                status: r.get(4)?,
            })
        },
    )
    .map_err(|e| e.into())
}

/// Refuse toute mutation si l'exercice de l'écriture est `closed`.
pub fn assert_entry_fy_writable(conn: &Connection, entry_id: i64) -> DbResult<()> {
    let status: String = conn.query_row(
        "SELECT fy.status FROM entry e JOIN fiscal_year fy ON fy.id = e.fiscal_year_id WHERE e.id = ?1",
        params![entry_id],
        |r| r.get(0),
    )?;
    if status == "closed" {
        return Err("Exercice clôturé — écriture en lecture seule".into());
    }
    Ok(())
}

pub fn assert_has_open_fy(conn: &Connection) -> DbResult<()> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM fiscal_year WHERE status = 'open'",
        [],
        |r| r.get(0),
    )?;
    if n == 0 {
        return Err("Aucun exercice ouvert — clôture en cours ou workspace verrouillé".into());
    }
    Ok(())
}

fn year_filter(fy: &FiscalYear) -> LedgerFilter {
    statements::year_filter(&fy.start_date, &fy.end_date)
}

pub fn closing_checklist(conn: &Connection) -> DbResult<ClosingChecklist> {
    let fy = current_fiscal_year(conn)?;
    let filter = year_filter(&fy);
    let mut items = Vec::new();

    // 1. Balance équilibrée
    let bal = balance_report(conn, &filter)?;
    items.push(ChecklistItem {
        code: "BALANCE".into(),
        label: "Balance générale équilibrée (écart = 0)".into(),
        ok: bal.ecart_cents == 0,
        detail: Some(format!("écart {}", crate::db::tax::format_cents_csv(bal.ecart_cents))),
    });

    // 2. Aucun brouillon sur l'exercice
    let drafts: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entry WHERE fiscal_year_id = ?1 AND status = 'draft'",
        params![fy.id],
        |r| r.get(0),
    )?;
    items.push(ChecklistItem {
        code: "DRAFTS".into(),
        label: "Aucun brouillon restant sur l'exercice".into(),
        ok: drafts == 0,
        detail: Some(format!("{drafts} brouillon(s)")),
    });

    // 3. Bilan cohérent
    let sheet = statements::build_balance_sheet(conn, &filter)?;
    items.push(ChecklistItem {
        code: "BILAN".into(),
        label: "Bilan équilibré (actif = passif)".into(),
        ok: sheet.ecart_cents == 0,
        detail: Some(format!(
            "actif {} / passif {} / résultat {}",
            crate::db::tax::format_cents_csv(sheet.total_actif_cents),
            crate::db::tax::format_cents_csv(sheet.total_passif_cents),
            crate::db::tax::format_cents_csv(sheet.resultat_cents),
        )),
    });

    // 4. Dotations immos pour le label d'exercice (si registre non vide)
    let assets = assets::list_assets(conn)?;
    let active_assets: Vec<_> = assets.into_iter().filter(|a| a.active).collect();
    if active_assets.is_empty() {
        items.push(ChecklistItem {
            code: "IMMO".into(),
            label: "Dotations immobilisations (aucun bien actif)".into(),
            ok: true,
            detail: Some("N/A".into()),
        });
    } else {
        let runs = assets::list_depreciation_runs(conn, None)?;
        let missing: Vec<String> = active_assets
            .iter()
            .filter(|a| {
                !runs
                    .iter()
                    .any(|r| r.asset_id == a.id && r.period_label == fy.label)
            })
            .map(|a| a.code.clone())
            .collect();
        items.push(ChecklistItem {
            code: "IMMO".into(),
            label: format!("Dotations amortissement générées pour {}", fy.label),
            ok: missing.is_empty(),
            detail: if missing.is_empty() {
                Some("OK".into())
            } else {
                Some(format!("Manquant: {}", missing.join(", ")))
            },
        });
    }

    // 5. Au moins une écriture posted
    let posted: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entry WHERE fiscal_year_id = ?1 AND status = 'posted'",
        params![fy.id],
        |r| r.get(0),
    )?;
    items.push(ChecklistItem {
        code: "POSTED".into(),
        label: "Écritures validées présentes".into(),
        ok: posted > 0,
        detail: Some(format!("{posted} écriture(s)")),
    });

    // 6. Statut encore ouvert
    items.push(ChecklistItem {
        code: "OPEN".into(),
        label: "Exercice encore ouvert (prêt à clôturer)".into(),
        ok: fy.status == "open",
        detail: Some(fy.status.clone()),
    });

    let all_ok = items.iter().all(|i| i.ok);
    Ok(ClosingChecklist {
        fiscal_year: fy,
        items,
        all_ok,
        resultat_cents: sheet.resultat_cents,
    })
}

fn next_year_dates(end_date: &str) -> DbResult<(String, String, String)> {
    // end_date = YYYY-MM-DD → next start = day after, label = year+1 if civil
    let parts: Vec<&str> = end_date.split('-').collect();
    if parts.len() != 3 {
        return Err("Date de fin d'exercice invalide".into());
    }
    let y: i32 = parts[0].parse().map_err(|_| "Année invalide")?;
    let m: u32 = parts[1].parse().map_err(|_| "Mois invalide")?;
    let d: u32 = parts[2].parse().map_err(|_| "Jour invalide")?;
    let date = chrono::NaiveDate::from_ymd_opt(y, m, d).ok_or("Date invalide")?;
    let next_start = date
        .succ_opt()
        .ok_or("Impossible de calculer le jour suivant")?;
    let next_end = chrono::NaiveDate::from_ymd_opt(next_start.year() + 1, m, d)
        .or_else(|| chrono::NaiveDate::from_ymd_opt(next_start.year() + 1, m, d.saturating_sub(1)))
        .ok_or("Impossible de calculer la fin N+1")?;
    // Prefer same month-day end; for 12-31 → next 12-31
    let next_end = if m == 12 && d == 31 {
        chrono::NaiveDate::from_ymd_opt(next_start.year(), 12, 31).unwrap()
    } else {
        next_end
    };
    let label = next_start.year().to_string();
    Ok((
        label,
        next_start.format("%Y-%m-%d").to_string(),
        next_end.format("%Y-%m-%d").to_string(),
    ))
}

/// Construit les lignes d'à-nouveaux (classes 1–5 + résultat → 120).
pub fn build_an_lines(conn: &Connection, fy: &FiscalYear) -> DbResult<Vec<EntryLineInput>> {
    let filter = year_filter(fy);
    let bal = balance_report(conn, &filter)?;
    let income = statements::build_income_statement(conn, &filter)?;
    let mut lines = Vec::new();

    for row in &bal.rows {
        let class = row.class;
        if !(1..=5).contains(&class) {
            continue;
        }
        let net = row.solde_debit_cents - row.solde_credit_cents;
        if net == 0 {
            continue;
        }
        if net > 0 {
            lines.push(EntryLineInput {
                account_number: row.account_number.clone(),
                label: Some(format!("AN {}", row.account_label)),
                debit_cents: net,
                credit_cents: 0,
                vat_code: None,
                third_party_id: None,
                comp_aux_type: None,
                comp_aux_num: None,
            });
        } else {
            lines.push(EntryLineInput {
                account_number: row.account_number.clone(),
                label: Some(format!("AN {}", row.account_label)),
                debit_cents: 0,
                credit_cents: -net,
                vat_code: None,
                third_party_id: None,
                comp_aux_type: None,
                comp_aux_num: None,
            });
        }
    }

    // Résultat → 120 (bénéfice au crédit) ou 129 si perte — on utilise 120 avec sens
    if income.resultat_cents > 0 {
        lines.push(EntryLineInput {
            account_number: "120".into(),
            label: Some(format!("Résultat reporté exercice {}", fy.label)),
            debit_cents: 0,
            credit_cents: income.resultat_cents,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    } else if income.resultat_cents < 0 {
        lines.push(EntryLineInput {
            account_number: "120".into(),
            label: Some(format!("Perte reportée exercice {}", fy.label)),
            debit_cents: -income.resultat_cents,
            credit_cents: 0,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        });
    }

    let d: i64 = lines.iter().map(|l| l.debit_cents).sum();
    let c: i64 = lines.iter().map(|l| l.credit_cents).sum();
    if d != c {
        return Err(format!("AN déséquilibré : débit {d} ≠ crédit {c}").into());
    }
    if lines.len() < 2 {
        return Err("AN : pas assez de soldes à reporter".into());
    }
    Ok(lines)
}

/// Clôture l'exercice courant : checklist → AN N+1 → verrouillage `closed`.
pub fn close_fiscal_year(conn: &Connection) -> DbResult<CloseYearResult> {
    let checklist = closing_checklist(conn)?;
    if !checklist.all_ok {
        let fails: Vec<_> = checklist
            .items
            .iter()
            .filter(|i| !i.ok)
            .map(|i| i.code.clone())
            .collect();
        return Err(format!("Checklist incomplète : {}", fails.join(", ")).into());
    }
    let fy = checklist.fiscal_year.clone();
    if fy.status != "open" {
        return Err("Exercice déjà clôturé ou en cours de clôture".into());
    }

    conn.execute(
        "UPDATE fiscal_year SET status = 'closing' WHERE id = ?1",
        params![fy.id],
    )?;

    let (next_label, next_start, next_end) = next_year_dates(&fy.end_date)?;
    conn.execute(
        "INSERT INTO fiscal_year(label, start_date, end_date, status) VALUES (?1, ?2, ?3, 'open')",
        params![next_label, next_start, next_end],
    )?;
    let next_fy_id = conn.last_insert_rowid();

    let an_lines = build_an_lines(conn, &fy)?;
    let draft = ledger::create_draft(
        conn,
        &CreateEntryInput {
            journal_code: "AN".into(),
            entry_date: next_start.clone(),
            piece_ref: format!("AN-{}", next_label),
            label: format!("À-nouveaux exercice {}", next_label),
            lines: an_lines,
        },
    )?;
    let an_entry = ledger::post_entry(conn, draft.id)?;

    conn.execute(
        "UPDATE fiscal_year SET status = 'closed' WHERE id = ?1",
        params![fy.id],
    )?;

    let now = Utc::now().to_rfc3339();
    conn.execute(
        "INSERT INTO closing_event(fiscal_year_id, closed_at, an_entry_id, next_fiscal_year_id, notes)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            fy.id,
            now,
            an_entry.id,
            next_fy_id,
            format!("Clôture {} → ouverture {}", fy.label, next_label)
        ],
    )?;

    let next = fiscal_year_by_id(conn, next_fy_id)?;
    Ok(CloseYearResult {
        closed_year: fiscal_year_by_id(conn, fy.id)?,
        next_year: next,
        an_entry_id: an_entry.id,
        an_ecriture_num: an_entry.ecriture_num.clone(),
        resultat_cents: checklist.resultat_cents,
    })
}

// ——— FEC brouillon ———

fn fec_amount(cents: i64) -> String {
    crate::db::tax::format_cents_csv(cents)
}

fn fec_date(iso: &str) -> String {
    // YYYY-MM-DD → YYYYMMDD
    iso.chars().filter(|c| c.is_ascii_digit()).collect()
}

/// Export FEC brouillon (TSV, séparateur tabulation) — non certifié DGFiP.
pub fn export_fec_draft(conn: &Connection, filter: &LedgerFilter) -> DbResult<String> {
    let movements = super::reporting::ledger_movements(conn, filter)?;
    // Enrich with aux lib / journal lib / piece_date / validation_date from DB
    let mut out = String::new();
    out.push_str(
        "JournalCode\tJournalLib\tEcritureNum\tEcritureDate\tCompteNum\tCompteLib\tCompAuxNum\tCompAuxLib\tPieceRef\tPieceDate\tEcritureLib\tDebit\tCredit\tEcritureLet\tDateLet\tValidDate\tMontantdevise\tIdevise\n",
    );

    let mut stmt = conn.prepare(
        "SELECT e.id, j.code, j.label, e.ecriture_num, e.entry_date, a.number, a.label,
                COALESCE(el.comp_aux_num, ''), COALESCE(tp.name, ''),
                e.piece_ref, COALESCE(e.piece_date, e.entry_date),
                COALESCE(el.label, e.label), el.debit_cents, el.credit_cents,
                COALESCE(e.ecriture_lettre, ''), COALESCE(e.ecriture_date_lettre, ''),
                COALESCE(e.validation_date, '')
         FROM entry_line el
         JOIN entry e ON e.id = el.entry_id
         JOIN journal j ON j.id = e.journal_id
         JOIN account a ON a.id = el.account_id
         LEFT JOIN third_party tp ON tp.id = el.third_party_id
         WHERE e.status = 'posted'
           AND e.entry_date >= ?1 AND e.entry_date <= ?2
         ORDER BY e.entry_date, e.id, el.line_no",
    )?;
    let from = filter.from_date.as_deref().unwrap_or("0000-01-01");
    let to = filter.to_date.as_deref().unwrap_or("9999-12-31");
    let rows = stmt.query_map(params![from, to], |r| {
        Ok((
            r.get::<_, String>(1)?,  // journal code
            r.get::<_, String>(2)?,  // journal lib
            r.get::<_, Option<String>>(3)?,
            r.get::<_, String>(4)?,  // date
            r.get::<_, String>(5)?,  // compte
            r.get::<_, String>(6)?,  // compte lib
            r.get::<_, String>(7)?,  // aux num
            r.get::<_, String>(8)?,  // aux lib
            r.get::<_, String>(9)?,  // piece
            r.get::<_, String>(10)?, // piece date
            r.get::<_, String>(11)?, // lib
            r.get::<_, i64>(12)?,
            r.get::<_, i64>(13)?,
            r.get::<_, String>(14)?,
            r.get::<_, String>(15)?,
            r.get::<_, String>(16)?,
        ))
    })?;

    let mut n = 0usize;
    for row in rows.flatten() {
        let (
            jcode,
            jlib,
            enum_,
            edate,
            cnum,
            clib,
            auxn,
            auxl,
            pref,
            pdate,
            elib,
            debit,
            credit,
            elet,
            dlet,
            vdate,
        ) = row;
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t\t\n",
            jcode,
            jlib.replace('\t', " "),
            enum_.unwrap_or_default(),
            fec_date(&edate),
            cnum,
            clib.replace('\t', " "),
            auxn,
            auxl.replace('\t', " "),
            pref.replace('\t', " "),
            fec_date(&pdate),
            elib.replace('\t', " "),
            fec_amount(debit),
            fec_amount(credit),
            elet,
            if dlet.is_empty() {
                String::new()
            } else {
                fec_date(&dlet)
            },
            if vdate.is_empty() {
                String::new()
            } else {
                fec_date(&vdate)
            },
        ));
        n += 1;
    }
    let _ = (movements.len(), n);
    Ok(out)
}

// ——— Pack publication ZIP ———

fn zip_add_bytes(zip: &mut ZipWriter<BufWriter<File>>, name: &str, data: &[u8]) -> DbResult<()> {
    zip.start_file(name, SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated))
        .map_err(|e| format!("ZIP start {name}: {e}"))?;
    zip.write_all(data)
        .map_err(|e| format!("ZIP write {name}: {e}"))?;
    Ok(())
}

pub fn build_publication_pack(conn: &Connection, fy: &FiscalYear) -> DbResult<PublicationPackMeta> {
    let filter = year_filter(fy);
    let pack = statements::build_annual_pack(conn, &filter)?;
    let bal = balance_report(conn, &filter)?;
    Ok(PublicationPackMeta {
        organization_name: pack.organization_name.clone(),
        siren: pack.siren.clone(),
        fiscal_year_label: fy.label.clone(),
        from_date: fy.start_date.clone(),
        to_date: fy.end_date.clone(),
        fy_status: fy.status.clone(),
        resultat_cents: pack.compte_resultat.resultat_cents,
        total_actif_cents: pack.bilan.total_actif_cents,
        total_passif_cents: pack.bilan.total_passif_cents,
        balance_ecart_cents: bal.ecart_cents,
        generated_at: Utc::now().to_rfc3339(),
        disclaimer: "Pack d'aide au dépôt manuel greffe — non certifié. FEC brouillon non certifié DGFiP.".into(),
    })
}

pub fn export_publication_zip(conn: &Connection, fy: &FiscalYear, zip_path: &Path) -> DbResult<PublicationPackMeta> {
    let filter = year_filter(fy);
    let annual = statements::build_annual_pack(conn, &filter)?;
    let meta = build_publication_pack(conn, fy)?;
    let bal = balance_report(conn, &filter)?;
    let fec = export_fec_draft(conn, &filter)?;

    if let Some(parent) = zip_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(zip_path)?;
    let mut zip = ZipWriter::new(BufWriter::new(file));

    let meta_json = serde_json::to_string_pretty(&meta).map_err(|e| e.to_string())?;
    zip_add_bytes(&mut zip, "meta.json", meta_json.as_bytes())?;
    zip_add_bytes(
        &mut zip,
        "bilan.csv",
        statements::export_bilan_csv(&annual.bilan).as_bytes(),
    )?;
    zip_add_bytes(
        &mut zip,
        "compte-resultat.csv",
        statements::export_cr_csv(&annual.compte_resultat).as_bytes(),
    )?;
    zip_add_bytes(
        &mut zip,
        "annexe.csv",
        statements::export_annexe_csv(&annual.annexe).as_bytes(),
    )?;
    zip_add_bytes(
        &mut zip,
        "balance.csv",
        super::reporting::export_balance_csv(&bal).as_bytes(),
    )?;
    zip_add_bytes(&mut zip, "fec-brouillon.txt", fec.as_bytes())?;

    // PDF to temp then into zip
    let tmp_pdf = zip_path.with_extension("tmp.pdf");
    statements::export_annual_pdf(&tmp_pdf, &annual)?;
    let pdf_bytes = std::fs::read(&tmp_pdf)?;
    zip_add_bytes(&mut zip, "comptes-annuels.pdf", &pdf_bytes)?;
    let _ = std::fs::remove_file(&tmp_pdf);

    let readme = format!(
        "SasCompta — Pack comptes annuels {}\nSociété: {} (SIREN {})\nExercice: {} → {}\n\nContenu:\n- comptes-annuels.pdf\n- bilan.csv / compte-resultat.csv / annexe.csv / balance.csv\n- meta.json\n- fec-brouillon.txt (BROUILLON — non certifié DGFiP)\n\nDéposer manuellement auprès du greffe. Outil d'aide uniquement.\n",
        fy.label, meta.organization_name, meta.siren, fy.start_date, fy.end_date
    );
    zip_add_bytes(&mut zip, "README.txt", readme.as_bytes())?;

    zip.finish()
        .map_err(|e| format!("ZIP finish: {e}"))?;

    // Trace path on last closing event if any
    let _ = conn.execute(
        "UPDATE closing_event SET pack_path = ?1 WHERE id = (
            SELECT id FROM closing_event ORDER BY id DESC LIMIT 1
         )",
        params![zip_path.display().to_string()],
    );

    Ok(meta)
}

pub fn write_fec_file(conn: &Connection, filter: &LedgerFilter, path: &Path) -> DbResult<()> {
    let fec = export_fec_draft(conn, filter)?;
    write_text_file(path, &fec)
}

pub fn last_closing_event(conn: &Connection) -> DbResult<Option<ClosingEventInfo>> {
    let row = conn
        .query_row(
            "SELECT id, fiscal_year_id, closed_at, an_entry_id, next_fiscal_year_id, pack_path, notes
             FROM closing_event ORDER BY id DESC LIMIT 1",
            [],
            |r| {
                Ok(ClosingEventInfo {
                    id: r.get(0)?,
                    fiscal_year_id: r.get(1)?,
                    closed_at: r.get(2)?,
                    an_entry_id: r.get(3)?,
                    next_fiscal_year_id: r.get(4)?,
                    pack_path: r.get(5)?,
                    notes: r.get(6)?,
                })
            },
        )
        .optional()?;
    Ok(row)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn checklist_ok_on_demo() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("j7.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let cl = closing_checklist(&conn).unwrap();
        assert!(cl.all_ok, "{:?}", cl.items);
        assert_eq!(cl.resultat_cents, 567_500);
    }

    #[test]
    fn close_locks_and_creates_an() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("j7c.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let res = close_fiscal_year(&conn).unwrap();
        assert_eq!(res.closed_year.status, "closed");
        assert_eq!(res.next_year.status, "open");
        assert_eq!(res.next_year.label, "2027");
        assert!(res.an_ecriture_num.is_some());

        // Lecture seule sur N
        let old_id: i64 = conn
            .query_row(
                "SELECT id FROM entry WHERE fiscal_year_id = ?1 AND status = 'posted' LIMIT 1",
                params![res.closed_year.id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(assert_entry_fy_writable(&conn, old_id).is_err());
        assert!(ledger::reverse_entry(&conn, old_id).is_err());

        // AN équilibré sur 2027
        let an = ledger::get_entry(&conn, res.an_entry_id).unwrap();
        assert_eq!(an.journal_code, "AN");
        let d: i64 = an.lines.iter().map(|l| l.debit_cents).sum();
        let c: i64 = an.lines.iter().map(|l| l.credit_cents).sum();
        assert_eq!(d, c);
    }

    #[test]
    fn fec_and_zip_pack() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("j7p.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let fy = current_fiscal_year(&conn).unwrap();
        let filter = year_filter(&fy);

        let fec = export_fec_draft(&conn, &filter).unwrap();
        assert!(fec.contains("JournalCode"));
        assert!(fec.contains("AN\t"));
        assert!(fec.contains("512"));

        let zip_path = dir.path().join("pack-2026.zip");
        let meta = export_publication_zip(&conn, &fy, &zip_path).unwrap();
        assert!(zip_path.exists());
        assert!(zip_path.metadata().unwrap().len() > 100);
        assert_eq!(meta.resultat_cents, 567_500);
        assert_eq!(meta.siren, "123456782");
    }
}

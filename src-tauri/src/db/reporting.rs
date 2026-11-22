use super::DbResult;
use crate::models::*;
use printpdf::*;
use rusqlite::Connection;
use std::fs::File;
use std::io::BufWriter;
use std::path::Path;

pub fn ledger_movements(conn: &Connection, filter: &LedgerFilter) -> DbResult<Vec<LedgerMovement>> {
    let from = filter.from_date.as_deref().unwrap_or("0000-01-01");
    let to = filter.to_date.as_deref().unwrap_or("9999-12-31");
    let mut sql = String::from(
        "SELECT e.id, e.entry_date, j.code, e.piece_ref, e.ecriture_num, e.label,
                a.number, a.label, el.label, el.debit_cents, el.credit_cents,
                el.vat_code, COALESCE(tp.code, el.comp_aux_num)
         FROM entry_line el
         JOIN entry e ON e.id = el.entry_id
         JOIN journal j ON j.id = e.journal_id
         JOIN account a ON a.id = el.account_id
         LEFT JOIN third_party tp ON tp.id = el.third_party_id
         WHERE e.status = 'posted'
           AND e.entry_date >= ?1 AND e.entry_date <= ?2",
    );
    let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> =
        vec![Box::new(from.to_string()), Box::new(to.to_string())];
    let mut idx = 3;
    if let Some(ref jc) = filter.journal_code {
        if !jc.is_empty() {
            sql.push_str(&format!(" AND j.code = ?{idx}"));
            params_vec.push(Box::new(jc.clone()));
            idx += 1;
        }
    }
    if let Some(ref ac) = filter.account_number {
        if !ac.is_empty() {
            sql.push_str(&format!(" AND a.number = ?{idx}"));
            params_vec.push(Box::new(ac.clone()));
            idx += 1;
        }
    }
    if let Some(fy_id) = filter.fiscal_year_id {
        sql.push_str(&format!(" AND e.fiscal_year_id = ?{idx}"));
        params_vec.push(Box::new(fy_id));
    }
    sql.push_str(" ORDER BY e.entry_date, e.id, el.line_no");

    let mut stmt = conn.prepare(&sql)?;
    let params_refs: Vec<&dyn rusqlite::types::ToSql> =
        params_vec.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(params_refs.as_slice(), |r| {
        Ok(LedgerMovement {
            entry_id: r.get(0)?,
            entry_date: r.get(1)?,
            journal_code: r.get(2)?,
            piece_ref: r.get(3)?,
            ecriture_num: r.get(4)?,
            entry_label: r.get(5)?,
            account_number: r.get(6)?,
            account_label: r.get(7)?,
            line_label: r.get(8)?,
            debit_cents: r.get(9)?,
            credit_cents: r.get(10)?,
            vat_code: r.get(11)?,
            third_party_code: r.get(12)?,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn balance_report(conn: &Connection, filter: &LedgerFilter) -> DbResult<BalanceReport> {
    let movements = ledger_movements(conn, filter)?;
    use std::collections::BTreeMap;
    let mut map: BTreeMap<String, (String, i32, i64, i64)> = BTreeMap::new();
    for m in &movements {
        let class: i32 = m
            .account_number
            .chars()
            .next()
            .and_then(|c| c.to_digit(10))
            .unwrap_or(0) as i32;
        let e = map
            .entry(m.account_number.clone())
            .or_insert((m.account_label.clone(), class, 0, 0));
        e.2 += m.debit_cents;
        e.3 += m.credit_cents;
    }
    let mut rows = Vec::new();
    let mut total_debit = 0i64;
    let mut total_credit = 0i64;
    for (number, (label, class, debit, credit)) in map {
        total_debit += debit;
        total_credit += credit;
        let net = debit - credit;
        rows.push(BalanceRow {
            account_number: number,
            account_label: label,
            class,
            debit_cents: debit,
            credit_cents: credit,
            solde_debit_cents: if net > 0 { net } else { 0 },
            solde_credit_cents: if net < 0 { -net } else { 0 },
        });
    }
    Ok(BalanceReport {
        rows,
        total_debit_cents: total_debit,
        total_credit_cents: total_credit,
        ecart_cents: total_debit - total_credit,
    })
}

fn cents_fr(cents: i64) -> String {
    crate::db::tax::format_cents_csv(cents)
}

pub fn export_balance_csv(report: &BalanceReport) -> String {
    let mut out = String::from('\u{FEFF}');
    out.push_str("Compte;Libellé;Classe;Débit;Crédit;Solde débit;Solde crédit\n");
    for r in &report.rows {
        out.push_str(&format!(
            "{};{};{};{};{};{};{}\n",
            r.account_number,
            r.account_label,
            r.class,
            cents_fr(r.debit_cents),
            cents_fr(r.credit_cents),
            cents_fr(r.solde_debit_cents),
            cents_fr(r.solde_credit_cents),
        ));
    }
    out.push_str(&format!(
        "TOTAL;;;{};{};;écart {}\n",
        cents_fr(report.total_debit_cents),
        cents_fr(report.total_credit_cents),
        cents_fr(report.ecart_cents),
    ));
    out
}

pub fn export_ledger_csv(movements: &[LedgerMovement]) -> String {
    let mut out = String::from('\u{FEFF}');
    out.push_str(
        "Date;Journal;Pièce;N° écriture;Compte;Libellé compte;Libellé;Tiers;TVA;Débit;Crédit\n",
    );
    for m in movements {
        out.push_str(&format!(
            "{};{};{};{};{};{};{};{};{};{};{}\n",
            m.entry_date,
            m.journal_code,
            m.piece_ref,
            m.ecriture_num.as_deref().unwrap_or(""),
            m.account_number,
            m.account_label,
            m.line_label
                .as_deref()
                .unwrap_or(m.entry_label.as_str())
                .replace(';', ","),
            m.third_party_code.as_deref().unwrap_or(""),
            m.vat_code.as_deref().unwrap_or(""),
            cents_fr(m.debit_cents),
            cents_fr(m.credit_cents),
        ));
    }
    out
}

fn ascii_safe(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'î' | 'ï' => 'i',
            'ô' | 'ö' => 'o',
            'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            'À' | 'Â' | 'Ä' => 'A',
            'É' | 'È' | 'Ê' | 'Ë' => 'E',
            'Î' | 'Ï' => 'I',
            'Ô' | 'Ö' => 'O',
            'Ù' | 'Û' | 'Ü' => 'U',
            'Ç' => 'C',
            c if c.is_ascii() => c,
            _ => '?',
        })
        .collect()
}

pub fn export_ledger_pdf(path: &Path, title: &str, movements: &[LedgerMovement]) -> DbResult<()> {
    let (doc, page1, layer1) = PdfDocument::new("SasCompta", Mm(210.0), Mm(297.0), "Layer1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font: {e}"))?;
    let mut current_layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 280.0;
    current_layer.use_text(ascii_safe(title), 14.0, Mm(15.0), Mm(y), &font);
    y -= 10.0;
    current_layer.use_text(
        ascii_safe(&format!("{} mouvements", movements.len())),
        10.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    y -= 8.0;
    for m in movements {
        if y < 20.0 {
            let (p, l) = doc.add_page(Mm(210.0), Mm(297.0), "Layer");
            current_layer = doc.get_page(p).get_layer(l);
            y = 280.0;
        }
        let line = format!(
            "{} {} {} {} D{} C{}",
            m.entry_date,
            m.journal_code,
            m.account_number,
            m.piece_ref,
            cents_fr(m.debit_cents),
            cents_fr(m.credit_cents),
        );
        current_layer.use_text(ascii_safe(&line), 8.0, Mm(15.0), Mm(y), &font);
        y -= 5.0;
    }
    doc.save(&mut BufWriter::new(File::create(path)?))
        .map_err(|e| format!("PDF save: {e}"))?;
    Ok(())
}

pub fn export_balance_pdf(path: &Path, title: &str, report: &BalanceReport) -> DbResult<()> {
    let (doc, page1, layer1) = PdfDocument::new("SasCompta Balance", Mm(210.0), Mm(297.0), "L1");
    let font = doc
        .add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| format!("PDF font: {e}"))?;
    let layer = doc.get_page(page1).get_layer(layer1);
    let mut y = 280.0;
    layer.use_text(ascii_safe(title), 14.0, Mm(15.0), Mm(y), &font);
    y -= 10.0;
    for r in &report.rows {
        if y < 20.0 {
            break;
        }
        let line = format!(
            "{} {} D{} C{}",
            r.account_number,
            r.account_label,
            cents_fr(r.debit_cents),
            cents_fr(r.credit_cents),
        );
        layer.use_text(ascii_safe(&line), 8.0, Mm(15.0), Mm(y), &font);
        y -= 5.0;
    }
    y -= 5.0;
    layer.use_text(
        ascii_safe(&format!(
            "TOTAL D{} C{} ecart {}",
            cents_fr(report.total_debit_cents),
            cents_fr(report.total_credit_cents),
            cents_fr(report.ecart_cents),
        )),
        10.0,
        Mm(15.0),
        Mm(y),
        &font,
    );
    doc.save(&mut BufWriter::new(File::create(path)?))
        .map_err(|e| format!("PDF save: {e}"))?;
    Ok(())
}

pub fn write_text_file(path: &Path, content: &str) -> DbResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, content)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::closing::{current_fiscal_year, ensure_open_fiscal_years_for_import};
    use crate::db::ledger::{create_draft, post_entry};
    use crate::db::{create_demo_workspace, open_connection};
    use crate::models::{CreateEntryInput, EntryLineInput};
    use tempfile::tempdir;

    fn od_line(account: &str, debit: i64, credit: i64) -> EntryLineInput {
        EntryLineInput {
            account_number: account.into(),
            label: None,
            debit_cents: debit,
            credit_cents: credit,
            vat_code: None,
            third_party_id: None,
            comp_aux_type: None,
            comp_aux_num: None,
        }
    }

    #[test]
    fn fiscal_year_id_excludes_prior_year_entries() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("fy-filter.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        let fy2025_id = ensure_open_fiscal_years_for_import(&conn, "2025-06-15").unwrap();
        let prior = create_draft(
            &conn,
            &CreateEntryInput {
                journal_code: "OD".into(),
                entry_date: "2025-06-15".into(),
                piece_ref: "OD-2025".into(),
                label: "Charge N-1".into(),
                lines: vec![od_line("627", 50_00, 0), od_line("512", 0, 50_00)],
            },
        )
        .unwrap();
        let prior = post_entry(&conn, prior.id).unwrap();
        assert_eq!(prior.fiscal_year_id, fy2025_id);

        let fy2026 = current_fiscal_year(&conn).unwrap();
        assert_ne!(fy2026.id, fy2025_id);

        let wide_current = LedgerFilter {
            from_date: Some("2024-01-01".into()),
            to_date: Some("2026-12-31".into()),
            journal_code: None,
            account_number: None,
            fiscal_year_id: Some(fy2026.id),
        };
        let cur_moves = ledger_movements(&conn, &wide_current).unwrap();
        assert!(
            cur_moves.iter().all(|m| m.entry_id != prior.id),
            "current-FY filter must not mix prior-year entries"
        );

        let prior_only = LedgerFilter {
            from_date: Some("2024-01-01".into()),
            to_date: Some("2026-12-31".into()),
            journal_code: None,
            account_number: None,
            fiscal_year_id: Some(fy2025_id),
        };
        let prior_moves = ledger_movements(&conn, &prior_only).unwrap();
        assert!(
            prior_moves.iter().any(|m| m.entry_id == prior.id),
            "prior-FY filter must still see its own entries"
        );
        assert!(prior_moves.iter().all(|m| {
            // Demo 2026 seed entries must stay out of the 2025 scope.
            m.entry_id == prior.id || m.entry_date.starts_with("2025")
        }));
    }

    #[test]
    fn balance_equals_ledger_ecart_zero() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("bal.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let filter = LedgerFilter {
            from_date: Some("2026-01-01".into()),
            to_date: Some("2026-12-31".into()),
            journal_code: None,
            account_number: None,
            fiscal_year_id: None,
        };
        let movements = ledger_movements(&conn, &filter).unwrap();
        let report = balance_report(&conn, &filter).unwrap();
        let mov_d: i64 = movements.iter().map(|m| m.debit_cents).sum();
        let mov_c: i64 = movements.iter().map(|m| m.credit_cents).sum();
        assert_eq!(report.total_debit_cents, mov_d);
        assert_eq!(report.total_credit_cents, mov_c);
        assert_eq!(report.ecart_cents, 0);
        // Seed J2–J5 + acquisition IMMO 5 000 € (J6) + dotation 500 €
        assert_eq!(report.total_debit_cents, 8_408_500);
    }

    #[test]
    fn export_csv_and_pdf() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("exp.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let filter = LedgerFilter {
            from_date: None,
            to_date: None,
            journal_code: None,
            account_number: None,
            fiscal_year_id: None,
        };
        let bal = balance_report(&conn, &filter).unwrap();
        let csv = export_balance_csv(&bal);
        assert!(csv.contains("écart"));
        let pdf_path = dir.path().join("out.pdf");
        let mov = ledger_movements(&conn, &filter).unwrap();
        export_ledger_pdf(&pdf_path, "Grand livre", &mov).unwrap();
        assert!(pdf_path.exists());
    }
}

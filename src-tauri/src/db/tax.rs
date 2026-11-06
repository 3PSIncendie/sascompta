use super::DbResult;
use crate::models::*;
use rusqlite::{params, Connection};

pub fn seed_vat_codes(conn: &Connection) -> DbResult<()> {
    let codes: &[(&str, &str, i32, &str)] = &[
        ("T20C", "TVA 20% collectée", 2000, "collected"),
        ("T20D", "TVA 20% déductible", 2000, "deductible"),
        ("T10C", "TVA 10% collectée", 1000, "collected"),
        ("T10D", "TVA 10% déductible", 1000, "deductible"),
        ("T55C", "TVA 5,5% collectée", 550, "collected"),
        ("T55D", "TVA 5,5% déductible", 550, "deductible"),
        ("TEX", "Exonéré / hors champ", 0, "exempt"),
    ];
    for (code, label, rate, dir) in codes {
        conn.execute(
            "INSERT OR IGNORE INTO vat_code(code, label, rate_bp, direction, active)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![code, label, rate, dir],
        )?;
    }
    Ok(())
}

pub fn ensure_vat_code(conn: &Connection, code: &str) -> DbResult<()> {
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM vat_code WHERE code = ?1",
        params![code],
        |r| r.get(0),
    )?;
    if exists == 0 {
        return Err(format!("Code TVA inconnu: {code}").into());
    }
    Ok(())
}

pub fn list_vat_codes(conn: &Connection) -> DbResult<Vec<VatCode>> {
    let mut stmt = conn.prepare(
        "SELECT code, label, rate_bp, direction, active FROM vat_code ORDER BY code",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(VatCode {
            code: r.get(0)?,
            label: r.get(1)?,
            rate_bp: r.get(2)?,
            direction: r.get(3)?,
            active: r.get::<_, i32>(4)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn seed_demo_third_parties(conn: &Connection) -> DbResult<()> {
    let tiers = [
        ("F001", "Fournisseur Alpha", "supplier", "401"),
        ("F002", "Fournisseur Beta", "supplier", "401"),
        ("C001", "Client Gamma", "customer", "411"),
        ("C002", "Client Delta", "customer", "411"),
    ];
    for (code, name, kind, account) in tiers {
        conn.execute(
            "INSERT OR IGNORE INTO third_party(code, name, kind, account_number, active)
             VALUES (?1, ?2, ?3, ?4, 1)",
            params![code, name, kind, account],
        )?;
    }
    Ok(())
}

pub fn list_third_parties(conn: &Connection) -> DbResult<Vec<ThirdParty>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, name, kind, account_number, active FROM third_party ORDER BY code",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(ThirdParty {
            id: r.get(0)?,
            code: r.get(1)?,
            name: r.get(2)?,
            kind: r.get(3)?,
            account_number: r.get(4)?,
            active: r.get::<_, i32>(5)? != 0,
        })
    })?;
    Ok(rows.filter_map(|r| r.ok()).collect())
}

pub fn create_third_party(conn: &Connection, input: &CreateThirdPartyInput) -> DbResult<ThirdParty> {
    let account = match input.kind.as_str() {
        "supplier" => "401",
        "customer" => "411",
        _ => return Err("Type de tiers invalide (supplier|customer)".into()),
    };
    conn.execute(
        "INSERT INTO third_party(code, name, kind, account_number, active) VALUES (?1, ?2, ?3, ?4, 1)",
        params![input.code, input.name, input.kind, account],
    )?;
    let id = conn.last_insert_rowid();
    Ok(ThirdParty {
        id,
        code: input.code.clone(),
        name: input.name.clone(),
        kind: input.kind.clone(),
        account_number: account.to_string(),
        active: true,
    })
}

pub fn update_third_party(
    conn: &Connection,
    id: i64,
    name: &str,
    active: bool,
) -> DbResult<()> {
    let n = conn.execute(
        "UPDATE third_party SET name = ?1, active = ?2 WHERE id = ?3",
        params![name, active as i32, id],
    )?;
    if n == 0 {
        return Err("Tiers introuvable".into());
    }
    Ok(())
}

/// CA3 draft: base HT from charge/product lines tagged with VAT;
/// TVA from 44566 (deductible) / 44571 (collected) lines with same code.
pub fn vat_report(
    conn: &Connection,
    from_date: &str,
    to_date: &str,
) -> DbResult<VatReport> {
    let codes = list_vat_codes(conn)?;
    let mut lines = Vec::new();
    let mut collected = 0i64;
    let mut deductible = 0i64;

    for vc in &codes {
        if vc.direction == "exempt" {
            continue;
        }
        // Base HT: classes 6 (charges) / 7 (produits) only — not 401/411/445
        let base: i64 = conn.query_row(
            "SELECT COALESCE(SUM(
                CASE
                  WHEN v.direction = 'collected' THEN el.credit_cents - el.debit_cents
                  WHEN v.direction = 'deductible' THEN el.debit_cents - el.credit_cents
                  ELSE 0
                END
             ), 0)
             FROM entry_line el
             JOIN entry e ON e.id = el.entry_id
             JOIN account a ON a.id = el.account_id
             JOIN vat_code v ON v.code = el.vat_code
             WHERE e.status = 'posted'
               AND e.entry_date >= ?1 AND e.entry_date <= ?2
               AND el.vat_code = ?3
               AND a.class IN (6, 7)",
            params![from_date, to_date, vc.code],
            |r| r.get(0),
        )?;

        let vat: i64 = conn.query_row(
            "SELECT COALESCE(SUM(
                CASE
                  WHEN v.direction = 'collected' THEN el.credit_cents - el.debit_cents
                  WHEN v.direction = 'deductible' THEN el.debit_cents - el.credit_cents
                  ELSE 0
                END
             ), 0)
             FROM entry_line el
             JOIN entry e ON e.id = el.entry_id
             JOIN account a ON a.id = el.account_id
             JOIN vat_code v ON v.code = el.vat_code
             WHERE e.status = 'posted'
               AND e.entry_date >= ?1 AND e.entry_date <= ?2
               AND el.vat_code = ?3
               AND (a.number LIKE '44566%' OR a.number LIKE '44571%')",
            params![from_date, to_date, vc.code],
            |r| r.get(0),
        )?;

        if base == 0 && vat == 0 {
            continue;
        }

        match vc.direction.as_str() {
            "collected" => collected += vat,
            "deductible" => deductible += vat,
            _ => {}
        }

        lines.push(VatReportLine {
            code: vc.code.clone(),
            label: vc.label.clone(),
            direction: vc.direction.clone(),
            rate_bp: vc.rate_bp,
            base_cents: base,
            vat_cents: vat,
        });
    }

    Ok(VatReport {
        lines,
        collected_cents: collected,
        deductible_cents: deductible,
        net_cents: collected - deductible,
        from_date: from_date.to_string(),
        to_date: to_date.to_string(),
    })
}

pub fn auxiliary_balance(
    conn: &Connection,
    from_date: Option<&str>,
    to_date: Option<&str>,
) -> DbResult<AuxBalanceReport> {
    let from = from_date.unwrap_or("0000-01-01");
    let to = to_date.unwrap_or("9999-12-31");
    let mut stmt = conn.prepare(
        "SELECT a.number, a.label,
                COALESCE(el.comp_aux_num, tp.code) AS aux_code,
                COALESCE(tp.name, el.comp_aux_num, '') AS aux_label,
                COALESCE(SUM(el.debit_cents), 0),
                COALESCE(SUM(el.credit_cents), 0)
         FROM entry_line el
         JOIN entry e ON e.id = el.entry_id
         JOIN account a ON a.id = el.account_id
         LEFT JOIN third_party tp ON tp.id = el.third_party_id
         WHERE e.status = 'posted'
           AND a.number IN ('401', '411')
           AND e.entry_date >= ?1 AND e.entry_date <= ?2
           AND COALESCE(el.comp_aux_num, tp.code) IS NOT NULL
         GROUP BY a.number, a.label, aux_code, aux_label
         ORDER BY a.number, aux_code",
    )?;
    let rows: Vec<AuxBalanceRow> = stmt
        .query_map(params![from, to], |r| {
            let debit: i64 = r.get(4)?;
            let credit: i64 = r.get(5)?;
            let net = debit - credit;
            Ok(AuxBalanceRow {
                account_number: r.get(0)?,
                account_label: r.get(1)?,
                aux_code: r.get(2)?,
                aux_label: r.get(3)?,
                debit_cents: debit,
                credit_cents: credit,
                solde_debit_cents: if net > 0 { net } else { 0 },
                solde_credit_cents: if net < 0 { -net } else { 0 },
            })
        })?
        .filter_map(|r| r.ok())
        .collect();
    Ok(AuxBalanceReport { rows })
}

pub fn export_vat_csv(report: &VatReport) -> String {
    let mut out = String::from("Code;Libellé;Sens;Taux %;Base HT;TVA\n");
    for l in &report.lines {
        let sens = match l.direction.as_str() {
            "collected" => "collected",
            "deductible" => "deductible",
            other => other,
        };
        out.push_str(&format!(
            "{};{};{};{};{};{}\n",
            l.code,
            l.label,
            sens,
            format_rate_bp(l.rate_bp),
            format_cents_csv(l.base_cents),
            format_cents_csv(l.vat_cents),
        ));
    }
    out.push_str(&format!(
        ";;;;Collectée;{}\n;;;;Déductible;{}\n;;;;Net à payer;{}\n",
        format_cents_csv(report.collected_cents),
        format_cents_csv(report.deductible_cents),
        format_cents_csv(report.net_cents),
    ));
    out
}

pub fn export_aux_csv(report: &AuxBalanceReport) -> String {
    let mut out = String::from(
        "Compte;Libellé compte;Auxiliaire;Libellé auxiliaire;Débit;Crédit;Solde D;Solde C\n",
    );
    for r in &report.rows {
        out.push_str(&format!(
            "{};{};{};{};{};{};{};{}\n",
            r.account_number,
            r.account_label,
            r.aux_code,
            r.aux_label,
            format_cents_csv(r.debit_cents),
            format_cents_csv(r.credit_cents),
            format_cents_csv(r.solde_debit_cents),
            format_cents_csv(r.solde_credit_cents),
        ));
    }
    out
}

pub fn format_cents_csv(cents: i64) -> String {
    let neg = cents < 0;
    let abs = cents.abs();
    let euros = abs / 100;
    let rem = abs % 100;
    let s = format!("{euros},{rem:02}");
    if neg {
        format!("-{s}")
    } else {
        s
    }
}

fn format_rate_bp(bp: i32) -> String {
    let whole = bp / 100;
    let frac = bp % 100;
    format!("{whole},{frac:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn vat_report_matches_demo_lines() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vat.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let report = vat_report(&conn, "2026-01-01", "2026-12-31").unwrap();
        assert_eq!(report.collected_cents, 260_000, "collectée");
        assert_eq!(report.deductible_cents, 136_000, "déductible");
        assert_eq!(report.net_cents, 124_000, "net");
        let t20c = report.lines.iter().find(|l| l.code == "T20C").unwrap();
        assert_eq!(t20c.base_cents, 1_300_000);
        assert_eq!(t20c.vat_cents, 260_000);
        let t20d = report.lines.iter().find(|l| l.code == "T20D").unwrap();
        assert_eq!(t20d.base_cents, 680_000);
        assert_eq!(t20d.vat_cents, 136_000);
    }

    #[test]
    fn auxiliary_balance_ok() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("aux.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();
        let report = auxiliary_balance(&conn, Some("2026-01-01"), Some("2026-12-31")).unwrap();
        let find = |code: &str| report.rows.iter().find(|r| r.aux_code == code).unwrap();
        let f001 = find("F001");
        assert_eq!(f001.solde_credit_cents, 96_000);
        assert_eq!(f001.solde_debit_cents, 0);
        let f002 = find("F002");
        assert_eq!(f002.solde_credit_cents, 600_000);
        let c001 = find("C001");
        assert_eq!(c001.solde_debit_cents, 0);
        assert_eq!(c001.solde_credit_cents, 0);
        let c002 = find("C002");
        assert_eq!(c002.solde_debit_cents, 1_200_000);
    }
}

//! Immobilisations & amortissements linéaires (J5).

use super::ledger;
use super::DbResult;
use crate::models::*;
use rusqlite::{params, Connection, OptionalExtension};

pub fn migrate_assets(conn: &Connection) -> DbResult<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS fixed_asset (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            code TEXT NOT NULL UNIQUE,
            label TEXT NOT NULL,
            account_number TEXT NOT NULL,
            amort_account TEXT NOT NULL DEFAULT '281',
            expense_account TEXT NOT NULL DEFAULT '681',
            acquisition_date TEXT NOT NULL,
            service_date TEXT NOT NULL,
            base_cents INTEGER NOT NULL CHECK (base_cents > 0),
            residual_cents INTEGER NOT NULL DEFAULT 0 CHECK (residual_cents >= 0),
            useful_life_years INTEGER NOT NULL CHECK (useful_life_years > 0),
            method TEXT NOT NULL DEFAULT 'linear'
                CHECK (method IN ('linear')),
            active INTEGER NOT NULL DEFAULT 1,
            notes TEXT
        );

        CREATE TABLE IF NOT EXISTS depreciation_run (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            asset_id INTEGER NOT NULL REFERENCES fixed_asset(id),
            period_label TEXT NOT NULL,
            period_start TEXT NOT NULL,
            period_end TEXT NOT NULL,
            amount_cents INTEGER NOT NULL CHECK (amount_cents > 0),
            entry_id INTEGER REFERENCES entry(id),
            created_at TEXT NOT NULL,
            UNIQUE(asset_id, period_label)
        );
        "#,
    )?;
    Ok(())
}

/// Dotation annuelle linéaire (centimes) : (base − résidu) / durée.
pub fn annual_dotation_cents(base_cents: i64, residual_cents: i64, useful_life_years: i32) -> i64 {
    let amortizable = (base_cents - residual_cents).max(0);
    if useful_life_years <= 0 {
        return 0;
    }
    amortizable / i64::from(useful_life_years)
}

pub fn build_linear_schedule(asset: &FixedAsset) -> Vec<DepreciationScheduleRow> {
    let annual = annual_dotation_cents(
        asset.base_cents,
        asset.residual_cents,
        asset.useful_life_years,
    );
    let amortizable = (asset.base_cents - asset.residual_cents).max(0);
    let mut rows = Vec::new();
    let mut cumul = 0_i64;
    let start_year: i32 = asset
        .service_date
        .get(0..4)
        .and_then(|s| s.parse().ok())
        .unwrap_or(2026);

    for year_idx in 0..asset.useful_life_years {
        let year_no = year_idx + 1;
        let amount = if year_no == asset.useful_life_years {
            // Ajuste la dernière année pour absorber les arrondis
            amortizable - cumul
        } else {
            annual
        };
        cumul += amount;
        let nbv = asset.base_cents - cumul;
        rows.push(DepreciationScheduleRow {
            year_index: year_no,
            period_label: format!("{}", start_year + year_idx),
            amount_cents: amount,
            cumulative_cents: cumul,
            net_book_value_cents: nbv.max(asset.residual_cents),
        });
    }
    rows
}

fn map_asset(row: &rusqlite::Row<'_>) -> rusqlite::Result<FixedAsset> {
    Ok(FixedAsset {
        id: row.get(0)?,
        code: row.get(1)?,
        label: row.get(2)?,
        account_number: row.get(3)?,
        amort_account: row.get(4)?,
        expense_account: row.get(5)?,
        acquisition_date: row.get(6)?,
        service_date: row.get(7)?,
        base_cents: row.get(8)?,
        residual_cents: row.get(9)?,
        useful_life_years: row.get(10)?,
        method: row.get(11)?,
        active: row.get::<_, i32>(12)? != 0,
        notes: row.get(13)?,
        annual_dotation_cents: 0,
    })
}

fn with_annual(mut asset: FixedAsset) -> FixedAsset {
    asset.annual_dotation_cents = annual_dotation_cents(
        asset.base_cents,
        asset.residual_cents,
        asset.useful_life_years,
    );
    asset
}

pub fn list_assets(conn: &Connection) -> DbResult<Vec<FixedAsset>> {
    let mut stmt = conn.prepare(
        "SELECT id, code, label, account_number, amort_account, expense_account,
                acquisition_date, service_date, base_cents, residual_cents,
                useful_life_years, method, active, notes
         FROM fixed_asset ORDER BY code",
    )?;
    let rows = stmt
        .query_map([], map_asset)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows.into_iter().map(with_annual).collect())
}

pub fn get_asset(conn: &Connection, id: i64) -> DbResult<FixedAsset> {
    let asset = conn.query_row(
        "SELECT id, code, label, account_number, amort_account, expense_account,
                acquisition_date, service_date, base_cents, residual_cents,
                useful_life_years, method, active, notes
         FROM fixed_asset WHERE id = ?1",
        params![id],
        map_asset,
    )?;
    Ok(with_annual(asset))
}

pub fn create_asset(conn: &Connection, input: &CreateFixedAssetInput) -> DbResult<FixedAsset> {
    if input.code.trim().is_empty() || input.label.trim().is_empty() {
        return Err("Code et libellé obligatoires".into());
    }
    if input.base_cents <= 0 {
        return Err("Base amortissable invalide".into());
    }
    if input.useful_life_years <= 0 {
        return Err("Durée d'amortissement invalide".into());
    }
    if input.residual_cents < 0 || input.residual_cents >= input.base_cents {
        return Err("Valeur résiduelle invalide".into());
    }
    let account = if input.account_number.trim().is_empty() {
        "215"
    } else {
        input.account_number.trim()
    };
    let amort = input
        .amort_account
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("281");
    let expense = input
        .expense_account
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("681");
    let service = if input.service_date.trim().is_empty() {
        input.acquisition_date.as_str()
    } else {
        input.service_date.as_str()
    };

    conn.execute(
        "INSERT INTO fixed_asset(
            code, label, account_number, amort_account, expense_account,
            acquisition_date, service_date, base_cents, residual_cents,
            useful_life_years, method, active, notes)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'linear',1,?11)",
        params![
            input.code.trim(),
            input.label.trim(),
            account,
            amort,
            expense,
            input.acquisition_date,
            service,
            input.base_cents,
            input.residual_cents,
            input.useful_life_years,
            input.notes
        ],
    )?;
    let id = conn.last_insert_rowid();
    get_asset(conn, id)
}

pub fn update_asset(
    conn: &Connection,
    id: i64,
    label: &str,
    active: bool,
    notes: Option<String>,
) -> DbResult<FixedAsset> {
    conn.execute(
        "UPDATE fixed_asset SET label = ?1, active = ?2, notes = ?3 WHERE id = ?4",
        params![label, active as i32, notes, id],
    )?;
    get_asset(conn, id)
}

pub fn asset_schedule(conn: &Connection, asset_id: i64) -> DbResult<Vec<DepreciationScheduleRow>> {
    let asset = get_asset(conn, asset_id)?;
    Ok(build_linear_schedule(&asset))
}

pub fn list_depreciation_runs(
    conn: &Connection,
    asset_id: Option<i64>,
) -> DbResult<Vec<DepreciationRun>> {
    let mut sql = String::from(
        "SELECT r.id, r.asset_id, a.code, a.label, r.period_label, r.period_start, r.period_end,
                r.amount_cents, r.entry_id, e.ecriture_num, j.code, r.created_at
         FROM depreciation_run r
         JOIN fixed_asset a ON a.id = r.asset_id
         LEFT JOIN entry e ON e.id = r.entry_id
         LEFT JOIN journal j ON j.id = e.journal_id
         WHERE 1=1",
    );
    if asset_id.is_some() {
        sql.push_str(" AND r.asset_id = ?1");
    }
    sql.push_str(" ORDER BY r.period_label, a.code");

    let mut stmt = conn.prepare(&sql)?;
    let map = |row: &rusqlite::Row<'_>| {
        Ok(DepreciationRun {
            id: row.get(0)?,
            asset_id: row.get(1)?,
            asset_code: row.get(2)?,
            asset_label: row.get(3)?,
            period_label: row.get(4)?,
            period_start: row.get(5)?,
            period_end: row.get(6)?,
            amount_cents: row.get(7)?,
            entry_id: row.get(8)?,
            ecriture_num: row.get(9)?,
            journal_code: row.get(10)?,
            created_at: row.get(11)?,
        })
    };

    let rows = if let Some(id) = asset_id {
        stmt.query_map(params![id], map)?
            .collect::<Result<Vec<_>, _>>()?
    } else {
        stmt.query_map([], map)?.collect::<Result<Vec<_>, _>>()?
    };
    Ok(rows)
}

fn period_amount_for_asset(asset: &FixedAsset, period_label: &str) -> DbResult<i64> {
    let schedule = build_linear_schedule(asset);
    schedule
        .into_iter()
        .find(|r| r.period_label == period_label)
        .map(|r| r.amount_cents)
        .filter(|a| *a > 0)
        .ok_or_else(|| {
            format!(
                "Aucune dotation prévue pour {} sur la période {}",
                asset.code, period_label
            )
            .into()
        })
}

/// Génère (ou rappelle) les OD d'amortissement pour une période (= année du plan).
pub fn generate_period_depreciation(
    conn: &Connection,
    period_label: &str,
    period_start: &str,
    period_end: &str,
) -> DbResult<GenerateDepreciationResult> {
    super::closing::assert_has_open_fy(conn)?;
    let assets = list_assets(conn)?;
    let mut created = Vec::new();
    let mut skipped = Vec::new();
    let mut total = 0_i64;

    for asset in assets.into_iter().filter(|a| a.active) {
        if asset.service_date.as_str() > period_end {
            skipped.push(format!("{} : pas encore en service", asset.code));
            continue;
        }
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM depreciation_run WHERE asset_id = ?1 AND period_label = ?2",
                params![asset.id, period_label],
                |r| r.get(0),
            )
            .optional()?;
        if existing.is_some() {
            skipped.push(format!("{} : déjà doté sur {}", asset.code, period_label));
            continue;
        }

        let amount = match period_amount_for_asset(&asset, period_label) {
            Ok(a) => a,
            Err(e) => {
                skipped.push(e.to_string());
                continue;
            }
        };

        let input = CreateEntryInput {
            journal_code: "OD".into(),
            entry_date: period_end.to_string(),
            piece_ref: format!("AMO-{}-{}", asset.code, period_label),
            label: format!(
                "Dotation amortissement {} — {}",
                asset.code, asset.label
            ),
            lines: vec![
                EntryLineInput {
                    account_number: asset.expense_account.clone(),
                    label: Some(format!("Dotation {}", asset.code)),
                    debit_cents: amount,
                    credit_cents: 0,
                    vat_code: None,
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                },
                EntryLineInput {
                    account_number: asset.amort_account.clone(),
                    label: Some(format!("Amort. cumulé {}", asset.code)),
                    debit_cents: 0,
                    credit_cents: amount,
                    vat_code: None,
                    third_party_id: None,
                    comp_aux_type: None,
                    comp_aux_num: None,
                },
            ],
        };

        let draft = ledger::create_draft(conn, &input)?;
        let posted = ledger::post_entry(conn, draft.id)?;
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO depreciation_run(
                asset_id, period_label, period_start, period_end, amount_cents, entry_id, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![
                asset.id,
                period_label,
                period_start,
                period_end,
                amount,
                posted.id,
                now
            ],
        )?;
        total += amount;
        created.push(list_depreciation_runs(conn, Some(asset.id))?
            .into_iter()
            .find(|r| r.period_label == period_label)
            .expect("run just inserted"));
    }

    Ok(GenerateDepreciationResult {
        period_label: period_label.to_string(),
        created,
        skipped,
        total_amount_cents: total,
    })
}

/// Bien démo : base 5 000 €, 10 ans → 500 € / an (aligne le test de dotation).
/// Poste aussi l’acquisition en OD (D 215 / C 512) pour un bilan cohérent (J6).
pub fn seed_demo_assets(conn: &Connection) -> DbResult<FixedAsset> {
    let existing: Option<i64> = conn
        .query_row(
            "SELECT id FROM fixed_asset WHERE code = 'IMMO-001'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return get_asset(conn, id);
    }
    let asset = create_asset(
        conn,
        &CreateFixedAssetInput {
            code: "IMMO-001".into(),
            label: "Machine atelier".into(),
            account_number: "215".into(),
            amort_account: Some("281".into()),
            expense_account: Some("681".into()),
            acquisition_date: "2026-01-01".into(),
            service_date: "2026-01-01".into(),
            base_cents: 500_000,
            residual_cents: 0,
            useful_life_years: 10,
            notes: Some("Démo J5 — plan linéaire 10 ans".into()),
        },
    )?;

    // Acquisition comptabilisée (cohérence bilan / annexe immos)
    let acq_exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM entry WHERE piece_ref = 'IMMO-ACQ-001' AND status = 'posted'",
        [],
        |r| r.get(0),
    )?;
    if acq_exists == 0 {
        let draft = ledger::create_draft(
            conn,
            &CreateEntryInput {
                journal_code: "OD".into(),
                entry_date: "2026-01-01".into(),
                piece_ref: "IMMO-ACQ-001".into(),
                label: "Acquisition Machine atelier IMMO-001".into(),
                lines: vec![
                    EntryLineInput {
                        account_number: asset.account_number.clone(),
                        label: Some("Acquisition IMMO-001".into()),
                        debit_cents: asset.base_cents,
                        credit_cents: 0,
                        vat_code: None,
                        third_party_id: None,
                        comp_aux_type: None,
                        comp_aux_num: None,
                    },
                    EntryLineInput {
                        account_number: "512".into(),
                        label: Some("Règlement acquisition IMMO-001".into()),
                        debit_cents: 0,
                        credit_cents: asset.base_cents,
                        vat_code: None,
                        third_party_id: None,
                        comp_aux_type: None,
                        comp_aux_num: None,
                    },
                ],
            },
        )?;
        ledger::post_entry(conn, draft.id)?;
    }

    Ok(asset)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{create_demo_workspace, open_connection};
    use tempfile::tempdir;

    #[test]
    fn linear_annual_matches_expected() {
        // 5000 € / 10 ans = 500 €
        assert_eq!(annual_dotation_cents(500_000, 0, 10), 50_000);
        let asset = FixedAsset {
            id: 1,
            code: "X".into(),
            label: "X".into(),
            account_number: "215".into(),
            amort_account: "281".into(),
            expense_account: "681".into(),
            acquisition_date: "2026-01-01".into(),
            service_date: "2026-01-01".into(),
            base_cents: 500_000,
            residual_cents: 0,
            useful_life_years: 10,
            method: "linear".into(),
            active: true,
            notes: None,
            annual_dotation_cents: 50_000,
        };
        let plan = build_linear_schedule(&asset);
        assert_eq!(plan.len(), 10);
        assert_eq!(plan[0].amount_cents, 50_000);
        assert_eq!(plan[0].period_label, "2026");
        assert_eq!(plan.last().unwrap().cumulative_cents, 500_000);
    }

    #[test]
    fn generate_od_in_journal_od() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("j5.sascompta");
        create_demo_workspace(&path).unwrap();
        let conn = open_connection(&path).unwrap();

        // Demo seed already creates asset + 2026 run ; vérifier montants / journal
        let assets = list_assets(&conn).unwrap();
        assert!(!assets.is_empty());
        let asset = &assets[0];
        assert_eq!(asset.annual_dotation_cents, 50_000);

        let runs = list_depreciation_runs(&conn, Some(asset.id)).unwrap();
        assert!(!runs.is_empty(), "expected depreciation run from demo seed");
        let run = &runs[0];
        assert_eq!(run.amount_cents, 50_000);
        assert_eq!(run.journal_code.as_deref(), Some("OD"));
        assert!(run.entry_id.is_some());

        // Idempotence
        let again = generate_period_depreciation(&conn, "2026", "2026-01-01", "2026-12-31").unwrap();
        assert!(again.created.is_empty());
        assert!(!again.skipped.is_empty());
    }
}

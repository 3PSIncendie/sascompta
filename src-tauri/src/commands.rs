use crate::db::{self, AppState, DbError};
use crate::models::*;
use std::path::PathBuf;
use tauri::State;

fn map_err(e: DbError) -> String {
    e.to_string()
}

#[tauri::command]
pub fn create_workspace(
    state: State<'_, AppState>,
    input: CreateWorkspaceInput,
) -> Result<WorkspaceInfo, String> {
    let info = db::create_workspace(&input).map_err(map_err)?;
    let conn = db::open_connection(std::path::Path::new(&info.path)).map_err(map_err)?;
    *state.conn.lock().map_err(|e| e.to_string())? = Some(conn);
    *state.path.lock().map_err(|e| e.to_string())? = Some(PathBuf::from(&info.path));
    Ok(info)
}

#[tauri::command]
pub fn open_workspace(
    state: State<'_, AppState>,
    path: String,
    password: String,
) -> Result<WorkspaceInfo, String> {
    let (conn, info) = db::open_workspace(&path, &password).map_err(map_err)?;
    *state.conn.lock().map_err(|e| e.to_string())? = Some(conn);
    *state.path.lock().map_err(|e| e.to_string())? = Some(PathBuf::from(&path));
    Ok(info)
}

#[tauri::command]
pub fn create_demo_workspace(
    state: State<'_, AppState>,
    path: String,
) -> Result<WorkspaceInfo, String> {
    let p = PathBuf::from(&path);
    let _info = db::create_demo_workspace(&p).map_err(map_err)?;
    let (conn, info) = db::open_workspace(&path, db::DEMO_PASSWORD).map_err(map_err)?;
    *state.conn.lock().map_err(|e| e.to_string())? = Some(conn);
    *state.path.lock().map_err(|e| e.to_string())? = Some(p);
    Ok(info)
}

#[tauri::command]
pub fn get_workspace_info(state: State<'_, AppState>) -> Result<WorkspaceInfo, String> {
    let path_guard = state.path.lock().map_err(|e| e.to_string())?;
    let path = path_guard.as_ref().ok_or("Aucun workspace ouvert")?;
    db::with_conn(&state, |conn| db::load_workspace_info(conn, path)).map_err(map_err)
}

#[tauri::command]
pub fn update_organization(
    state: State<'_, AppState>,
    input: UpdateOrganizationInput,
) -> Result<WorkspaceInfo, String> {
    let path_guard = state.path.lock().map_err(|e| e.to_string())?;
    let path = path_guard.as_ref().ok_or("Aucun workspace ouvert")?.clone();
    drop(path_guard);
    db::with_conn(&state, |conn| db::update_organization(conn, &path, &input)).map_err(map_err)
}

#[tauri::command]
pub fn close_workspace(state: State<'_, AppState>) -> Result<(), String> {
    *state.conn.lock().map_err(|e| e.to_string())? = None;
    *state.path.lock().map_err(|e| e.to_string())? = None;
    Ok(())
}

#[tauri::command]
pub fn list_accounts(state: State<'_, AppState>) -> Result<Vec<Account>, String> {
    db::with_conn(&state, db::chart::list_accounts).map_err(map_err)
}

#[tauri::command]
pub fn create_account(
    state: State<'_, AppState>,
    number: String,
    label: String,
    account_type: String,
) -> Result<Account, String> {
    // JS may send accountType (camelCase) — accept via serde alias by wrapping if needed.
    db::with_conn(&state, |c| db::chart::create_account(c, &number, &label, &account_type))
        .map_err(map_err)
}

#[tauri::command]
pub fn update_account(
    state: State<'_, AppState>,
    id: i64,
    label: String,
    active: bool,
) -> Result<(), String> {
    db::with_conn(&state, |c| db::chart::update_account(c, id, &label, active)).map_err(map_err)
}

#[tauri::command]
pub fn delete_account(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::with_conn(&state, |c| db::chart::delete_account(c, id)).map_err(map_err)
}

#[tauri::command]
pub fn list_journals(state: State<'_, AppState>) -> Result<Vec<Journal>, String> {
    db::with_conn(&state, db::chart::list_journals).map_err(map_err)
}

#[tauri::command]
pub fn list_entries(state: State<'_, AppState>) -> Result<Vec<Entry>, String> {
    db::with_conn(&state, db::ledger::list_entries).map_err(map_err)
}

#[tauri::command]
pub fn get_entry(state: State<'_, AppState>, id: i64) -> Result<Entry, String> {
    db::with_conn(&state, |c| db::ledger::get_entry(c, id)).map_err(map_err)
}

#[tauri::command]
pub fn create_entry(state: State<'_, AppState>, input: CreateEntryInput) -> Result<Entry, String> {
    db::with_conn(&state, |c| db::ledger::create_draft(c, &input)).map_err(map_err)
}

#[tauri::command]
pub fn update_entry(
    state: State<'_, AppState>,
    id: i64,
    input: CreateEntryInput,
) -> Result<Entry, String> {
    db::with_conn(&state, |c| db::ledger::update_draft(c, id, &input)).map_err(map_err)
}

#[tauri::command]
pub fn delete_entry(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::with_conn(&state, |c| db::ledger::delete_draft(c, id)).map_err(map_err)
}

#[tauri::command]
pub fn post_entry(state: State<'_, AppState>, id: i64) -> Result<Entry, String> {
    db::with_conn(&state, |c| db::ledger::post_entry(c, id)).map_err(map_err)
}

#[tauri::command]
pub fn reverse_entry(state: State<'_, AppState>, id: i64) -> Result<Entry, String> {
    db::with_conn(&state, |c| db::ledger::reverse_entry(c, id)).map_err(map_err)
}

#[tauri::command]
pub fn add_attachment(
    state: State<'_, AppState>,
    entry_id: i64,
    file_name: String,
    file_path: String,
    mime_type: Option<String>,
) -> Result<Attachment, String> {
    db::with_conn(&state, |c| {
        db::ledger::add_attachment(c, entry_id, &file_name, &file_path, mime_type)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn ledger_movements(
    state: State<'_, AppState>,
    filter: LedgerFilter,
) -> Result<Vec<LedgerMovement>, String> {
    db::with_conn(&state, |c| db::reporting::ledger_movements(c, &filter)).map_err(map_err)
}

#[tauri::command]
pub fn balance_report(
    state: State<'_, AppState>,
    filter: LedgerFilter,
) -> Result<BalanceReport, String> {
    db::with_conn(&state, |c| db::reporting::balance_report(c, &filter)).map_err(map_err)
}

#[tauri::command]
pub fn export_balance_csv(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let report = db::reporting::balance_report(c, &filter)?;
        let csv = db::reporting::export_balance_csv(&report);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_ledger_csv(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let mov = db::reporting::ledger_movements(c, &filter)?;
        let csv = db::reporting::export_ledger_csv(&mov);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_ledger_pdf(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
    title: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let mov = db::reporting::ledger_movements(c, &filter)?;
        db::reporting::export_ledger_pdf(std::path::Path::new(&path), &title, &mov)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_balance_pdf(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
    title: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let report = db::reporting::balance_report(c, &filter)?;
        db::reporting::export_balance_pdf(std::path::Path::new(&path), &title, &report)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn list_vat_codes(state: State<'_, AppState>) -> Result<Vec<VatCode>, String> {
    db::with_conn(&state, db::tax::list_vat_codes).map_err(map_err)
}

#[tauri::command]
pub fn list_third_parties(state: State<'_, AppState>) -> Result<Vec<ThirdParty>, String> {
    db::with_conn(&state, db::tax::list_third_parties).map_err(map_err)
}

#[tauri::command]
pub fn create_third_party(
    state: State<'_, AppState>,
    input: CreateThirdPartyInput,
) -> Result<ThirdParty, String> {
    db::with_conn(&state, |c| db::tax::create_third_party(c, &input)).map_err(map_err)
}

#[tauri::command]
pub fn update_third_party(
    state: State<'_, AppState>,
    id: i64,
    name: String,
    active: bool,
) -> Result<(), String> {
    db::with_conn(&state, |c| db::tax::update_third_party(c, id, &name, active)).map_err(map_err)
}

#[tauri::command]
pub fn vat_report(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
) -> Result<VatReport, String> {
    db::with_conn(&state, |c| db::tax::vat_report(c, &from_date, &to_date)).map_err(map_err)
}

#[tauri::command]
pub fn auxiliary_balance(
    state: State<'_, AppState>,
    from_date: Option<String>,
    to_date: Option<String>,
) -> Result<AuxBalanceReport, String> {
    db::with_conn(&state, |c| {
        db::tax::auxiliary_balance(c, from_date.as_deref(), to_date.as_deref())
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_vat_csv(
    state: State<'_, AppState>,
    from_date: String,
    to_date: String,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let report = db::tax::vat_report(c, &from_date, &to_date)?;
        let csv = db::tax::export_vat_csv(&report);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_aux_csv(
    state: State<'_, AppState>,
    from_date: Option<String>,
    to_date: Option<String>,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let report = db::tax::auxiliary_balance(c, from_date.as_deref(), to_date.as_deref())?;
        let csv = db::tax::export_aux_csv(&report);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn list_fixed_assets(state: State<'_, AppState>) -> Result<Vec<FixedAsset>, String> {
    db::with_conn(&state, db::assets::list_assets).map_err(map_err)
}

#[tauri::command]
pub fn create_fixed_asset(
    state: State<'_, AppState>,
    input: CreateFixedAssetInput,
) -> Result<FixedAsset, String> {
    db::with_conn(&state, |c| db::assets::create_asset(c, &input)).map_err(map_err)
}

#[tauri::command]
pub fn update_fixed_asset(
    state: State<'_, AppState>,
    id: i64,
    label: String,
    active: bool,
    notes: Option<String>,
) -> Result<FixedAsset, String> {
    db::with_conn(&state, |c| db::assets::update_asset(c, id, &label, active, notes))
        .map_err(map_err)
}

#[tauri::command]
pub fn asset_schedule(
    state: State<'_, AppState>,
    asset_id: i64,
) -> Result<Vec<DepreciationScheduleRow>, String> {
    db::with_conn(&state, |c| db::assets::asset_schedule(c, asset_id)).map_err(map_err)
}

#[tauri::command]
pub fn list_depreciation_runs(
    state: State<'_, AppState>,
    asset_id: Option<i64>,
) -> Result<Vec<DepreciationRun>, String> {
    db::with_conn(&state, |c| db::assets::list_depreciation_runs(c, asset_id)).map_err(map_err)
}

#[tauri::command]
pub fn generate_depreciation(
    state: State<'_, AppState>,
    period_label: String,
    period_start: String,
    period_end: String,
) -> Result<GenerateDepreciationResult, String> {
    db::with_conn(&state, |c| {
        db::assets::generate_period_depreciation(c, &period_label, &period_start, &period_end)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_assets_csv(state: State<'_, AppState>, path: String) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let assets = db::assets::list_assets(c)?;
        let mut out = String::from(
            "Code;Libellé;Compte;Base;Résidu;Durée;Dotation annuelle;Mise en service\n",
        );
        for a in assets {
            out.push_str(&format!(
                "{};{};{};{};{};{};{};{}\n",
                a.code,
                a.label.replace(';', ","),
                a.account_number,
                format_cents_csv(a.base_cents),
                format_cents_csv(a.residual_cents),
                a.useful_life_years,
                format_cents_csv(a.annual_dotation_cents),
                a.service_date,
            ));
        }
        db::reporting::write_text_file(std::path::Path::new(&path), &out)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_schedule_csv(
    state: State<'_, AppState>,
    asset_id: i64,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let rows = db::assets::asset_schedule(c, asset_id)?;
        let mut out =
            String::from("Année;Période;Dotation;Cumul;VNC\n");
        for r in rows {
            out.push_str(&format!(
                "{};{};{};{};{}\n",
                r.year_index,
                r.period_label,
                format_cents_csv(r.amount_cents),
                format_cents_csv(r.cumulative_cents),
                format_cents_csv(r.net_book_value_cents),
            ));
        }
        db::reporting::write_text_file(std::path::Path::new(&path), &out)
    })
    .map_err(map_err)
}

fn format_cents_csv(cents: i64) -> String {
    let neg = cents < 0;
    let v = cents.abs();
    let s = format!("{},{:02}", v / 100, v % 100);
    if neg {
        format!("-{s}")
    } else {
        s
    }
}

#[tauri::command]
pub fn closing_checklist(state: State<'_, AppState>) -> Result<ClosingChecklist, String> {
    db::with_conn(&state, db::closing::closing_checklist).map_err(map_err)
}

#[tauri::command]
pub fn close_fiscal_year(state: State<'_, AppState>) -> Result<CloseYearResult, String> {
    db::with_conn(&state, db::closing::close_fiscal_year).map_err(map_err)
}

#[tauri::command]
pub fn list_fiscal_years(state: State<'_, AppState>) -> Result<Vec<FiscalYear>, String> {
    db::with_conn(&state, |c| {
        let mut stmt = c.prepare(
            "SELECT id, label, start_date, end_date, status FROM fiscal_year ORDER BY id",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(FiscalYear {
                id: r.get(0)?,
                label: r.get(1)?,
                start_date: r.get(2)?,
                end_date: r.get(3)?,
                status: r.get(4)?,
            })
        })?;
        Ok(rows.filter_map(|r| r.ok()).collect())
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_fec_draft(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        db::closing::write_fec_file(c, &filter, std::path::Path::new(&path))
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_publication_zip(
    state: State<'_, AppState>,
    fiscal_year_id: i64,
    path: String,
) -> Result<PublicationPackMeta, String> {
    db::with_conn(&state, |c| {
        let fy = db::closing::fiscal_year_by_id(c, fiscal_year_id)?;
        db::closing::export_publication_zip(c, &fy, std::path::Path::new(&path))
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn last_closing_event(state: State<'_, AppState>) -> Result<Option<ClosingEventInfo>, String> {
    db::with_conn(&state, db::closing::last_closing_event).map_err(map_err)
}

#[tauri::command]
pub fn annual_statements(
    state: State<'_, AppState>,
    filter: LedgerFilter,
) -> Result<AnnualStatements, String> {
    db::with_conn(&state, |c| db::statements::build_annual_pack(c, &filter)).map_err(map_err)
}

#[tauri::command]
pub fn export_bilan_csv(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let sheet = db::statements::build_balance_sheet(c, &filter)?;
        let csv = db::statements::export_bilan_csv(&sheet);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_cr_csv(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let cr = db::statements::build_income_statement(c, &filter)?;
        let csv = db::statements::export_cr_csv(&cr);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_annexe_csv(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let annexe = db::statements::build_annex(c, &filter)?;
        let csv = db::statements::export_annexe_csv(&annexe);
        db::reporting::write_text_file(std::path::Path::new(&path), &csv)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_annual_pdf(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    path: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let pack = db::statements::build_annual_pack(c, &filter)?;
        db::statements::export_annual_pdf(std::path::Path::new(&path), &pack)
    })
    .map_err(map_err)
}

#[tauri::command]
pub fn export_annual_pack(
    state: State<'_, AppState>,
    filter: LedgerFilter,
    dir: String,
) -> Result<(), String> {
    db::with_conn(&state, |c| {
        let pack = db::statements::build_annual_pack(c, &filter)?;
        db::statements::export_pack_files(std::path::Path::new(&dir), &pack)
    })
    .map_err(map_err)
}

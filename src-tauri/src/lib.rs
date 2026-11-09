mod commands;
pub mod db;
mod models;
mod security;

use db::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init());

    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_process::init());
    }

    builder
        .setup(|app| {
            #[cfg(desktop)]
            app.handle()
                .plugin(tauri_plugin_updater::Builder::new().build())?;
            Ok(())
        })
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::create_workspace,
            commands::open_workspace,
            commands::create_demo_workspace,
            commands::get_workspace_info,
            commands::update_organization,
            commands::close_workspace,
            commands::list_accounts,
            commands::create_account,
            commands::update_account,
            commands::delete_account,
            commands::list_journals,
            commands::list_entries,
            commands::get_entry,
            commands::create_entry,
            commands::update_entry,
            commands::delete_entry,
            commands::post_entry,
            commands::reverse_entry,
            commands::add_attachment,
            commands::ledger_movements,
            commands::balance_report,
            commands::export_balance_csv,
            commands::export_ledger_csv,
            commands::export_ledger_pdf,
            commands::export_balance_pdf,
            commands::list_vat_codes,
            commands::list_third_parties,
            commands::create_third_party,
            commands::update_third_party,
            commands::vat_report,
            commands::auxiliary_balance,
            commands::export_vat_csv,
            commands::export_aux_csv,
            commands::list_fixed_assets,
            commands::create_fixed_asset,
            commands::update_fixed_asset,
            commands::asset_schedule,
            commands::list_depreciation_runs,
            commands::generate_depreciation,
            commands::export_assets_csv,
            commands::export_schedule_csv,
            commands::annual_statements,
            commands::export_bilan_csv,
            commands::export_cr_csv,
            commands::export_annexe_csv,
            commands::export_annual_pdf,
            commands::export_annual_pack,
            commands::closing_checklist,
            commands::close_fiscal_year,
            commands::list_fiscal_years,
            commands::export_fec_draft,
            commands::export_publication_zip,
            commands::last_closing_event,
            commands::list_quotes,
            commands::get_quote,
            commands::create_quote,
            commands::update_quote,
            commands::set_quote_status,
            commands::delete_quote,
            commands::export_quote_pdf,
            commands::list_invoices,
            commands::get_invoice,
            commands::convert_quote_to_invoice,
            commands::build_invoice_entry_draft,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de SasCompta");
}

mod commands;
pub mod db;
mod models;
mod registry;
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
            commands::list_known_workspaces,
            commands::forget_known_workspace,
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
            commands::create_invoice,
            commands::update_invoice,
            commands::delete_invoice,
            commands::issue_invoice,
            commands::create_credit_note,
            commands::create_deposit_invoice,
            commands::export_invoice_pdf,
            commands::convert_quote_to_invoice,
            commands::build_invoice_entry_draft,
            commands::list_payments,
            commands::record_payment,
            commands::list_reminders,
            commands::generate_due_reminders,
            commands::mark_reminder_sent,
            commands::get_einvoice_profile,
            commands::update_einvoice_profile,
            commands::list_einvoice_queue,
            commands::export_facturx,
            commands::import_einvoice_file,
            commands::pdp_stub_submit,
            commands::list_ereporting_batches,
            commands::build_ereporting_batch,
            commands::list_employees,
            commands::get_employee,
            commands::create_employee,
            commands::update_employee,
            commands::delete_employee,
            commands::list_contracts,
            commands::create_contract,
            commands::update_contract,
            commands::delete_contract,
            commands::list_leave_balances,
            commands::upsert_leave_balance,
            commands::list_leave_requests,
            commands::create_leave_request,
            commands::set_leave_status,
            commands::delete_leave_request,
            commands::list_social_rates,
            commands::upsert_social_rate,
            commands::list_payslips,
            commands::get_payslip,
            commands::compute_payslip,
            commands::generate_period_payslips,
            commands::validate_payslip,
            commands::delete_payslip,
            commands::build_payslip_entry_draft,
            commands::export_dsn_oriented,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au démarrage de SasCompta");
}

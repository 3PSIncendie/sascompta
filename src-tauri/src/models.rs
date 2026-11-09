use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Organization {
    pub id: i64,
    pub name: String,
    pub siren: String,
    pub siret: Option<String>,
    pub legal_form: String,
    pub vat_regime: String,
    pub address: Option<String>,
    pub capital_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateOrganizationInput {
    pub name: String,
    pub siren: String,
    pub siret: Option<String>,
    pub legal_form: String,
    pub vat_regime: String,
    pub address: Option<String>,
    pub capital_cents: i64,
    pub display_name: String,
    pub fiscal_year_label: String,
    pub start_date: String,
    pub end_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocalProfile {
    pub id: i64,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FiscalYear {
    pub id: i64,
    pub label: String,
    pub start_date: String,
    pub end_date: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInfo {
    pub path: String,
    pub organization: Organization,
    pub fiscal_year: FiscalYear,
    pub profile: LocalProfile,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateWorkspaceInput {
    pub path: String,
    pub password: String,
    pub org_name: String,
    pub siren: String,
    pub fiscal_year_label: String,
    pub start_date: String,
    pub end_date: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: i64,
    pub number: String,
    pub label: String,
    pub class: i32,
    pub account_type: String,
    pub is_auxiliary: bool,
    pub is_custom: bool,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Journal {
    pub id: i64,
    pub code: String,
    pub label: String,
    pub journal_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryLineInput {
    pub account_number: String,
    pub label: Option<String>,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub vat_code: Option<String>,
    pub third_party_id: Option<i64>,
    pub comp_aux_type: Option<String>,
    pub comp_aux_num: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryLine {
    pub id: i64,
    pub entry_id: i64,
    pub line_no: i32,
    pub account_number: String,
    pub account_label: String,
    pub label: Option<String>,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub vat_code: Option<String>,
    pub third_party_id: Option<i64>,
    pub third_party_code: Option<String>,
    pub third_party_name: Option<String>,
    pub comp_aux_type: Option<String>,
    pub comp_aux_num: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub id: i64,
    pub journal_code: String,
    pub entry_date: String,
    pub piece_ref: String,
    pub label: String,
    pub status: String,
    pub fiscal_year_id: i64,
    pub ecriture_num: Option<String>,
    pub reversal_of_id: Option<i64>,
    pub created_at: String,
    pub posted_at: Option<String>,
    pub lines: Vec<EntryLine>,
    pub attachments: Vec<Attachment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEntryInput {
    pub journal_code: String,
    pub entry_date: String,
    pub piece_ref: String,
    pub label: String,
    pub lines: Vec<EntryLineInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    pub id: i64,
    pub entry_id: i64,
    pub file_name: String,
    pub file_path: String,
    pub mime_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VatCode {
    pub code: String,
    pub label: String,
    pub rate_bp: i32,
    pub direction: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThirdParty {
    pub id: i64,
    pub code: String,
    pub name: String,
    pub kind: String,
    pub account_number: String,
    pub active: bool,
    #[serde(default)]
    pub siret: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub routing_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateThirdPartyInput {
    pub code: String,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub siret: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub address: Option<String>,
    #[serde(default)]
    pub routing_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerFilter {
    pub from_date: Option<String>,
    pub to_date: Option<String>,
    pub journal_code: Option<String>,
    pub account_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerMovement {
    pub entry_id: i64,
    pub entry_date: String,
    pub journal_code: String,
    pub piece_ref: String,
    pub ecriture_num: Option<String>,
    pub entry_label: String,
    pub account_number: String,
    pub account_label: String,
    pub line_label: Option<String>,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub vat_code: Option<String>,
    pub third_party_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceRow {
    pub account_number: String,
    pub account_label: String,
    pub class: i32,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub solde_debit_cents: i64,
    pub solde_credit_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceReport {
    pub rows: Vec<BalanceRow>,
    pub total_debit_cents: i64,
    pub total_credit_cents: i64,
    pub ecart_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VatReportLine {
    pub code: String,
    pub label: String,
    pub direction: String,
    pub rate_bp: i32,
    pub base_cents: i64,
    pub vat_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VatReport {
    pub lines: Vec<VatReportLine>,
    pub collected_cents: i64,
    pub deductible_cents: i64,
    pub net_cents: i64,
    pub from_date: String,
    pub to_date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxBalanceRow {
    pub account_number: String,
    pub account_label: String,
    pub aux_code: String,
    pub aux_label: String,
    pub debit_cents: i64,
    pub credit_cents: i64,
    pub solde_debit_cents: i64,
    pub solde_credit_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AuxBalanceReport {
    pub rows: Vec<AuxBalanceRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FixedAsset {
    pub id: i64,
    pub code: String,
    pub label: String,
    pub account_number: String,
    pub amort_account: String,
    pub expense_account: String,
    pub acquisition_date: String,
    pub service_date: String,
    pub base_cents: i64,
    pub residual_cents: i64,
    pub useful_life_years: i32,
    pub method: String,
    pub active: bool,
    pub notes: Option<String>,
    pub annual_dotation_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateFixedAssetInput {
    pub code: String,
    pub label: String,
    pub account_number: String,
    pub amort_account: Option<String>,
    pub expense_account: Option<String>,
    pub acquisition_date: String,
    pub service_date: String,
    pub base_cents: i64,
    pub residual_cents: i64,
    pub useful_life_years: i32,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepreciationScheduleRow {
    pub year_index: i32,
    pub period_label: String,
    pub amount_cents: i64,
    pub cumulative_cents: i64,
    pub net_book_value_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepreciationRun {
    pub id: i64,
    pub asset_id: i64,
    pub asset_code: String,
    pub asset_label: String,
    pub period_label: String,
    pub period_start: String,
    pub period_end: String,
    pub amount_cents: i64,
    pub entry_id: Option<i64>,
    pub ecriture_num: Option<String>,
    pub journal_code: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GenerateDepreciationResult {
    pub period_label: String,
    pub created: Vec<DepreciationRun>,
    pub skipped: Vec<String>,
    pub total_amount_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementLine {
    pub code: String,
    pub label: String,
    pub amount_cents: i64,
    pub accounts: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSheet {
    pub as_of: String,
    pub from_date: String,
    pub to_date: String,
    pub actif: Vec<StatementLine>,
    pub passif: Vec<StatementLine>,
    pub total_actif_cents: i64,
    pub total_passif_cents: i64,
    pub ecart_cents: i64,
    pub resultat_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeStatement {
    pub from_date: String,
    pub to_date: String,
    pub charges: Vec<StatementLine>,
    pub produits: Vec<StatementLine>,
    pub total_charges_cents: i64,
    pub total_produits_cents: i64,
    pub resultat_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnexAssetRow {
    pub code: String,
    pub label: String,
    pub account_number: String,
    pub gross_cents: i64,
    pub cumulative_amort_cents: i64,
    pub net_cents: i64,
    pub useful_life_years: i32,
    pub method: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnexDebtRow {
    pub account_number: String,
    pub label: String,
    pub amount_cents: i64,
    pub nature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnualAnnex {
    pub as_of: String,
    pub methods: Vec<String>,
    pub immobilisations: Vec<AnnexAssetRow>,
    pub dettes: Vec<AnnexDebtRow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnnualStatements {
    pub organization_name: String,
    pub siren: String,
    pub bilan: BalanceSheet,
    pub compte_resultat: IncomeStatement,
    pub annexe: AnnualAnnex,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChecklistItem {
    pub code: String,
    pub label: String,
    pub ok: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosingChecklist {
    pub fiscal_year: FiscalYear,
    pub items: Vec<ChecklistItem>,
    pub all_ok: bool,
    pub resultat_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseYearResult {
    pub closed_year: FiscalYear,
    pub next_year: FiscalYear,
    pub an_entry_id: i64,
    pub an_ecriture_num: Option<String>,
    pub resultat_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicationPackMeta {
    pub organization_name: String,
    pub siren: String,
    pub fiscal_year_label: String,
    pub from_date: String,
    pub to_date: String,
    pub fy_status: String,
    pub resultat_cents: i64,
    pub total_actif_cents: i64,
    pub total_passif_cents: i64,
    pub balance_ecart_cents: i64,
    pub generated_at: String,
    pub disclaimer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClosingEventInfo {
    pub id: i64,
    pub fiscal_year_id: i64,
    pub closed_at: String,
    pub an_entry_id: Option<i64>,
    pub next_fiscal_year_id: Option<i64>,
    pub pack_path: Option<String>,
    pub notes: Option<String>,
}

/* ——— Commercial / billing ——— */

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteLine {
    pub id: i64,
    pub quote_id: i64,
    pub line_no: i32,
    pub label: String,
    pub quantity_milli: i64,
    pub unit: String,
    pub unit_price_cents: i64,
    pub ht_cents: i64,
    pub vat_rate_bp: i32,
    pub vat_code: Option<String>,
    pub account_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuoteLineInput {
    pub label: String,
    pub quantity_milli: i64,
    pub unit: String,
    pub unit_price_cents: i64,
    #[serde(default)]
    pub ht_cents: i64,
    pub vat_rate_bp: i32,
    pub vat_code: Option<String>,
    pub account_number: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Quote {
    pub id: i64,
    pub number: String,
    pub third_party_id: i64,
    pub third_party_code: String,
    pub third_party_name: String,
    pub status: String,
    pub issue_date: String,
    pub valid_until: Option<String>,
    pub currency: String,
    pub ht_cents: i64,
    pub vat_cents: i64,
    pub ttc_cents: i64,
    pub vat_mode: String,
    pub notes: Option<String>,
    pub legal_mentions: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<QuoteLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateQuoteInput {
    pub third_party_id: i64,
    pub issue_date: String,
    pub valid_until: Option<String>,
    pub vat_mode: String,
    pub notes: Option<String>,
    pub legal_mentions: Option<String>,
    pub lines: Vec<QuoteLineInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLine {
    pub id: i64,
    pub invoice_id: i64,
    pub line_no: i32,
    pub label: String,
    pub quantity_milli: i64,
    pub unit: String,
    pub unit_price_cents: i64,
    pub ht_cents: i64,
    pub vat_rate_bp: i32,
    pub vat_code: Option<String>,
    pub account_number: String,
    pub analytic_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Invoice {
    pub id: i64,
    pub number: Option<String>,
    pub kind: String,
    pub third_party_id: i64,
    pub third_party_code: String,
    pub third_party_name: String,
    pub quote_id: Option<i64>,
    pub parent_invoice_id: Option<i64>,
    pub status: String,
    pub issue_date: String,
    pub due_date: Option<String>,
    pub service_date: Option<String>,
    pub ht_cents: i64,
    pub vat_cents: i64,
    pub ttc_cents: i64,
    pub paid_cents: i64,
    pub vat_mode: String,
    pub vat_code: Option<String>,
    pub einvoice_profile: String,
    pub einvoice_status: String,
    pub entry_id: Option<i64>,
    pub pdf_path: Option<String>,
    pub facturx_path: Option<String>,
    pub notes: Option<String>,
    pub legal_mentions: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<InvoiceLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvoiceLineInput {
    pub label: String,
    pub quantity_milli: i64,
    pub unit: String,
    pub unit_price_cents: i64,
    #[serde(default)]
    pub ht_cents: i64,
    pub vat_rate_bp: i32,
    pub vat_code: Option<String>,
    pub account_number: String,
    pub analytic_code: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateInvoiceInput {
    pub kind: String,
    pub third_party_id: i64,
    pub quote_id: Option<i64>,
    pub parent_invoice_id: Option<i64>,
    pub issue_date: String,
    pub due_date: Option<String>,
    pub service_date: Option<String>,
    pub vat_mode: String,
    pub notes: Option<String>,
    pub legal_mentions: Option<String>,
    pub lines: Vec<InvoiceLineInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payment {
    pub id: i64,
    pub invoice_id: i64,
    pub invoice_number: Option<String>,
    pub third_party_code: String,
    pub third_party_name: String,
    pub paid_on: String,
    pub amount_cents: i64,
    pub method: String,
    pub bank_ref: Option<String>,
    pub entry_id: Option<i64>,
    pub lettre: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePaymentInput {
    pub invoice_id: i64,
    pub paid_on: String,
    pub amount_cents: i64,
    pub method: String,
    pub bank_ref: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reminder {
    pub id: i64,
    pub invoice_id: i64,
    pub invoice_number: Option<String>,
    pub third_party_code: String,
    pub third_party_name: String,
    pub level: i32,
    pub due_date: Option<String>,
    pub days_overdue: i32,
    pub amount_due_cents: i64,
    pub sent_at: Option<String>,
    pub channel: String,
    pub body_snapshot: Option<String>,
}

// —— RH (M8/M9) ——————————————————————————————————————————————————————————

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Employee {
    pub id: i64,
    pub code: String,
    pub first_name: String,
    pub last_name: String,
    pub birth_date: Option<String>,
    pub address: Option<String>,
    pub email: Option<String>,
    pub iban_ref: Option<String>,
    pub active: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateEmployeeInput {
    pub code: String,
    pub first_name: String,
    pub last_name: String,
    pub birth_date: Option<String>,
    pub address: Option<String>,
    pub email: Option<String>,
    pub iban_ref: Option<String>,
    #[serde(default = "default_true")]
    pub active: bool,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmploymentContract {
    pub id: i64,
    pub employee_id: i64,
    pub employee_code: String,
    pub employee_name: String,
    pub kind: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub job_title: String,
    pub weekly_hours_bp: i64,
    pub gross_monthly_cents: i64,
    pub convention_ref: Option<String>,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateContractInput {
    pub employee_id: i64,
    pub kind: String,
    pub start_date: String,
    pub end_date: Option<String>,
    pub job_title: String,
    pub weekly_hours_bp: i64,
    pub gross_monthly_cents: i64,
    pub convention_ref: Option<String>,
    #[serde(default = "default_active_status")]
    pub status: String,
}

fn default_active_status() -> String {
    "active".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveBalance {
    pub employee_id: i64,
    pub employee_code: String,
    pub employee_name: String,
    pub year: i32,
    pub acquired_days_bp: i64,
    pub taken_days_bp: i64,
    pub remaining_days_bp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeaveRequest {
    pub id: i64,
    pub employee_id: i64,
    pub employee_code: String,
    pub employee_name: String,
    pub from_date: String,
    pub to_date: String,
    pub days_bp: i64,
    pub status: String,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateLeaveRequestInput {
    pub employee_id: i64,
    pub from_date: String,
    pub to_date: String,
    pub days_bp: i64,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SocialRateTable {
    pub id: i64,
    pub valid_from: String,
    pub payload_json: String,
    pub label: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PayslipLine {
    pub id: i64,
    pub payslip_id: i64,
    pub line_no: i32,
    pub code: String,
    pub label: String,
    pub base_cents: i64,
    pub rate_bp: i32,
    pub employee_cents: i64,
    pub employer_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Payslip {
    pub id: i64,
    pub employee_id: i64,
    pub employee_code: String,
    pub employee_name: String,
    pub contract_id: Option<i64>,
    pub period_ym: String,
    pub status: String,
    pub gross_cents: i64,
    pub employee_charges_cents: i64,
    pub employer_charges_cents: i64,
    pub net_cents: i64,
    pub pas_rate_bp: Option<i32>,
    pub pas_cents: i64,
    pub entry_id: Option<i64>,
    pub pdf_path: Option<String>,
    pub notes: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub lines: Vec<PayslipLine>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DsnExportRun {
    pub id: i64,
    pub period_ym: String,
    pub status: String,
    pub path: Option<String>,
    pub checklist_json: String,
    pub created_at: String,
}

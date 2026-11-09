import { invoke } from "@tauri-apps/api/core";
import type {
  Account,
  AnnualStatements,
  Attachment,
  AuxBalanceReport,
  BalanceReport,
  CloseYearResult,
  ClosingChecklist,
  ClosingEventInfo,
  CreateContractInput,
  CreateEmployeeInput,
  CreateEntryInput,
  CreateFixedAssetInput,
  CreateEreportingBatchInput,
  CreateInvoiceInput,
  CreateLeaveRequestInput,
  CreatePaymentInput,
  CreateQuoteInput,
  CreateThirdPartyInput,
  CreateWorkspaceInput,
  DocumentSequenceInfo,
  KnownWorkspace,
  EinvoicePartyProfile,
  SetDocumentSequenceInput,
  EinvoiceQueueItem,
  EreportingBatch,
  DepreciationRun,
  DepreciationScheduleRow,
  DsnExportRun,
  Employee,
  EmploymentContract,
  Entry,
  FiscalYear,
  FixedAsset,
  GenerateDepreciationResult,
  Invoice,
  Journal,
  LeaveBalance,
  LeaveRequest,
  LedgerFilter,
  LedgerMovement,
  Payment,
  Payslip,
  PublicationPackMeta,
  Quote,
  Reminder,
  SocialRateTable,
  ThirdParty,
  UpdateEinvoiceProfileInput,
  UpdateOrganizationInput,
  VatCode,
  VatReport,
  WorkspaceInfo,
} from "./types";

export const api = {
  listKnownWorkspaces: () => invoke<KnownWorkspace[]>("list_known_workspaces"),
  forgetKnownWorkspace: (path: string) =>
    invoke<void>("forget_known_workspace", { path }),
  createWorkspace: (input: CreateWorkspaceInput) =>
    invoke<WorkspaceInfo>("create_workspace", { input }),
  openWorkspace: (path: string, password: string) =>
    invoke<WorkspaceInfo>("open_workspace", { path, password }),
  createDemoWorkspace: (path?: string | null) =>
    invoke<WorkspaceInfo>("create_demo_workspace", { path: path ?? null }),
  getWorkspaceInfo: () => invoke<WorkspaceInfo>("get_workspace_info"),
  updateOrganization: (input: UpdateOrganizationInput) =>
    invoke<WorkspaceInfo>("update_organization", { input }),
  closeWorkspace: () => invoke<void>("close_workspace"),

  listAccounts: () => invoke<Account[]>("list_accounts"),
  createAccount: (number: string, label: string, accountType: string) =>
    invoke<Account>("create_account", {
      number,
      label,
      accountType,
    }),
  updateAccount: (id: number, label: string, active: boolean) =>
    invoke<void>("update_account", { id, label, active }),
  deleteAccount: (id: number) => invoke<void>("delete_account", { id }),

  listJournals: () => invoke<Journal[]>("list_journals"),

  listEntries: () => invoke<Entry[]>("list_entries"),
  getEntry: (id: number) => invoke<Entry>("get_entry", { id }),
  createEntry: (input: CreateEntryInput) =>
    invoke<Entry>("create_entry", { input }),
  updateEntry: (id: number, input: CreateEntryInput) =>
    invoke<Entry>("update_entry", { id, input }),
  deleteEntry: (id: number) => invoke<void>("delete_entry", { id }),
  postEntry: (id: number) => invoke<Entry>("post_entry", { id }),
  reverseEntry: (id: number) => invoke<Entry>("reverse_entry", { id }),
  addAttachment: (
    entryId: number,
    fileName: string,
    filePath: string,
    mimeType?: string | null,
  ) =>
    invoke<Attachment>("add_attachment", {
      entryId,
      fileName,
      filePath,
      mimeType: mimeType ?? null,
    }),

  ledgerMovements: (filter: LedgerFilter) =>
    invoke<LedgerMovement[]>("ledger_movements", { filter }),
  balanceReport: (filter: LedgerFilter) =>
    invoke<BalanceReport>("balance_report", { filter }),
  exportBalanceCsv: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_balance_csv", { filter, path }),
  exportLedgerCsv: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_ledger_csv", { filter, path }),
  exportLedgerPdf: (filter: LedgerFilter, path: string, title: string) =>
    invoke<void>("export_ledger_pdf", { filter, path, title }),
  exportBalancePdf: (filter: LedgerFilter, path: string, title: string) =>
    invoke<void>("export_balance_pdf", { filter, path, title }),

  listVatCodes: () => invoke<VatCode[]>("list_vat_codes"),
  listThirdParties: () => invoke<ThirdParty[]>("list_third_parties"),
  createThirdParty: (input: CreateThirdPartyInput) =>
    invoke<ThirdParty>("create_third_party", { input }),
  updateThirdParty: (id: number, name: string, active: boolean) =>
    invoke<void>("update_third_party", { id, name, active }),
  vatReport: (fromDate: string, toDate: string) =>
    invoke<VatReport>("vat_report", { fromDate, toDate }),
  auxiliaryBalance: (fromDate?: string | null, toDate?: string | null) =>
    invoke<AuxBalanceReport>("auxiliary_balance", { fromDate, toDate }),
  exportVatCsv: (fromDate: string, toDate: string, path: string) =>
    invoke<void>("export_vat_csv", { fromDate, toDate, path }),
  exportAuxCsv: (
    fromDate: string | null | undefined,
    toDate: string | null | undefined,
    path: string,
  ) => invoke<void>("export_aux_csv", { fromDate, toDate, path }),

  listFixedAssets: () => invoke<FixedAsset[]>("list_fixed_assets"),
  createFixedAsset: (input: CreateFixedAssetInput) =>
    invoke<FixedAsset>("create_fixed_asset", { input }),
  updateFixedAsset: (
    id: number,
    label: string,
    active: boolean,
    notes?: string | null,
  ) =>
    invoke<FixedAsset>("update_fixed_asset", {
      id,
      label,
      active,
      notes: notes ?? null,
    }),
  assetSchedule: (assetId: number) =>
    invoke<DepreciationScheduleRow[]>("asset_schedule", { assetId }),
  listDepreciationRuns: (assetId?: number | null) =>
    invoke<DepreciationRun[]>("list_depreciation_runs", {
      assetId: assetId ?? null,
    }),
  generateDepreciation: (
    periodLabel: string,
    periodStart: string,
    periodEnd: string,
  ) =>
    invoke<GenerateDepreciationResult>("generate_depreciation", {
      periodLabel,
      periodStart,
      periodEnd,
    }),
  exportAssetsCsv: (path: string) =>
    invoke<void>("export_assets_csv", { path }),
  exportScheduleCsv: (assetId: number, path: string) =>
    invoke<void>("export_schedule_csv", { assetId, path }),

  annualStatements: (filter: LedgerFilter) =>
    invoke<AnnualStatements>("annual_statements", { filter }),
  exportBilanCsv: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_bilan_csv", { filter, path }),
  exportCrCsv: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_cr_csv", { filter, path }),
  exportAnnexeCsv: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_annexe_csv", { filter, path }),
  exportAnnualPdf: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_annual_pdf", { filter, path }),
  exportAnnualPack: (filter: LedgerFilter, dir: string) =>
    invoke<void>("export_annual_pack", { filter, dir }),

  closingChecklist: () => invoke<ClosingChecklist>("closing_checklist"),
  closeFiscalYear: () => invoke<CloseYearResult>("close_fiscal_year"),
  listFiscalYears: () => invoke<FiscalYear[]>("list_fiscal_years"),
  exportFecDraft: (filter: LedgerFilter, path: string) =>
    invoke<void>("export_fec_draft", { filter, path }),
  exportPublicationZip: (fiscalYearId: number, path: string) =>
    invoke<PublicationPackMeta>("export_publication_zip", {
      fiscalYearId,
      path,
    }),
  lastClosingEvent: () =>
    invoke<ClosingEventInfo | null>("last_closing_event"),

  listQuotes: () => invoke<Quote[]>("list_quotes"),
  getQuote: (id: number) => invoke<Quote>("get_quote", { id }),
  createQuote: (input: CreateQuoteInput) =>
    invoke<Quote>("create_quote", { input }),
  updateQuote: (id: number, input: CreateQuoteInput) =>
    invoke<Quote>("update_quote", { id, input }),
  setQuoteStatus: (id: number, status: string) =>
    invoke<Quote>("set_quote_status", { id, status }),
  deleteQuote: (id: number) => invoke<void>("delete_quote", { id }),
  exportQuotePdf: (id: number, path: string) =>
    invoke<void>("export_quote_pdf", { id, path }),

  listInvoices: () => invoke<Invoice[]>("list_invoices"),
  getInvoice: (id: number) => invoke<Invoice>("get_invoice", { id }),
  createInvoice: (input: CreateInvoiceInput) =>
    invoke<Invoice>("create_invoice", { input }),
  updateInvoice: (id: number, input: CreateInvoiceInput) =>
    invoke<Invoice>("update_invoice", { id, input }),
  deleteInvoice: (id: number) => invoke<void>("delete_invoice", { id }),
  issueInvoice: (id: number) => invoke<Invoice>("issue_invoice", { id }),
  createCreditNote: (invoiceId: number) =>
    invoke<Invoice>("create_credit_note", { invoiceId }),
  createDepositInvoice: (
    parentInvoiceId: number,
    amountHtCents: number,
    vatRateBp: number,
    vatCode?: string | null,
  ) =>
    invoke<Invoice>("create_deposit_invoice", {
      parentInvoiceId,
      amountHtCents,
      vatRateBp,
      vatCode: vatCode ?? null,
    }),
  exportInvoicePdf: (id: number, path: string) =>
    invoke<void>("export_invoice_pdf", { id, path }),
  convertQuoteToInvoice: (quoteId: number) =>
    invoke<Invoice>("convert_quote_to_invoice", { quoteId }),
  buildInvoiceEntryDraft: (invoiceId: number) =>
    invoke<Entry>("build_invoice_entry_draft", { invoiceId }),

  listPayments: (invoiceId?: number | null) =>
    invoke<Payment[]>("list_payments", { invoiceId: invoiceId ?? null }),
  recordPayment: (input: CreatePaymentInput) =>
    invoke<Payment>("record_payment", { input }),
  listReminders: () => invoke<Reminder[]>("list_reminders"),
  generateDueReminders: () => invoke<Reminder[]>("generate_due_reminders"),
  markReminderSent: (id: number) =>
    invoke<Reminder>("mark_reminder_sent", { id }),

  listDocumentSequences: () =>
    invoke<DocumentSequenceInfo[]>("list_document_sequences"),
  setDocumentSequences: (inputs: SetDocumentSequenceInput[]) =>
    invoke<DocumentSequenceInfo[]>("set_document_sequences", { inputs }),

  getEinvoiceProfile: () =>
    invoke<EinvoicePartyProfile>("get_einvoice_profile"),
  updateEinvoiceProfile: (input: UpdateEinvoiceProfileInput) =>
    invoke<EinvoicePartyProfile>("update_einvoice_profile", { input }),
  listEinvoiceQueue: () =>
    invoke<EinvoiceQueueItem[]>("list_einvoice_queue"),
  exportFacturx: (invoiceId: number, destDir: string) =>
    invoke<EinvoiceQueueItem>("export_facturx", { invoiceId, destDir }),
  importEinvoiceFile: (path: string) =>
    invoke<EinvoiceQueueItem>("import_einvoice_file", { path }),
  pdpStubSubmit: (queueId: number) =>
    invoke<EinvoiceQueueItem>("pdp_stub_submit", { queueId }),
  listEreportingBatches: () =>
    invoke<EreportingBatch[]>("list_ereporting_batches"),
  buildEreportingBatch: (input: CreateEreportingBatchInput) =>
    invoke<EreportingBatch>("build_ereporting_batch", { input }),

  listEmployees: () => invoke<Employee[]>("list_employees"),
  getEmployee: (id: number) => invoke<Employee>("get_employee", { id }),
  createEmployee: (input: CreateEmployeeInput) =>
    invoke<Employee>("create_employee", { input }),
  updateEmployee: (id: number, input: CreateEmployeeInput) =>
    invoke<Employee>("update_employee", { id, input }),
  deleteEmployee: (id: number) => invoke<void>("delete_employee", { id }),

  listContracts: (employeeId?: number | null) =>
    invoke<EmploymentContract[]>("list_contracts", {
      employeeId: employeeId ?? null,
    }),
  createContract: (input: CreateContractInput) =>
    invoke<EmploymentContract>("create_contract", { input }),
  updateContract: (id: number, input: CreateContractInput) =>
    invoke<EmploymentContract>("update_contract", { id, input }),
  deleteContract: (id: number) => invoke<void>("delete_contract", { id }),

  listLeaveBalances: (year?: number | null) =>
    invoke<LeaveBalance[]>("list_leave_balances", { year: year ?? null }),
  upsertLeaveBalance: (
    employeeId: number,
    year: number,
    acquiredDaysBp: number,
  ) =>
    invoke<LeaveBalance>("upsert_leave_balance", {
      employeeId,
      year,
      acquiredDaysBp,
    }),
  listLeaveRequests: (employeeId?: number | null) =>
    invoke<LeaveRequest[]>("list_leave_requests", {
      employeeId: employeeId ?? null,
    }),
  createLeaveRequest: (input: CreateLeaveRequestInput) =>
    invoke<LeaveRequest>("create_leave_request", { input }),
  setLeaveStatus: (id: number, status: string) =>
    invoke<LeaveRequest>("set_leave_status", { id, status }),
  deleteLeaveRequest: (id: number) =>
    invoke<void>("delete_leave_request", { id }),

  listSocialRates: () => invoke<SocialRateTable[]>("list_social_rates"),
  upsertSocialRate: (
    validFrom: string,
    employeeRateBp: number,
    employerRateBp: number,
    label: string,
  ) =>
    invoke<SocialRateTable>("upsert_social_rate", {
      validFrom,
      employeeRateBp,
      employerRateBp,
      label,
    }),
  listPayslips: (periodYm?: string | null) =>
    invoke<Payslip[]>("list_payslips", { periodYm: periodYm ?? null }),
  getPayslip: (id: number) => invoke<Payslip>("get_payslip", { id }),
  computePayslip: (
    employeeId: number,
    periodYm: string,
    pasRateBp?: number | null,
  ) =>
    invoke<Payslip>("compute_payslip", {
      employeeId,
      periodYm,
      pasRateBp: pasRateBp ?? null,
    }),
  generatePeriodPayslips: (periodYm: string) =>
    invoke<Payslip[]>("generate_period_payslips", { periodYm }),
  validatePayslip: (id: number) => invoke<Payslip>("validate_payslip", { id }),
  deletePayslip: (id: number) => invoke<void>("delete_payslip", { id }),
  buildPayslipEntryDraft: (payslipId: number) =>
    invoke<Entry>("build_payslip_entry_draft", { payslipId }),
  exportDsnOriented: (periodYm: string, path: string) =>
    invoke<DsnExportRun>("export_dsn_oriented", { periodYm, path }),
};

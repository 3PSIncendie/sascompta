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
  CreateEntryInput,
  CreateFixedAssetInput,
  CreateInvoiceInput,
  CreateQuoteInput,
  CreateThirdPartyInput,
  CreateWorkspaceInput,
  DepreciationRun,
  DepreciationScheduleRow,
  Entry,
  FiscalYear,
  FixedAsset,
  GenerateDepreciationResult,
  Invoice,
  Journal,
  LedgerFilter,
  LedgerMovement,
  PublicationPackMeta,
  Quote,
  ThirdParty,
  UpdateOrganizationInput,
  VatCode,
  VatReport,
  WorkspaceInfo,
} from "./types";

export const api = {
  createWorkspace: (input: CreateWorkspaceInput) =>
    invoke<WorkspaceInfo>("create_workspace", { input }),
  openWorkspace: (path: string, password: string) =>
    invoke<WorkspaceInfo>("open_workspace", { path, password }),
  createDemoWorkspace: (path: string) =>
    invoke<WorkspaceInfo>("create_demo_workspace", { path }),
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
};

export interface Organization {
  id: number;
  name: string;
  siren: string;
  siret: string | null;
  legalForm: string;
  vatRegime: string;
  address: string | null;
  capitalCents: number;
}

export interface UpdateOrganizationInput {
  name: string;
  siren: string;
  siret?: string | null;
  legalForm: string;
  vatRegime: string;
  address?: string | null;
  capitalCents: number;
  displayName: string;
  fiscalYearLabel: string;
  startDate: string;
  endDate: string;
}

export interface LocalProfile {
  id: number;
  displayName: string;
}

export interface FiscalYear {
  id: number;
  label: string;
  startDate: string;
  endDate: string;
  status: string;
}

export interface WorkspaceInfo {
  path: string;
  organization: Organization;
  fiscalYear: FiscalYear;
  profile: LocalProfile;
}

export interface CreateWorkspaceInput {
  /** Omise pour stocker sous le dossier app data (workspaces/). */
  path?: string | null;
  password: string;
  orgName: string;
  siren: string;
  fiscalYearLabel: string;
  startDate: string;
  endDate: string;
  displayName: string;
}

/** Société connue sur cette machine (registre local). */
export interface KnownWorkspace {
  id: string;
  name: string;
  siren: string;
  path: string;
  lastOpenedAt: string;
}

export interface Account {
  id: number;
  number: string;
  label: string;
  class: number;
  accountType: string;
  isAuxiliary: boolean;
  isCustom: boolean;
  active: boolean;
}

export interface Journal {
  id: number;
  code: string;
  label: string;
  journalType: string;
}

export interface EntryLineInput {
  accountNumber: string;
  label?: string | null;
  debitCents: number;
  creditCents: number;
  vatCode?: string | null;
  thirdPartyId?: number | null;
  compAuxType?: string | null;
  compAuxNum?: string | null;
}

export interface EntryLine {
  id: number;
  entryId: number;
  lineNo: number;
  accountNumber: string;
  accountLabel: string;
  label: string | null;
  debitCents: number;
  creditCents: number;
  vatCode: string | null;
  thirdPartyId: number | null;
  thirdPartyCode: string | null;
  thirdPartyName: string | null;
  compAuxType: string | null;
  compAuxNum: string | null;
}

export interface Attachment {
  id: number;
  entryId: number;
  fileName: string;
  filePath: string;
  mimeType: string | null;
}

export interface Entry {
  id: number;
  journalCode: string;
  entryDate: string;
  pieceRef: string;
  label: string;
  status: string;
  fiscalYearId: number;
  ecritureNum: string | null;
  reversalOfId: number | null;
  createdAt: string;
  postedAt: string | null;
  lines: EntryLine[];
  attachments: Attachment[];
}

export interface CreateEntryInput {
  journalCode: string;
  entryDate: string;
  pieceRef: string;
  label: string;
  lines: EntryLineInput[];
}

export interface VatCode {
  code: string;
  label: string;
  rateBp: number;
  direction: string;
  active: boolean;
}

export interface ThirdParty {
  id: number;
  code: string;
  name: string;
  kind: string;
  accountNumber: string;
  active: boolean;
  siret?: string | null;
  email?: string | null;
  address?: string | null;
  routingId?: string | null;
}

export interface CreateThirdPartyInput {
  code: string;
  name: string;
  kind: string;
  siret?: string | null;
  email?: string | null;
  address?: string | null;
  routingId?: string | null;
}

export type QuoteStatus =
  | "draft"
  | "sent"
  | "accepted"
  | "refused"
  | "expired"
  | "invoiced";

export type BillingVatMode =
  | "normale"
  | "sans"
  | "autoliquidation_chantier";

export interface QuoteLine {
  id: number;
  quoteId: number;
  lineNo: number;
  label: string;
  description?: string | null;
  quantityMilli: number;
  unit: string;
  unitPriceCents: number;
  htCents: number;
  vatRateBp: number;
  vatCode: string | null;
  accountNumber: string;
}

export interface QuoteLineInput {
  label: string;
  description?: string | null;
  quantityMilli: number;
  unit: string;
  unitPriceCents: number;
  htCents?: number;
  vatRateBp: number;
  vatCode?: string | null;
  accountNumber: string;
}

export interface Quote {
  id: number;
  number: string;
  thirdPartyId: number;
  thirdPartyCode: string;
  thirdPartyName: string;
  status: QuoteStatus | string;
  issueDate: string;
  validUntil: string | null;
  currency: string;
  htCents: number;
  vatCents: number;
  ttcCents: number;
  vatMode: BillingVatMode | string;
  notes: string | null;
  legalMentions: string | null;
  createdAt: string;
  updatedAt: string;
  lines: QuoteLine[];
}

export interface CreateQuoteInput {
  thirdPartyId: number;
  issueDate: string;
  validUntil?: string | null;
  vatMode: BillingVatMode | string;
  notes?: string | null;
  legalMentions?: string | null;
  lines: QuoteLineInput[];
}

export type InvoiceKind = "standard" | "deposit" | "balance" | "credit_note";

export type InvoiceStatus =
  | "draft"
  | "issued"
  | "sent"
  | "partially_paid"
  | "paid"
  | "cancelled";

export interface InvoiceLine {
  id: number;
  invoiceId: number;
  lineNo: number;
  label: string;
  description?: string | null;
  quantityMilli: number;
  unit: string;
  unitPriceCents: number;
  htCents: number;
  vatRateBp: number;
  vatCode: string | null;
  accountNumber: string;
  analyticCode: string | null;
}

export interface InvoiceLineInput {
  label: string;
  description?: string | null;
  quantityMilli: number;
  unit: string;
  unitPriceCents: number;
  htCents?: number;
  vatRateBp: number;
  vatCode?: string | null;
  accountNumber: string;
  analyticCode?: string | null;
}

export interface Invoice {
  id: number;
  number: string | null;
  kind: InvoiceKind | string;
  thirdPartyId: number;
  thirdPartyCode: string;
  thirdPartyName: string;
  quoteId: number | null;
  parentInvoiceId: number | null;
  status: InvoiceStatus | string;
  issueDate: string;
  dueDate: string | null;
  serviceDate: string | null;
  htCents: number;
  vatCents: number;
  ttcCents: number;
  paidCents: number;
  vatMode: string;
  vatCode: string | null;
  einvoiceProfile: string;
  einvoiceStatus: string;
  entryId: number | null;
  pdfPath: string | null;
  facturxPath: string | null;
  notes: string | null;
  legalMentions: string | null;
  createdAt: string;
  updatedAt: string;
  lines: InvoiceLine[];
}

export interface CreateInvoiceInput {
  kind: InvoiceKind | string;
  thirdPartyId: number;
  quoteId?: number | null;
  parentInvoiceId?: number | null;
  issueDate: string;
  dueDate?: string | null;
  serviceDate?: string | null;
  vatMode: BillingVatMode | string;
  notes?: string | null;
  legalMentions?: string | null;
  lines: InvoiceLineInput[];
}

export interface Payment {
  id: number;
  invoiceId: number;
  invoiceNumber: string | null;
  thirdPartyCode: string;
  thirdPartyName: string;
  paidOn: string;
  amountCents: number;
  method: string;
  bankRef: string | null;
  entryId: number | null;
  lettre: string | null;
}

export interface CreatePaymentInput {
  invoiceId: number;
  paidOn: string;
  amountCents: number;
  method: string;
  bankRef?: string | null;
}

export interface Reminder {
  id: number;
  invoiceId: number;
  invoiceNumber: string | null;
  thirdPartyCode: string;
  thirdPartyName: string;
  level: number;
  dueDate: string | null;
  daysOverdue: number;
  amountDueCents: number;
  sentAt: string | null;
  channel: string;
  bodySnapshot: string | null;
}

export type ObligationPhase =
  | "receive_only"
  | "emit_large"
  | "emit_all"
  | "franchise_receive";

export interface EinvoicePartyProfile {
  organizationId: number;
  siret: string | null;
  siren: string | null;
  vatNumber: string | null;
  legalName: string | null;
  addressStructured: string | null;
  routingScheme: string | null;
  routingId: string | null;
  obligationPhase: ObligationPhase | string;
  obligationReceiveFrom: string | null;
  obligationEmitFrom: string | null;
  pdpProvider: string | null;
  pdpApiBase: string | null;
  pdpTechId: string | null;
  certificateRef: string | null;
}

export interface UpdateEinvoiceProfileInput {
  siret?: string | null;
  siren?: string | null;
  vatNumber?: string | null;
  legalName?: string | null;
  addressStructured?: string | null;
  routingScheme?: string | null;
  routingId?: string | null;
  obligationPhase: string;
  obligationReceiveFrom?: string | null;
  obligationEmitFrom?: string | null;
  pdpProvider?: string | null;
  pdpApiBase?: string | null;
  pdpTechId?: string | null;
  certificateRef?: string | null;
}

export interface DocumentSequenceInfo {
  kind: string;
  label: string;
  fiscalYearId: number;
  fiscalYearLabel: string;
  nextNum: number;
  minNextNum: number;
  preview: string;
}

export interface SetDocumentSequenceInput {
  kind: string;
  nextNum: number;
}

export interface EinvoiceQueueItem {
  id: number;
  direction: "outbound" | "inbound" | string;
  invoiceId: number | null;
  entryId: number | null;
  payloadPath: string | null;
  format: string;
  lifecycleStatus: string;
  lastError: string | null;
  attempts: number;
  summary: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface EreportingBatch {
  id: number;
  periodFrom: string;
  periodTo: string;
  kind: string;
  status: string;
  payloadPath: string | null;
  entryCount: number;
  createdAt: string;
}

export interface CreateEreportingBatchInput {
  periodFrom: string;
  periodTo: string;
  kind: string;
  destPath: string;
}

export interface LedgerFilter {
  fromDate?: string | null;
  toDate?: string | null;
  journalCode?: string | null;
  accountNumber?: string | null;
}

export interface LedgerMovement {
  entryId: number;
  entryDate: string;
  journalCode: string;
  pieceRef: string;
  ecritureNum: string | null;
  entryLabel: string;
  accountNumber: string;
  accountLabel: string;
  lineLabel: string | null;
  debitCents: number;
  creditCents: number;
  vatCode: string | null;
  thirdPartyCode: string | null;
}

export interface BalanceRow {
  accountNumber: string;
  accountLabel: string;
  class: number;
  debitCents: number;
  creditCents: number;
  soldeDebitCents: number;
  soldeCreditCents: number;
}

export interface BalanceReport {
  rows: BalanceRow[];
  totalDebitCents: number;
  totalCreditCents: number;
  ecartCents: number;
}

export interface VatReportLine {
  code: string;
  label: string;
  direction: string;
  rateBp: number;
  baseCents: number;
  vatCents: number;
}

export interface VatReport {
  lines: VatReportLine[];
  collectedCents: number;
  deductibleCents: number;
  netCents: number;
  fromDate: string;
  toDate: string;
}

export interface AuxBalanceRow {
  accountNumber: string;
  accountLabel: string;
  auxCode: string;
  auxLabel: string;
  debitCents: number;
  creditCents: number;
  soldeDebitCents: number;
  soldeCreditCents: number;
}

export interface AuxBalanceReport {
  rows: AuxBalanceRow[];
}

export interface FixedAsset {
  id: number;
  code: string;
  label: string;
  accountNumber: string;
  amortAccount: string;
  expenseAccount: string;
  acquisitionDate: string;
  serviceDate: string;
  baseCents: number;
  residualCents: number;
  usefulLifeYears: number;
  method: string;
  active: boolean;
  notes?: string | null;
  annualDotationCents: number;
}

export interface CreateFixedAssetInput {
  code: string;
  label: string;
  accountNumber: string;
  amortAccount?: string | null;
  expenseAccount?: string | null;
  acquisitionDate: string;
  serviceDate: string;
  baseCents: number;
  residualCents: number;
  usefulLifeYears: number;
  notes?: string | null;
}

export interface DepreciationScheduleRow {
  yearIndex: number;
  periodLabel: string;
  amountCents: number;
  cumulativeCents: number;
  netBookValueCents: number;
}

export interface DepreciationRun {
  id: number;
  assetId: number;
  assetCode: string;
  assetLabel: string;
  periodLabel: string;
  periodStart: string;
  periodEnd: string;
  amountCents: number;
  entryId?: number | null;
  ecritureNum?: string | null;
  journalCode?: string | null;
  createdAt: string;
}

export interface GenerateDepreciationResult {
  periodLabel: string;
  created: DepreciationRun[];
  skipped: string[];
  totalAmountCents: number;
}

export interface StatementLine {
  code: string;
  label: string;
  amountCents: number;
  accounts: string[];
  note?: string | null;
}

export interface BalanceSheet {
  asOf: string;
  fromDate: string;
  toDate: string;
  actif: StatementLine[];
  passif: StatementLine[];
  totalActifCents: number;
  totalPassifCents: number;
  ecartCents: number;
  resultatCents: number;
}

export interface IncomeStatement {
  fromDate: string;
  toDate: string;
  charges: StatementLine[];
  produits: StatementLine[];
  totalChargesCents: number;
  totalProduitsCents: number;
  resultatCents: number;
}

export interface AnnexAssetRow {
  code: string;
  label: string;
  accountNumber: string;
  grossCents: number;
  cumulativeAmortCents: number;
  netCents: number;
  usefulLifeYears: number;
  method: string;
}

export interface AnnexDebtRow {
  accountNumber: string;
  label: string;
  amountCents: number;
  nature: string;
}

export interface AnnualAnnex {
  asOf: string;
  methods: string[];
  immobilisations: AnnexAssetRow[];
  dettes: AnnexDebtRow[];
}

export interface AnnualStatements {
  organizationName: string;
  siren: string;
  bilan: BalanceSheet;
  compteResultat: IncomeStatement;
  annexe: AnnualAnnex;
}

export interface ChecklistItem {
  code: string;
  label: string;
  ok: boolean;
  detail?: string | null;
}

export interface ClosingChecklist {
  fiscalYear: FiscalYear;
  items: ChecklistItem[];
  allOk: boolean;
  resultatCents: number;
}

export interface CloseYearResult {
  closedYear: FiscalYear;
  nextYear: FiscalYear;
  anEntryId: number;
  anEcritureNum?: string | null;
  resultatCents: number;
}

export interface PublicationPackMeta {
  organizationName: string;
  siren: string;
  fiscalYearLabel: string;
  fromDate: string;
  toDate: string;
  fyStatus: string;
  resultatCents: number;
  totalActifCents: number;
  totalPassifCents: number;
  balanceEcartCents: number;
  generatedAt: string;
  disclaimer: string;
}

export interface ClosingEventInfo {
  id: number;
  fiscalYearId: number;
  closedAt: string;
  anEntryId?: number | null;
  nextFiscalYearId?: number | null;
  packPath?: string | null;
  notes?: string | null;
}

export type NavId =
  | "guide"
  | "ma-sas"
  | "plan"
  | "journaux"
  | "ecritures"
  | "import-docs"
  | "grand-livre"
  | "tva"
  | "immos"
  | "etats"
  | "cloture"
  | "devis"
  | "factures"
  | "relances"
  | "einvoice"
  | "salaries"
  | "conges"
  | "paie";

export type ContractKind = "cdi" | "cdd" | "apprentice";
export type ContractStatus = "draft" | "active" | "ended";
export type LeaveStatus = "pending" | "approved" | "refused" | "cancelled";
export type PayslipStatus = "draft" | "validated";

export interface Employee {
  id: number;
  code: string;
  firstName: string;
  lastName: string;
  birthDate?: string | null;
  address?: string | null;
  email?: string | null;
  ibanRef?: string | null;
  active: boolean;
  createdAt: string;
  updatedAt: string;
}

export interface CreateEmployeeInput {
  code: string;
  firstName: string;
  lastName: string;
  birthDate?: string | null;
  address?: string | null;
  email?: string | null;
  ibanRef?: string | null;
  active?: boolean;
}

export interface EmploymentContract {
  id: number;
  employeeId: number;
  employeeCode: string;
  employeeName: string;
  kind: ContractKind | string;
  startDate: string;
  endDate?: string | null;
  jobTitle: string;
  weeklyHoursBp: number;
  grossMonthlyCents: number;
  conventionRef?: string | null;
  status: ContractStatus | string;
  createdAt: string;
  updatedAt: string;
}

export interface CreateContractInput {
  employeeId: number;
  kind: string;
  startDate: string;
  endDate?: string | null;
  jobTitle: string;
  weeklyHoursBp: number;
  grossMonthlyCents: number;
  conventionRef?: string | null;
  status?: string;
}

export interface LeaveBalance {
  employeeId: number;
  employeeCode: string;
  employeeName: string;
  year: number;
  acquiredDaysBp: number;
  takenDaysBp: number;
  remainingDaysBp: number;
}

export interface LeaveRequest {
  id: number;
  employeeId: number;
  employeeCode: string;
  employeeName: string;
  fromDate: string;
  toDate: string;
  daysBp: number;
  status: LeaveStatus | string;
  notes?: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface CreateLeaveRequestInput {
  employeeId: number;
  fromDate: string;
  toDate: string;
  daysBp: number;
  notes?: string | null;
}

export interface SocialRateTable {
  id: number;
  validFrom: string;
  payloadJson: string;
  label: string;
  createdAt: string;
}

export interface PayslipLine {
  id: number;
  payslipId: number;
  lineNo: number;
  code: string;
  label: string;
  baseCents: number;
  rateBp: number;
  employeeCents: number;
  employerCents: number;
}

export interface Payslip {
  id: number;
  employeeId: number;
  employeeCode: string;
  employeeName: string;
  contractId?: number | null;
  periodYm: string;
  status: PayslipStatus | string;
  grossCents: number;
  employeeChargesCents: number;
  employerChargesCents: number;
  netCents: number;
  pasRateBp?: number | null;
  pasCents: number;
  entryId?: number | null;
  pdfPath?: string | null;
  notes?: string | null;
  createdAt: string;
  updatedAt: string;
  lines: PayslipLine[];
}

export interface DsnExportRun {
  id: number;
  periodYm: string;
  status: string;
  path?: string | null;
  checklistJson: string;
  createdAt: string;
}

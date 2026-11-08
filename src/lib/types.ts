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
  path: string;
  password: string;
  orgName: string;
  siren: string;
  fiscalYearLabel: string;
  startDate: string;
  endDate: string;
  displayName: string;
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
}

export interface CreateThirdPartyInput {
  code: string;
  name: string;
  kind: string;
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

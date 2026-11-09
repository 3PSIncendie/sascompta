import {
  draftLinesToInput,
  emptyDraftLine,
  legalMentionsFor,
  summarizeQuoteLines,
  type DraftLine,
} from "./quotes";
import type {
  BillingVatMode,
  CreateInvoiceInput,
  Invoice,
  InvoiceKind,
  InvoiceStatus,
} from "../../lib/types";

export const INVOICE_KIND_LABELS: Record<InvoiceKind, string> = {
  standard: "Facture",
  deposit: "Acompte",
  balance: "Solde",
  credit_note: "Avoir",
};

export const INVOICE_STATUS_LABELS: Record<InvoiceStatus, string> = {
  draft: "Brouillon",
  issued: "Émise",
  sent: "Envoyée",
  partially_paid: "Partiellement payée",
  paid: "Payée",
  cancelled: "Annulée",
};

export function kindLabel(kind: string): string {
  return INVOICE_KIND_LABELS[kind as InvoiceKind] ?? kind;
}

export function statusLabel(status: string): string {
  return INVOICE_STATUS_LABELS[status as InvoiceStatus] ?? status;
}

export function defaultAccountForKind(kind: InvoiceKind | string): string {
  return kind === "deposit" ? "4191" : "706";
}

export function invoiceToDraftLines(inv: Invoice): DraftLine[] {
  return inv.lines.map((l) => ({
    key: `i-${l.id}`,
    label: l.label,
    description: l.description ?? "",
    quantity: (l.quantityMilli / 1000).toString(),
    unit: l.unit,
    unitPriceEuros: (l.unitPriceCents / 100).toFixed(2),
    vatRateBp: l.vatRateBp,
    vatCode: l.vatCode ?? "",
    accountNumber: l.accountNumber,
  }));
}

export function buildInvoiceCreateInput(opts: {
  kind: InvoiceKind;
  thirdPartyId: number;
  parentInvoiceId?: number | null;
  quoteId?: number | null;
  issueDate: string;
  dueDate: string;
  serviceDate: string;
  vatMode: BillingVatMode;
  notes: string;
  lines: DraftLine[];
  parseEuros: (s: string) => number;
}): CreateInvoiceInput {
  const lines = draftLinesToInput(opts.lines, opts.parseEuros).map((l) => ({
    ...l,
    accountNumber:
      l.accountNumber.trim() || defaultAccountForKind(opts.kind),
    analyticCode: null as string | null,
  }));
  return {
    kind: opts.kind,
    thirdPartyId: opts.thirdPartyId,
    quoteId: opts.quoteId ?? null,
    parentInvoiceId: opts.parentInvoiceId ?? null,
    issueDate: opts.issueDate,
    dueDate: opts.dueDate.trim() || null,
    serviceDate: opts.serviceDate.trim() || null,
    vatMode: opts.vatMode,
    notes: opts.notes.trim() || null,
    legalMentions: legalMentionsFor(opts.vatMode) || null,
    lines,
  };
}

export function summarizeInvoiceDraft(
  lines: DraftLine[],
  vatMode: BillingVatMode | string,
  parseEuros: (s: string) => number,
) {
  return summarizeQuoteLines(draftLinesToInput(lines, parseEuros), vatMode);
}

export { emptyDraftLine };

/** Aperçu navigateur (`?preview=factures`). */
export function previewInvoices(): Invoice[] {
  const now = "2026-04-01T10:00:00Z";
  return [
    {
      id: 1,
      number: null,
      kind: "standard",
      thirdPartyId: 3,
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      quoteId: null,
      parentInvoiceId: null,
      status: "draft",
      issueDate: "2026-04-01",
      dueDate: "2026-04-30",
      serviceDate: "2026-03-28",
      htCents: 120_000,
      vatCents: 24_000,
      ttcCents: 144_000,
      paidCents: 0,
      vatMode: "normale",
      vatCode: "T20C",
      einvoiceProfile: "none",
      einvoiceStatus: "local_only",
      entryId: null,
      pdfPath: null,
      facturxPath: null,
      notes: "Maintenance annuelle détection",
      legalMentions: null,
      createdAt: now,
      updatedAt: now,
      lines: [
        {
          id: 1,
          invoiceId: 1,
          lineNo: 1,
          label: "Contrat maintenance annuelle",
          description: "Visites périodiques + rapport",
          quantityMilli: 1000,
          unit: "u",
          unitPriceCents: 120_000,
          htCents: 120_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "706",
          analyticCode: null,
        },
      ],
    },
    {
      id: 2,
      number: "FACT-2026-0001",
      kind: "standard",
      thirdPartyId: 3,
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      quoteId: 1,
      parentInvoiceId: null,
      status: "issued",
      issueDate: "2026-03-15",
      dueDate: "2026-04-15",
      serviceDate: null,
      htCents: 302_000,
      vatCents: 60_400,
      ttcCents: 362_400,
      paidCents: 0,
      vatMode: "normale",
      vatCode: "T20C",
      einvoiceProfile: "none",
      einvoiceStatus: "local_only",
      entryId: 42,
      pdfPath: null,
      facturxPath: null,
      notes: "Depuis devis DEV-2026-0001",
      legalMentions: null,
      createdAt: now,
      updatedAt: now,
      lines: [
        {
          id: 2,
          invoiceId: 2,
          lineNo: 1,
          label: "Centrale détection addressable",
          description:
            "Fourniture centrale + boucles adressables\nMise en service incluse",
          quantityMilli: 1000,
          unit: "u",
          unitPriceCents: 250_000,
          htCents: 250_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "707",
          analyticCode: null,
        },
        {
          id: 3,
          invoiceId: 2,
          lineNo: 2,
          label: "Main d'œuvre pose",
          description: "Pose et câblage sur site",
          quantityMilli: 8000,
          unit: "h",
          unitPriceCents: 6_500,
          htCents: 52_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "706",
          analyticCode: null,
        },
      ],
    },
  ];
}

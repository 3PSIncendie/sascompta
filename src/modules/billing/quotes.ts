import { computeVatCents } from "../../lib/guidedEntry";
import type {
  BillingVatMode,
  CreateQuoteInput,
  Quote,
  QuoteLine,
  QuoteLineInput,
  QuoteStatus,
} from "../../lib/types";

export const QUOTE_STATUS_LABELS: Record<QuoteStatus, string> = {
  draft: "Brouillon",
  sent: "Envoyé",
  accepted: "Accepté",
  refused: "Refusé",
  expired: "Expiré",
  invoiced: "Facturé",
};

export const BILLING_VAT_MODE_OPTIONS: {
  id: BillingVatMode;
  label: string;
  help: string;
}[] = [
  {
    id: "normale",
    label: "TVA classique",
    help: "TVA collectée au taux des lignes.",
  },
  {
    id: "sans",
    label: "Sans TVA",
    help: "Franchise / exonération / hors champ.",
  },
  {
    id: "autoliquidation_chantier",
    label: "Autoliquidation chantier (art. 283)",
    help: "Travaux BTP HT — le client assujetti reverse la TVA.",
  },
];

export function legalMentionsFor(vatMode: BillingVatMode | string): string {
  switch (vatMode) {
    case "autoliquidation_chantier":
      return "TVA due par le preneur — autoliquidation (art. 283 du CGI). Montant HT facturé ; le client assujetti reverse la TVA.";
    case "sans":
      return "Opération sans TVA (franchise en base / exonération / hors champ).";
    default:
      return "";
  }
}

export function lineHtCents(
  quantityMilli: number,
  unitPriceCents: number,
): number {
  return Math.round((quantityMilli * unitPriceCents) / 1000);
}

export function quoteLineVatCents(
  vatMode: BillingVatMode | string,
  htCents: number,
  rateBp: number,
): number {
  if (vatMode !== "normale") return 0;
  return computeVatCents(htCents, rateBp);
}

export function summarizeQuoteLines(
  lines: QuoteLineInput[],
  vatMode: BillingVatMode | string,
): { htCents: number; vatCents: number; ttcCents: number } {
  let htCents = 0;
  let vatCents = 0;
  for (const line of lines) {
    const ht =
      line.htCents && line.htCents > 0
        ? line.htCents
        : lineHtCents(line.quantityMilli, line.unitPriceCents);
    htCents += ht;
    vatCents += quoteLineVatCents(vatMode, ht, line.vatRateBp);
  }
  return { htCents, vatCents, ttcCents: htCents + vatCents };
}

export type DraftLine = {
  key: string;
  label: string;
  quantity: string;
  unit: string;
  unitPriceEuros: string;
  vatRateBp: number;
  vatCode: string;
  accountNumber: string;
};

export function emptyDraftLine(rateBp = 2000, vatCode = "T20C"): DraftLine {
  return {
    key: `l-${Math.random().toString(36).slice(2, 9)}`,
    label: "",
    quantity: "1",
    unit: "u",
    unitPriceEuros: "",
    vatRateBp: rateBp,
    vatCode,
    accountNumber: "706",
  };
}

export function draftLinesToInput(
  lines: DraftLine[],
  parseEuros: (s: string) => number,
): QuoteLineInput[] {
  return lines
    .filter((l) => l.label.trim())
    .map((l) => {
      const qty = Number(String(l.quantity).replace(",", ".")) || 0;
      const quantityMilli = Math.max(1, Math.round(qty * 1000));
      const unitPriceCents = parseEuros(l.unitPriceEuros);
      return {
        label: l.label.trim(),
        quantityMilli,
        unit: l.unit.trim() || "u",
        unitPriceCents,
        htCents: lineHtCents(quantityMilli, unitPriceCents),
        vatRateBp: l.vatRateBp,
        vatCode: l.vatCode || null,
        accountNumber: l.accountNumber.trim() || "706",
      };
    });
}

export function quoteToDraftLines(quote: Quote): DraftLine[] {
  return quote.lines.map((l: QuoteLine) => ({
    key: `q-${l.id}`,
    label: l.label,
    quantity: (l.quantityMilli / 1000).toString(),
    unit: l.unit,
    unitPriceEuros: (l.unitPriceCents / 100).toFixed(2),
    vatRateBp: l.vatRateBp,
    vatCode: l.vatCode ?? "",
    accountNumber: l.accountNumber,
  }));
}

export function buildCreateInput(opts: {
  thirdPartyId: number;
  issueDate: string;
  validUntil: string;
  vatMode: BillingVatMode;
  notes: string;
  lines: DraftLine[];
  parseEuros: (s: string) => number;
}): CreateQuoteInput {
  const lines = draftLinesToInput(opts.lines, opts.parseEuros);
  return {
    thirdPartyId: opts.thirdPartyId,
    issueDate: opts.issueDate,
    validUntil: opts.validUntil.trim() || null,
    vatMode: opts.vatMode,
    notes: opts.notes.trim() || null,
    legalMentions: legalMentionsFor(opts.vatMode) || null,
    lines,
  };
}

/** Données d’aperçu navigateur (`?preview`). */
export function previewQuotes(): Quote[] {
  const now = "2026-03-01T10:00:00Z";
  return [
    {
      id: 1,
      number: "DEV-2026-0001",
      thirdPartyId: 3,
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      status: "sent",
      issueDate: "2026-03-01",
      validUntil: "2026-03-31",
      currency: "EUR",
      htCents: 302_000,
      vatCents: 60_400,
      ttcCents: 362_400,
      vatMode: "normale",
      notes: "Installation détection incendie — atelier",
      legalMentions: null,
      createdAt: now,
      updatedAt: now,
      lines: [
        {
          id: 1,
          quoteId: 1,
          lineNo: 1,
          label: "Centrale détection addressable",
          quantityMilli: 1000,
          unit: "u",
          unitPriceCents: 250_000,
          htCents: 250_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "707",
        },
        {
          id: 2,
          quoteId: 1,
          lineNo: 2,
          label: "Main d'œuvre pose",
          quantityMilli: 8000,
          unit: "h",
          unitPriceCents: 6_500,
          htCents: 52_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "706",
        },
      ],
    },
  ];
}

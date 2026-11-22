import type {
  BillingVatMode,
  CreateCustomerOrderInput,
  CustomerOrder,
  CustomerOrderStatus,
} from "../../lib/types";
import {
  BILLING_VAT_MODE_OPTIONS,
  draftLinesToInput,
  emptyDraftLine,
  emptyDraftLineForVatMode,
  legalMentionsFor,
  summarizeQuoteLines,
  syncDraftLinesToVatMode,
  vatCodesForBillingMode,
  type DraftLine,
} from "./quotes";

export {
  BILLING_VAT_MODE_OPTIONS,
  emptyDraftLine,
  emptyDraftLineForVatMode,
  legalMentionsFor,
  summarizeQuoteLines,
  syncDraftLinesToVatMode,
  vatCodesForBillingMode,
  type DraftLine,
};

export const ORDER_STATUS_LABELS: Record<CustomerOrderStatus, string> = {
  draft: "Brouillon",
  sent: "Envoyée",
  confirmed: "Confirmée",
  cancelled: "Annulée",
};

export function orderSourceHint(source?: string | null): string | null {
  return source === "import" ? "Import" : null;
}

export function isImportedOrder(o: Pick<CustomerOrder, "source">): boolean {
  return o.source === "import";
}

export function orderToDraftLines(order: CustomerOrder): DraftLine[] {
  return order.lines.map((l) => ({
    key: `o-${l.id}`,
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

export function buildCreateOrderInput(opts: {
  thirdPartyId: number;
  issueDate: string;
  vatMode: BillingVatMode;
  notes: string;
  lines: DraftLine[];
  parseEuros: (s: string) => number;
}): CreateCustomerOrderInput {
  const lines = draftLinesToInput(opts.lines, opts.parseEuros);
  return {
    thirdPartyId: opts.thirdPartyId,
    issueDate: opts.issueDate,
    vatMode: opts.vatMode,
    notes: opts.notes.trim() || null,
    legalMentions: legalMentionsFor(opts.vatMode) || null,
    lines,
  };
}

/** Données d’aperçu navigateur (`?preview=commandes`). */
export function previewOrders(): CustomerOrder[] {
  const now = "2026-03-15T10:00:00Z";
  return [
    {
      id: 1,
      number: "C2026-0301",
      thirdPartyId: 3,
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      status: "confirmed",
      issueDate: "2026-03-15",
      currency: "EUR",
      htCents: 120_000,
      vatCents: 24_000,
      ttcCents: 144_000,
      vatMode: "normale",
      notes: "Bon de commande détection incendie",
      legalMentions: null,
      createdAt: now,
      updatedAt: now,
      source: "native",
      lines: [
        {
          id: 1,
          orderId: 1,
          lineNo: 1,
          label: "Centrale détection addressable",
          description: "Selon devis accepté",
          quantityMilli: 1000,
          unit: "u",
          unitPriceCents: 120_000,
          htCents: 120_000,
          vatRateBp: 2000,
          vatCode: "T20C",
          accountNumber: "706",
        },
      ],
    },
  ];
}

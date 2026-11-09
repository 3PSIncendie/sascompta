import type { Payment, Reminder } from "../../lib/types";

export const PAYMENT_METHODS = [
  { id: "virement", label: "Virement" },
  { id: "cheque", label: "Chèque" },
  { id: "cb", label: "Carte bancaire" },
  { id: "especes", label: "Espèces" },
  { id: "autre", label: "Autre" },
] as const;

export function methodLabel(method: string): string {
  return PAYMENT_METHODS.find((m) => m.id === method)?.label ?? method;
}

export function reminderLevelLabel(level: number): string {
  return `J+${level}`;
}

/** Données d’aperçu navigateur. */
export function previewPayments(): Payment[] {
  return [
    {
      id: 1,
      invoiceId: 2,
      invoiceNumber: "FACT-2026-0001",
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      paidOn: "2026-04-10",
      amountCents: 180_000,
      method: "virement",
      bankRef: "VIR-GAMMA-10",
      entryId: 55,
      lettre: "FACT20260001",
    },
  ];
}

export function previewReminders(): Reminder[] {
  return [
    {
      id: 1,
      invoiceId: 2,
      invoiceNumber: "FACT-2026-0001",
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      level: 7,
      dueDate: "2026-03-01",
      daysOverdue: 40,
      amountDueCents: 182_400,
      sentAt: null,
      channel: "pdf",
      bodySnapshot:
        "Relance J+7 — Facture FACT-2026-0001\nClient : Client Gamma (C001)\nReste dû : 1824,00 €",
    },
    {
      id: 2,
      invoiceId: 2,
      invoiceNumber: "FACT-2026-0001",
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      level: 15,
      dueDate: "2026-03-01",
      daysOverdue: 40,
      amountDueCents: 182_400,
      sentAt: "2026-03-20T10:00:00Z",
      channel: "pdf",
      bodySnapshot: "Relance J+15 envoyée.",
    },
  ];
}

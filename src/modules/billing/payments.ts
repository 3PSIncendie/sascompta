import type { Payment, Reminder } from "../../lib/types";

export const PAYMENT_METHODS = [
  { id: "virement", label: "Virement" },
  { id: "cheque", label: "Chèque" },
  { id: "cb", label: "Carte bancaire" },
  { id: "especes", label: "Espèces" },
  { id: "autre", label: "Autre" },
] as const;

const METHOD_LABELS: Record<string, string> = {
  virement: "Virement",
  cheque: "Chèque",
  cb: "Carte bancaire",
  especes: "Espèces",
  autre: "Autre",
  affacturage: "Affacturage",
};

/** Compte de charge par défaut pour la commission factor (PCG). */
export const DEFAULT_FACTORING_FEE_ACCOUNT = "622";

export function methodLabel(method: string): string {
  return METHOD_LABELS[method] ?? method;
}

/** Net reçu banque = montant cédé − commission (centimes). */
export function factoringNetCents(amountCents: number, feeCents: number): number {
  return Math.max(0, amountCents - feeCents);
}

export function isFactoringMethod(method: string): boolean {
  return method === "affacturage";
}

/** Réf. banque / n° de virement — toujours optionnelle (vide → null). */
export function normalizeBankRef(
  raw: string | null | undefined,
): string | null {
  const t = (raw ?? "").trim();
  return t.length > 0 ? t : null;
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
      invoiceNumber: "2026-03-0001",
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      paidOn: "2026-04-10",
      amountCents: 180_000,
      method: "virement",
      bankRef: "VIR-GAMMA-10",
      entryId: 55,
      lettre: "FACT20260001",
      feeCents: 0,
      feeAccount: null,
      cessionEntryId: null,
    },
  ];
}

export function previewReminders(): Reminder[] {
  return [
    {
      id: 1,
      invoiceId: 2,
      invoiceNumber: "2026-03-0001",
      thirdPartyCode: "C001",
      thirdPartyName: "Client Gamma",
      level: 7,
      dueDate: "2026-03-01",
      daysOverdue: 40,
      amountDueCents: 182_400,
      sentAt: null,
      channel: "pdf",
      bodySnapshot:
        "Relance J+7 — Facture 2026-03-0001\nClient : Client Gamma (C001)\nReste dû : 1824,00 €",
    },
    {
      id: 2,
      invoiceId: 2,
      invoiceNumber: "2026-03-0001",
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

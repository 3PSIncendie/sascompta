/** Mode de la note « conditions de facturation » sur une facture. */
export type InvoiceNotesMode = "permanent" | "once";

export const INVOICE_NOTES_MODE_OPTIONS: {
  id: InvoiceNotesMode;
  label: string;
  hint: string;
}[] = [
  {
    id: "permanent",
    label: "Permanente",
    hint: "Enregistrée dans la société et réutilisée sur les prochaines factures.",
  },
  {
    id: "once",
    label: "Une seule fois",
    hint: "Appliquée uniquement à cette facture, sans modifier le modèle permanent.",
  },
];

/** Préremplissage / comparaison avec la note société. */
export function permanentInvoiceNotes(
  org: { invoiceBillingNotes?: string | null } | null | undefined,
): string {
  return (org?.invoiceBillingNotes ?? "").trim();
}

/**
 * Mode initial à l’ouverture d’un brouillon :
 * permanente si la note est vide ou égale au modèle société, sinon une seule fois.
 */
export function initialInvoiceNotesMode(
  invoiceNotes: string | null | undefined,
  permanentNotes: string | null | undefined,
): InvoiceNotesMode {
  const inv = (invoiceNotes ?? "").trim();
  const perm = (permanentNotes ?? "").trim();
  if (!inv || inv === perm) return "permanent";
  return "once";
}

import type { DocumentSequenceInfo } from "../../lib/types";

/** Mois courant (1–12) pour l’aperçu AAAA-MM-NNNN. */
export function currentPreviewMonth(now = new Date()): string {
  return String(now.getMonth() + 1).padStart(2, "0");
}

/** Aperçu navigateur (`?preview=`). */
export function previewDocumentSequences(
  fiscalYearLabel = "2026",
  previewMonth = currentPreviewMonth(),
): DocumentSequenceInfo[] {
  const kinds: Array<{ kind: string; label: string; next: number }> = [
    {
      kind: "FACT",
      label: "Factures (standard, acompte, solde, avoir)",
      next: 1,
    },
    { kind: "DEV", label: "Devis", next: 1 },
  ];
  return kinds.map((k) => ({
    kind: k.kind,
    label: k.label,
    fiscalYearId: 1,
    fiscalYearLabel,
    nextNum: k.next,
    minNextNum: 1,
    preview: previewNumber(fiscalYearLabel, previewMonth, k.next),
  }));
}

/** Format d’émission : `AAAA-MM-NNNN` (séquence annuelle depuis le 01/01). */
export function previewNumber(
  year: string,
  month: string | number,
  nextNum: number,
): string {
  const n = Number.isFinite(nextNum) && nextNum >= 1 ? Math.floor(nextNum) : 1;
  const m = String(month).replace(/\D/g, "").padStart(2, "0").slice(-2) || "01";
  const y = String(year).trim() || "2026";
  return `${y}-${m}-${String(n).padStart(4, "0")}`;
}

export function parseNextNumInput(raw: string): number | null {
  const t = raw.trim().replace(/\s/g, "");
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  if (!Number.isSafeInteger(n) || n < 1) return null;
  return n;
}

import type { DocumentSequenceInfo } from "../../lib/types";

/** Aperçu navigateur (`?preview=`). */
export function previewDocumentSequences(
  fiscalYearLabel = "2026",
): DocumentSequenceInfo[] {
  const kinds: Array<{ kind: string; label: string; next: number }> = [
    { kind: "FACT", label: "Factures (standard / solde)", next: 1 },
    { kind: "ACMP", label: "Factures d’acompte", next: 1 },
    { kind: "AVO", label: "Avoirs", next: 1 },
    { kind: "DEV", label: "Devis", next: 1 },
  ];
  return kinds.map((k) => ({
    kind: k.kind,
    label: k.label,
    fiscalYearId: 1,
    fiscalYearLabel,
    nextNum: k.next,
    minNextNum: 1,
    preview: `${k.kind}-${fiscalYearLabel}-${String(k.next).padStart(4, "0")}`,
  }));
}

export function previewNumber(
  kind: string,
  fiscalYearLabel: string,
  nextNum: number,
): string {
  const n = Number.isFinite(nextNum) && nextNum >= 1 ? Math.floor(nextNum) : 1;
  return `${kind}-${fiscalYearLabel}-${String(n).padStart(4, "0")}`;
}

export function parseNextNumInput(raw: string): number | null {
  const t = raw.trim().replace(/\s/g, "");
  if (!/^\d+$/.test(t)) return null;
  const n = Number(t);
  if (!Number.isSafeInteger(n) || n < 1) return null;
  return n;
}

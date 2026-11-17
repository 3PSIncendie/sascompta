import type { LedgerMovement } from "./types";

/** Livre journal order: journal → date → entry → account. */
export function sortLivreJournal(rows: LedgerMovement[]): LedgerMovement[] {
  return [...rows].sort((a, b) => {
    const j = a.journalCode.localeCompare(b.journalCode);
    if (j !== 0) return j;
    const d = a.entryDate.localeCompare(b.entryDate);
    if (d !== 0) return d;
    if (a.entryId !== b.entryId) return a.entryId - b.entryId;
    return a.accountNumber.localeCompare(b.accountNumber);
  });
}

import { describe, expect, it } from "vitest";
import { sortLivreJournal } from "./livreJournal";
import type { LedgerMovement } from "./types";

function row(
  partial: Pick<
    LedgerMovement,
    "entryId" | "entryDate" | "journalCode" | "accountNumber"
  >,
): LedgerMovement {
  return {
    pieceRef: "",
    ecritureNum: null,
    entryLabel: "",
    accountLabel: "",
    lineLabel: null,
    debitCents: 0,
    creditCents: 0,
    vatCode: null,
    thirdPartyCode: null,
    ...partial,
  };
}

describe("sortLivreJournal", () => {
  it("orders by journal then date then entry then account", () => {
    const sorted = sortLivreJournal([
      row({
        entryId: 2,
        entryDate: "2026-02-01",
        journalCode: "VTE",
        accountNumber: "411",
      }),
      row({
        entryId: 1,
        entryDate: "2026-03-01",
        journalCode: "ACH",
        accountNumber: "401",
      }),
      row({
        entryId: 3,
        entryDate: "2026-01-15",
        journalCode: "ACH",
        accountNumber: "606",
      }),
      row({
        entryId: 3,
        entryDate: "2026-01-15",
        journalCode: "ACH",
        accountNumber: "401",
      }),
    ]);
    expect(
      sorted.map((r) => `${r.journalCode}:${r.entryDate}:${r.accountNumber}`),
    ).toEqual([
      "ACH:2026-01-15:401",
      "ACH:2026-01-15:606",
      "ACH:2026-03-01:401",
      "VTE:2026-02-01:411",
    ]);
  });
});

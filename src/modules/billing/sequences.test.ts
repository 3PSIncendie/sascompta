import { describe, expect, it } from "vitest";
import {
  currentPreviewMonth,
  parseNextNumInput,
  previewDocumentSequences,
  previewNumber,
} from "./sequences";

describe("document sequences helpers", () => {
  it("formate l’aperçu AAAA-MM-NNNN", () => {
    expect(previewNumber("2026", "11", 42)).toBe("2026-11-0042");
    expect(previewNumber("2026", 3, 1)).toBe("2026-03-0001");
    expect(previewNumber("2027", "01", 1)).toBe("2027-01-0001");
  });

  it("parse le prochain numéro saisi", () => {
    expect(parseNextNumInput("42")).toBe(42);
    expect(parseNextNumInput(" 7 ")).toBe(7);
    expect(parseNextNumInput("0")).toBeNull();
    expect(parseNextNumInput("abc")).toBeNull();
  });

  it("fournit un aperçu navigateur des 2 séries (factures + devis)", () => {
    const rows = previewDocumentSequences("2026", "11");
    expect(rows.map((r) => r.kind)).toEqual(["FACT", "DEV"]);
    expect(rows[0].preview).toBe("2026-11-0001");
    expect(rows[1].preview).toBe("2026-11-0001");
  });

  it("expose le mois courant pour l’aperçu", () => {
    expect(currentPreviewMonth(new Date("2026-11-15T12:00:00Z"))).toBe("11");
  });
});

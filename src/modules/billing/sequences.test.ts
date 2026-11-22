import { describe, expect, it } from "vitest";
import {
  currentPreviewMonth,
  documentNumberLetter,
  parseNextNumInput,
  previewDocumentSequences,
  previewNumber,
} from "./sequences";

describe("document sequences helpers", () => {
  it("formate l’aperçu FAAAA-MMNN / DAAAA-MMNN / CAAAA-MMNN", () => {
    expect(previewNumber("FACT", "2026", "11", 1)).toBe("F2026-1101");
    expect(previewNumber("FACT", "2026", "11", 42)).toBe("F2026-1142");
    expect(previewNumber("DEV", "2026", 3, 1)).toBe("D2026-0301");
    expect(previewNumber("DEV", "2027", "01", 1)).toBe("D2027-0101");
    expect(previewNumber("CMD", "2026", "11", 1)).toBe("C2026-1101");
  });

  it("expose la lettre de série", () => {
    expect(documentNumberLetter("FACT")).toBe("F");
    expect(documentNumberLetter("DEV")).toBe("D");
    expect(documentNumberLetter("CMD")).toBe("C");
  });

  it("parse le prochain numéro saisi", () => {
    expect(parseNextNumInput("42")).toBe(42);
    expect(parseNextNumInput(" 7 ")).toBe(7);
    expect(parseNextNumInput("0")).toBeNull();
    expect(parseNextNumInput("abc")).toBeNull();
  });

  it("fournit un aperçu navigateur des 3 séries (factures + devis + commandes)", () => {
    const rows = previewDocumentSequences("2026", "11");
    expect(rows.map((r) => r.kind)).toEqual(["FACT", "DEV", "CMD"]);
    expect(rows[0].preview).toBe("F2026-1101");
    expect(rows[1].preview).toBe("D2026-1101");
    expect(rows[2].preview).toBe("C2026-1101");
  });

  it("expose le mois courant pour l’aperçu", () => {
    expect(currentPreviewMonth(new Date("2026-11-15T12:00:00Z"))).toBe("11");
  });
});

import { describe, expect, it } from "vitest";
import {
  parseNextNumInput,
  previewDocumentSequences,
  previewNumber,
} from "./sequences";

describe("document sequences helpers", () => {
  it("formate l’aperçu FACT-AAAA-NNNN", () => {
    expect(previewNumber("FACT", "2026", 42)).toBe("FACT-2026-0042");
    expect(previewNumber("ACMP", "2026", 1)).toBe("ACMP-2026-0001");
  });

  it("parse le prochain numéro saisi", () => {
    expect(parseNextNumInput("42")).toBe(42);
    expect(parseNextNumInput(" 7 ")).toBe(7);
    expect(parseNextNumInput("0")).toBeNull();
    expect(parseNextNumInput("abc")).toBeNull();
  });

  it("fournit un aperçu navigateur des 4 séries", () => {
    const rows = previewDocumentSequences("2026");
    expect(rows.map((r) => r.kind)).toEqual(["FACT", "ACMP", "AVO", "DEV"]);
    expect(rows[0].preview).toBe("FACT-2026-0001");
  });
});

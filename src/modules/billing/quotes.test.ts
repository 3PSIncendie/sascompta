import { describe, expect, it } from "vitest";
import {
  buildCreateInput,
  emptyDraftLine,
  legalMentionsFor,
  lineHtCents,
  quoteLineVatCents,
  summarizeQuoteLines,
} from "./quotes";

describe("billing quotes helpers", () => {
  it("calcule HT ligne en milli-quantités", () => {
    expect(lineHtCents(1000, 2500)).toBe(2500);
    expect(lineHtCents(2500, 1000)).toBe(2500);
    expect(lineHtCents(8000, 6500)).toBe(52_000);
  });

  it("applique la TVA seulement en mode normale", () => {
    expect(quoteLineVatCents("normale", 10_000, 2000)).toBe(2000);
    expect(quoteLineVatCents("sans", 10_000, 2000)).toBe(0);
    expect(quoteLineVatCents("autoliquidation_chantier", 10_000, 2000)).toBe(0);
  });

  it("agrège HT / TVA / TTC", () => {
    const sum = summarizeQuoteLines(
      [
        {
          label: "A",
          quantityMilli: 1000,
          unit: "u",
          unitPriceCents: 10_000,
          vatRateBp: 2000,
          accountNumber: "706",
        },
        {
          label: "B",
          quantityMilli: 2000,
          unit: "u",
          unitPriceCents: 5_000,
          vatRateBp: 2000,
          accountNumber: "706",
        },
      ],
      "normale",
    );
    expect(sum.htCents).toBe(20_000);
    expect(sum.vatCents).toBe(4_000);
    expect(sum.ttcCents).toBe(24_000);
  });

  it("génère les mentions BTP", () => {
    expect(legalMentionsFor("autoliquidation_chantier")).toMatch(/283/);
    expect(legalMentionsFor("normale")).toBe("");
  });

  it("buildCreateInput mappe les lignes brouillon", () => {
    const line = emptyDraftLine(2000, "T20C");
    line.label = "Prestation";
    line.description = "Détail A\nDétail B";
    line.quantity = "2";
    line.unitPriceEuros = "100,00";
    const input = buildCreateInput({
      thirdPartyId: 3,
      issueDate: "2026-03-01",
      validUntil: "",
      vatMode: "normale",
      notes: "note",
      lines: [line],
      parseEuros: (s) => Math.round(Number(s.replace(",", ".")) * 100),
    });
    expect(input.lines).toHaveLength(1);
    expect(input.lines[0].quantityMilli).toBe(2000);
    expect(input.lines[0].unitPriceCents).toBe(10_000);
    expect(input.lines[0].htCents).toBe(20_000);
    expect(input.lines[0].description).toBe("Détail A\nDétail B");
    expect(input.notes).toBe("note");
  });
});

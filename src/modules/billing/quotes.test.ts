import { describe, expect, it } from "vitest";
import type { VatCode } from "../../lib/types";
import {
  buildCreateInput,
  defaultBillingVatModeForOrg,
  defaultVatCodeForBillingMode,
  emptyDraftLine,
  emptyDraftLineForVatMode,
  isImportedQuote,
  legalMentionsFor,
  lineHtCents,
  quoteLineVatCents,
  quoteSourceHint,
  summarizeQuoteLines,
  syncDraftLinesToVatMode,
  vatCodesForBillingMode,
} from "./quotes";

const CODES: VatCode[] = [
  {
    code: "T20C",
    label: "TVA 20% collectée",
    rateBp: 2000,
    direction: "collected",
    active: true,
  },
  {
    code: "T10C",
    label: "TVA 10% collectée",
    rateBp: 1000,
    direction: "collected",
    active: true,
  },
  {
    code: "T20R",
    label: "Autoliquidation 20%",
    rateBp: 2000,
    direction: "reverse",
    active: true,
  },
  {
    code: "T10R",
    label: "Autoliquidation 10%",
    rateBp: 1000,
    direction: "reverse",
    active: true,
  },
  {
    code: "TEX",
    label: "Exonéré",
    rateBp: 0,
    direction: "exempt",
    active: true,
  },
];

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

  it("signale les devis issus de l’import", () => {
    expect(quoteSourceHint("import")).toBe("Import");
    expect(quoteSourceHint("native")).toBeNull();
    expect(isImportedQuote({ source: "import" })).toBe(true);
    expect(isImportedQuote({ source: "native" })).toBe(false);
  });

  it("filtre les codes selon le régime document", () => {
    expect(vatCodesForBillingMode("normale", CODES).map((c) => c.code)).toEqual(
      ["T20C", "T10C"],
    );
    expect(vatCodesForBillingMode("sans", CODES).map((c) => c.code)).toEqual([
      "TEX",
    ]);
    expect(
      vatCodesForBillingMode("autoliquidation_chantier", CODES).map((c) => c.code),
    ).toEqual(["T20R", "T10R"]);
  });

  it("choisit le code défaut (20 %) du régime", () => {
    expect(defaultVatCodeForBillingMode("normale", CODES)?.code).toBe("T20C");
    expect(defaultVatCodeForBillingMode("sans", CODES)?.code).toBe("TEX");
    expect(
      defaultVatCodeForBillingMode("autoliquidation_chantier", CODES)?.code,
    ).toBe("T20R");
  });

  it("nouvelle ligne hérite du régime", () => {
    const line = emptyDraftLineForVatMode("sans", CODES);
    expect(line.vatCode).toBe("TEX");
    expect(line.vatRateBp).toBe(0);
  });

  it("sync : remap hors régime, conserve override compatible", () => {
    const t20 = emptyDraftLine(2000, "T20C");
    const t10 = emptyDraftLine(1000, "T10C");
    const syncedSans = syncDraftLinesToVatMode([t20, t10], "sans", CODES);
    expect(syncedSans.map((l) => l.vatCode)).toEqual(["TEX", "TEX"]);

    const syncedAuto = syncDraftLinesToVatMode([t20, t10], "autoliquidation_chantier", CODES);
    expect(syncedAuto.map((l) => [l.vatCode, l.vatRateBp])).toEqual([
      ["T20R", 2000],
      ["T10R", 1000],
    ]);

    const kept = syncDraftLinesToVatMode(syncedAuto, "autoliquidation_chantier", CODES);
    expect(kept.map((l) => l.vatCode)).toEqual(["T20R", "T10R"]);

    const back = syncDraftLinesToVatMode(syncedAuto, "normale", CODES);
    expect(back.map((l) => [l.vatCode, l.vatRateBp])).toEqual([
      ["T20C", 2000],
      ["T10C", 1000],
    ]);
  });

  it("totaux suivent le régime après sync", () => {
    const lines = syncDraftLinesToVatMode(
      [
        (() => {
          const l = emptyDraftLine(2000, "T20C");
          l.label = "A";
          l.unitPriceEuros = "100,00";
          return l;
        })(),
      ],
      "sans",
      CODES,
    );
    const input = lines.map((l) => ({
      label: l.label,
      quantityMilli: 1000,
      unit: "u",
      unitPriceCents: 10_000,
      vatRateBp: l.vatRateBp,
      vatCode: l.vatCode,
      accountNumber: "706",
    }));
    expect(summarizeQuoteLines(input, "sans")).toEqual({
      htCents: 10_000,
      vatCents: 0,
      ttcCents: 10_000,
    });
    expect(summarizeQuoteLines(input, "normale").vatCents).toBe(0);
  });

  it("franchise société → régime document sans TVA", () => {
    expect(defaultBillingVatModeForOrg("franchise")).toBe("sans");
    expect(defaultBillingVatModeForOrg("reel_mensuel")).toBe("normale");
  });
});

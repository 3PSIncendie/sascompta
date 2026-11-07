import { describe, expect, it } from "vitest";
import {
  buildGuidedLines,
  computeVatCents,
  DEFAULT_COUNTERPART,
  defaultVatCodeFor,
  eurosStringsFromLines,
  preferredHtAccount,
  SUBCONTRACT_HT_ACCOUNT,
  VAT_ACCOUNT,
  VAT_DUE_ACCOUNT,
  vatCodesForKind,
  vatModesForKind,
} from "./guidedEntry";
import type { VatCode } from "./types";

const CODES: VatCode[] = [
  {
    code: "T20C",
    label: "TVA 20% collectée",
    rateBp: 2000,
    direction: "collected",
    active: true,
  },
  {
    code: "T20D",
    label: "TVA 20% déductible",
    rateBp: 2000,
    direction: "deductible",
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
    code: "T55D",
    label: "TVA 5,5% déductible",
    rateBp: 550,
    direction: "deductible",
    active: true,
  },
  {
    code: "T20R",
    label: "Autoliquidation 20% (art. 283)",
    rateBp: 2000,
    direction: "reverse",
    active: true,
  },
  {
    code: "T10R",
    label: "Autoliquidation 10% (art. 283)",
    rateBp: 1000,
    direction: "reverse",
    active: true,
  },
  {
    code: "TEX",
    label: "Exonéré / hors champ",
    rateBp: 0,
    direction: "exempt",
    active: true,
  },
];

describe("computeVatCents", () => {
  it("calcule 20 % sur 100 €", () => {
    expect(computeVatCents(10_000, 2000)).toBe(2000);
  });

  it("arrondit au centime", () => {
    // 33,33 € HT × 20 % = 6,666 → 6,67 €
    expect(computeVatCents(3333, 2000)).toBe(667);
  });

  it("retourne 0 si HT ou taux nul", () => {
    expect(computeVatCents(0, 2000)).toBe(0);
    expect(computeVatCents(10_000, 0)).toBe(0);
  });
});

describe("buildGuidedLines — vente classique", () => {
  it("génère HT + TVA 44571 + client TTC", () => {
    const r = buildGuidedLines({
      kind: "vente",
      htAccount: "706",
      htCents: 10_000,
      vatMode: "normale",
      vatCode: "T20C",
      rateBp: 2000,
      counterpartAccount: DEFAULT_COUNTERPART.vente,
    });
    expect(r.vatCents).toBe(2000);
    expect(r.ttcCents).toBe(12_000);
    expect(r.vatAccount).toBe("44571");
    expect(r.vatDueAccount).toBeNull();
    expect(r.lines).toHaveLength(3);
    expect(r.lines[0]).toMatchObject({
      accountNumber: "706",
      creditCents: 10_000,
      debitCents: 0,
      vatCode: "T20C",
    });
    expect(r.lines[1]).toMatchObject({
      accountNumber: "44571",
      creditCents: 2000,
      debitCents: 0,
      vatCode: "T20C",
    });
    expect(r.lines[2]).toMatchObject({
      accountNumber: "411",
      debitCents: 12_000,
      creditCents: 0,
      vatCode: null,
    });
    const d = r.lines.reduce((s, l) => s + l.debitCents, 0);
    const c = r.lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
  });

  it("sans TVA : 2 lignes équilibrées", () => {
    const r = buildGuidedLines({
      kind: "vente",
      htAccount: "706",
      htCents: 10_000,
      vatMode: "sans",
      vatCode: null,
      rateBp: 0,
      counterpartAccount: "411",
    });
    expect(r.lines).toHaveLength(2);
    expect(r.vatAccount).toBeNull();
    expect(r.lines.every((l) => l.vatCode == null)).toBe(true);
    expect(r.lines[0].creditCents).toBe(10_000);
    expect(r.lines[1].debitCents).toBe(10_000);
  });
});

describe("buildGuidedLines — achat classique", () => {
  it("génère HT + TVA 44566 + fournisseur TTC", () => {
    const r = buildGuidedLines({
      kind: "achat",
      htAccount: "606",
      htCents: 50_000,
      vatMode: "normale",
      vatCode: "T20D",
      rateBp: 2000,
      counterpartAccount: DEFAULT_COUNTERPART.achat,
      thirdPartyId: 3,
    });
    expect(r.vatAccount).toBe(VAT_ACCOUNT.achat);
    expect(r.lines).toHaveLength(3);
    expect(r.lines[0]).toMatchObject({
      accountNumber: "606",
      debitCents: 50_000,
      vatCode: "T20D",
    });
    expect(r.lines[1]).toMatchObject({
      accountNumber: "44566",
      debitCents: 10_000,
      vatCode: "T20D",
    });
    expect(r.lines[2]).toMatchObject({
      accountNumber: "401",
      creditCents: 60_000,
      thirdPartyId: 3,
    });
    const d = r.lines.reduce((s, l) => s + l.debitCents, 0);
    const c = r.lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
  });
});

describe("buildGuidedLines — autoliquidation sous-traitance", () => {
  it("génère 604 + 44566 + 4452 + 401 au HT", () => {
    const r = buildGuidedLines({
      kind: "achat",
      htAccount: SUBCONTRACT_HT_ACCOUNT,
      htCents: 10_000,
      vatMode: "autoliquidation_sous_traitance",
      vatCode: "T20R",
      rateBp: 2000,
      counterpartAccount: "401",
      thirdPartyId: 1,
    });
    expect(r.vatCents).toBe(2000);
    expect(r.ttcCents).toBe(10_000);
    expect(r.vatAccount).toBe("44566");
    expect(r.vatDueAccount).toBe(VAT_DUE_ACCOUNT);
    expect(r.lines).toHaveLength(4);
    expect(r.lines[0]).toMatchObject({
      accountNumber: "604",
      debitCents: 10_000,
      vatCode: "T20R",
    });
    expect(r.lines[1]).toMatchObject({
      accountNumber: "44566",
      debitCents: 2000,
      vatCode: "T20R",
    });
    expect(r.lines[2]).toMatchObject({
      accountNumber: "4452",
      creditCents: 2000,
      vatCode: "T20R",
    });
    expect(r.lines[3]).toMatchObject({
      accountNumber: "401",
      creditCents: 10_000,
      thirdPartyId: 1,
      vatCode: null,
    });
    const d = r.lines.reduce((s, l) => s + l.debitCents, 0);
    const c = r.lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
  });
});

describe("buildGuidedLines — autoliquidation chantier en direct", () => {
  it("facture HT sans 44571 (client autoliquide)", () => {
    const r = buildGuidedLines({
      kind: "vente",
      htAccount: "706",
      htCents: 25_000,
      vatMode: "autoliquidation_chantier",
      vatCode: "T20R",
      rateBp: 2000,
      counterpartAccount: "411",
      thirdPartyId: 9,
    });
    expect(r.vatCents).toBe(0);
    expect(r.ttcCents).toBe(25_000);
    expect(r.vatAccount).toBeNull();
    expect(r.vatDueAccount).toBeNull();
    expect(r.lines).toHaveLength(2);
    expect(r.lines[0]).toMatchObject({
      accountNumber: "706",
      creditCents: 25_000,
      vatCode: "T20R",
    });
    expect(r.lines[1]).toMatchObject({
      accountNumber: "411",
      debitCents: 25_000,
      thirdPartyId: 9,
    });
  });
});

describe("helpers TVA", () => {
  it("filtre les modes par nature", () => {
    expect(vatModesForKind("achat").map((m) => m.id)).toEqual([
      "normale",
      "sans",
      "autoliquidation_sous_traitance",
    ]);
    expect(vatModesForKind("vente").map((m) => m.id)).toEqual([
      "normale",
      "sans",
      "autoliquidation_chantier",
    ]);
  });

  it("filtre les codes par sens et mode", () => {
    expect(vatCodesForKind("vente", CODES).map((c) => c.code)).toEqual([
      "T20C",
      "T10C",
    ]);
    expect(vatCodesForKind("achat", CODES).map((c) => c.code)).toEqual([
      "T20D",
      "T55D",
    ]);
    expect(
      vatCodesForKind("achat", CODES, "autoliquidation_sous_traitance").map(
        (c) => c.code,
      ),
    ).toEqual(["T20R", "T10R"]);
  });

  it("choisit le code par défaut selon taux et mode", () => {
    expect(defaultVatCodeFor("vente", 2000, CODES)?.code).toBe("T20C");
    expect(defaultVatCodeFor("achat", 550, CODES)?.code).toBe("T55D");
    expect(
      defaultVatCodeFor("achat", 2000, CODES, "autoliquidation_sous_traitance")
        ?.code,
    ).toBe("T20R");
    expect(
      defaultVatCodeFor("vente", 1000, CODES, "autoliquidation_chantier")?.code,
    ).toBe("T10R");
  });

  it("préfère 604 pour sous-traitance", () => {
    expect(preferredHtAccount("achat", "autoliquidation_sous_traitance")).toBe(
      "604",
    );
    expect(preferredHtAccount("achat", "normale")).toBe("606");
  });

  it("formate les chaînes débit/crédit", () => {
    const { debitStr, creditStr } = eurosStringsFromLines([
      { accountNumber: "706", debitCents: 0, creditCents: 10_000 },
      { accountNumber: "411", debitCents: 10_000, creditCents: 0 },
    ]);
    expect(creditStr[0]).toBe("100.00");
    expect(debitStr[1]).toBe("100.00");
  });
});

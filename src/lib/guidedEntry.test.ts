import { describe, expect, it } from "vitest";
import {
  buildGuidedLines,
  computeVatCents,
  DEFAULT_COUNTERPART,
  defaultVatCodeFor,
  eurosStringsFromLines,
  VAT_ACCOUNT,
  vatCodesForKind,
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

describe("buildGuidedLines — vente", () => {
  it("génère HT + TVA 44571 + client TTC", () => {
    const r = buildGuidedLines({
      kind: "vente",
      htAccount: "706",
      htCents: 10_000,
      withVat: true,
      vatCode: "T20C",
      rateBp: 2000,
      counterpartAccount: DEFAULT_COUNTERPART.vente,
    });
    expect(r.vatCents).toBe(2000);
    expect(r.ttcCents).toBe(12_000);
    expect(r.vatAccount).toBe("44571");
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
      withVat: false,
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

describe("buildGuidedLines — achat", () => {
  it("génère HT + TVA 44566 + fournisseur TTC", () => {
    const r = buildGuidedLines({
      kind: "achat",
      htAccount: "606",
      htCents: 50_000,
      withVat: true,
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

describe("helpers TVA", () => {
  it("filtre les codes par sens", () => {
    expect(vatCodesForKind("vente", CODES).map((c) => c.code)).toEqual([
      "T20C",
      "T10C",
    ]);
    expect(vatCodesForKind("achat", CODES).map((c) => c.code)).toEqual([
      "T20D",
      "T55D",
    ]);
  });

  it("choisit le code par défaut selon taux", () => {
    expect(defaultVatCodeFor("vente", 2000, CODES)?.code).toBe("T20C");
    expect(defaultVatCodeFor("achat", 550, CODES)?.code).toBe("T55D");
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

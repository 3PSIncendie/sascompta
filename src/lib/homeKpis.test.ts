import { describe, expect, it } from "vitest";
import {
  homePeriods,
  kpisFromMonthStatements,
  monthStartYmd,
  previewHomeKpis,
  todayYmd,
  tresorerieFromStatements,
} from "./homeKpis";
import type { AnnualStatements } from "./types";

function emptyPack(
  overrides: Partial<AnnualStatements> & {
    compteResultat?: Partial<AnnualStatements["compteResultat"]>;
    bilan?: Partial<AnnualStatements["bilan"]>;
  } = {},
): AnnualStatements {
  return {
    organizationName: "Demo",
    siren: "123456782",
    bilan: {
      asOf: "2026-11-22",
      fromDate: "2026-01-01",
      toDate: "2026-11-22",
      actif: [],
      passif: [],
      totalActifCents: 0,
      totalPassifCents: 0,
      ecartCents: 0,
      resultatCents: 0,
      ...overrides.bilan,
    },
    compteResultat: {
      fromDate: "2026-11-01",
      toDate: "2026-11-22",
      charges: [],
      produits: [],
      totalChargesCents: 0,
      totalProduitsCents: 0,
      resultatCents: 0,
      ...overrides.compteResultat,
    },
    annexe: {
      asOf: "2026-11-22",
      methods: [],
      immobilisations: [],
      dettes: [],
    },
    ...overrides,
  };
}

describe("homeKpis dates", () => {
  it("formats today as local YMD", () => {
    expect(todayYmd(new Date(2026, 10, 22))).toBe("2026-11-22");
  });

  it("computes month start", () => {
    expect(monthStartYmd("2026-11-22")).toBe("2026-11-01");
  });

  it("clamps MTD to fiscal year bounds", () => {
    const p = homePeriods(
      { startDate: "2026-04-01", endDate: "2027-03-31", label: "2026-2027" },
      new Date(2026, 3, 15), // 15 avril
    );
    expect(p.monthFrom).toBe("2026-04-01");
    expect(p.monthTo).toBe("2026-04-15");
    expect(p.fyFrom).toBe("2026-04-01");
  });
});

describe("homeKpis extraction", () => {
  it("reads P&L totals", () => {
    const pack = emptyPack({
      compteResultat: {
        fromDate: "2026-11-01",
        toDate: "2026-11-22",
        charges: [],
        produits: [],
        totalProduitsCents: 100_00,
        totalChargesCents: 40_00,
        resultatCents: 60_00,
      },
    });
    expect(kpisFromMonthStatements(pack)).toEqual({
      caCents: 100_00,
      chargesCents: 40_00,
      resultatCents: 60_00,
    });
  });

  it("sums banque + caisse + autres dispo", () => {
    const pack = emptyPack({
      bilan: {
        asOf: "2026-11-22",
        fromDate: "2026-01-01",
        toDate: "2026-11-22",
        actif: [
          { code: "A-BQ", label: "Banque", amountCents: 50_00, accounts: ["512"] },
          { code: "A-CAI", label: "Caisse", amountCents: 10_00, accounts: ["53"] },
          {
            code: "A-TRESO",
            label: "Autres disponibilités",
            amountCents: 5_00,
            accounts: ["50"],
          },
          { code: "A-CLI", label: "Clients", amountCents: 999_00, accounts: ["411"] },
        ],
        passif: [],
        totalActifCents: 0,
        totalPassifCents: 0,
        ecartCents: 0,
        resultatCents: 0,
      },
    });
    expect(tresorerieFromStatements(pack)).toBe(65_00);
  });

  it("provides preview figures", () => {
    expect(previewHomeKpis().caCents).toBeGreaterThan(0);
  });
});

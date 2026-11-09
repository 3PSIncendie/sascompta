import { describe, expect, it } from "vitest";
import {
  buildDeadlines,
  formatDueLabel,
  nextMonthlyVatDue,
  nextQuarterlyVatDue,
  urgentDeadlines,
} from "./deadlines";

describe("nextMonthlyVatDue", () => {
  it("reste sur le 19 du mois si on n’a pas dépassé", () => {
    expect(nextMonthlyVatDue("2026-05-10")).toBe("2026-05-19");
  });

  it("passe au mois suivant après le 19", () => {
    expect(nextMonthlyVatDue("2026-05-20")).toBe("2026-06-19");
  });
});

describe("nextQuarterlyVatDue", () => {
  it("pointe vers l’échéance du trimestre en cours ou suivant", () => {
    // Q1 clôturé au 31/03 → échéance ~19/04
    expect(nextQuarterlyVatDue("2026-04-01")).toBe("2026-04-19");
    // Avant fin Q1 → prochaine échéance 19/04
    expect(nextQuarterlyVatDue("2026-01-05")).toBe("2026-04-19");
    // Après le 19/04 → prochaine = 19/07 (Q2)
    expect(nextQuarterlyVatDue("2026-04-20")).toBe("2026-07-19");
  });
});

describe("buildDeadlines", () => {
  it("inclut TVA, clôture, AG et dépôt", () => {
    const items = buildDeadlines({
      today: "2026-05-01",
      fiscalYearEnd: "2026-12-31",
      vatRegime: "reel_mensuel",
    });
    expect(items.map((i) => i.kind).sort()).toEqual(
      ["ag", "cloture", "depot", "einvoice", "tva"].sort(),
    );
    expect(items.find((i) => i.kind === "tva")?.dueDate).toBe("2026-05-19");
  });

  it("signale les urgences", () => {
    const items = buildDeadlines({
      today: "2026-05-15",
      fiscalYearEnd: "2026-12-31",
      vatRegime: "mensuel",
    });
    const urgent = urgentDeadlines(items);
    expect(urgent.some((i) => i.kind === "tva")).toBe(true);
  });
});

describe("formatDueLabel", () => {
  it("formule le délai en français", () => {
    expect(formatDueLabel("2026-05-01", "2026-05-01")).toBe("aujourd’hui");
    expect(formatDueLabel("2026-05-01", "2026-05-02")).toBe("demain");
    expect(formatDueLabel("2026-05-10", "2026-05-01")).toContain("retard");
  });
});

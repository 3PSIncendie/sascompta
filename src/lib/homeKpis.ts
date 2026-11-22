import type { AnnualStatements, FiscalYear, LedgerFilter } from "./types";

/** Local calendar date as YYYY-MM-DD (not UTC). */
export function todayYmd(now = new Date()): string {
  const y = now.getFullYear();
  const m = String(now.getMonth() + 1).padStart(2, "0");
  const d = String(now.getDate()).padStart(2, "0");
  return `${y}-${m}-${d}`;
}

/** First day of the calendar month containing `ymd`, or of `ymd`’s month. */
export function monthStartYmd(ymd: string): string {
  return `${ymd.slice(0, 7)}-01`;
}

function clampYmd(value: string, min: string, max: string): string {
  if (value < min) return min;
  if (value > max) return max;
  return value;
}

export interface HomePeriod {
  /** Month-to-date within the current fiscal year (CA / charges / résultat). */
  monthFrom: string;
  monthTo: string;
  /** FY start → as-of (trésorerie stock). */
  fyFrom: string;
  fyTo: string;
  /** Human label e.g. « novembre 2026 ». */
  monthLabel: string;
  fyLabel: string;
}

/**
 * Periods for home KPIs: month-to-date clamped to the current exercice,
 * plus FY-to-date for cash stock. Never spans outside `fy` dates.
 */
export function homePeriods(
  fy: Pick<FiscalYear, "startDate" | "endDate" | "label">,
  now = new Date(),
): HomePeriod {
  const today = clampYmd(todayYmd(now), fy.startDate, fy.endDate);
  const rawMonthStart = monthStartYmd(today);
  const monthFrom = clampYmd(rawMonthStart, fy.startDate, fy.endDate);
  const monthLabel = new Intl.DateTimeFormat("fr-FR", {
    month: "long",
    year: "numeric",
  }).format(new Date(`${today}T12:00:00`));
  return {
    monthFrom,
    monthTo: today,
    fyFrom: fy.startDate,
    fyTo: today,
    monthLabel,
    fyLabel: fy.label,
  };
}

/**
 * Ledger filters for Accueil KPIs — always scoped to the current exercice id
 * so prior years stay separate (independent closing).
 */
export function homeKpiFilters(
  fy: Pick<FiscalYear, "id" | "startDate" | "endDate" | "label">,
  now = new Date(),
): { period: HomePeriod; month: LedgerFilter; fyToDate: LedgerFilter } {
  const period = homePeriods(fy, now);
  const scope = {
    journalCode: null as string | null,
    accountNumber: null as string | null,
    fiscalYearId: fy.id,
  };
  return {
    period,
    month: {
      fromDate: period.monthFrom,
      toDate: period.monthTo,
      ...scope,
    },
    fyToDate: {
      fromDate: period.fyFrom,
      toDate: period.fyTo,
      ...scope,
    },
  };
}

export interface HomeKpis {
  caCents: number;
  chargesCents: number;
  resultatCents: number;
  /** Banque + caisse + autres disponibilités (postes A-BQ, A-CAI, A-TRESO). */
  tresorerieCents: number;
}

const TRESO_CODES = new Set(["A-BQ", "A-CAI", "A-TRESO"]);

/** P&L KPIs from a month-scoped compte de résultat pack. */
export function kpisFromMonthStatements(pack: AnnualStatements): Pick<
  HomeKpis,
  "caCents" | "chargesCents" | "resultatCents"
> {
  return {
    caCents: pack.compteResultat.totalProduitsCents,
    chargesCents: pack.compteResultat.totalChargesCents,
    resultatCents: pack.compteResultat.resultatCents,
  };
}

/** Cash stock from a FY-to-date bilan pack. */
export function tresorerieFromStatements(pack: AnnualStatements): number {
  return pack.bilan.actif
    .filter((l) => TRESO_CODES.has(l.code))
    .reduce((sum, l) => sum + l.amountCents, 0);
}

export function mergeHomeKpis(
  monthPack: AnnualStatements,
  fyPack: AnnualStatements,
): HomeKpis {
  return {
    ...kpisFromMonthStatements(monthPack),
    tresorerieCents: tresorerieFromStatements(fyPack),
  };
}

/** Aperçu navigateur (`?preview`) — chiffres fictifs cohérents. */
export function previewHomeKpis(): HomeKpis {
  return {
    caCents: 48_520_00,
    chargesCents: 31_140_00,
    resultatCents: 17_380_00,
    tresorerieCents: 62_450_00,
  };
}

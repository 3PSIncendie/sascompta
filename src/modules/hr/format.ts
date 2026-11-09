/** Helpers RH — jours/heures en bp (100 = 1,00). */

export const CONTRACT_KIND_LABELS: Record<string, string> = {
  cdi: "CDI",
  cdd: "CDD",
  apprentice: "Apprentissage",
};

export const CONTRACT_STATUS_LABELS: Record<string, string> = {
  draft: "Brouillon",
  active: "Actif",
  ended: "Terminé",
};

export const LEAVE_STATUS_LABELS: Record<string, string> = {
  pending: "En attente",
  approved: "Approuvé",
  refused: "Refusé",
  cancelled: "Annulé",
};

export const PAYSLIP_STATUS_LABELS: Record<string, string> = {
  draft: "Brouillon",
  validated: "Validé",
};

export function formatDaysBp(bp: number): string {
  const whole = Math.trunc(bp / 100);
  const frac = Math.abs(bp % 100);
  if (frac === 0) return `${whole}\u00a0j`;
  return `${whole},${frac.toString().padStart(2, "0")}\u00a0j`;
}

export function parseDaysToBp(input: string): number {
  const cleaned = input.trim().replace(/\s/g, "").replace(",", ".").replace(/j$/i, "");
  if (!cleaned) return 0;
  const n = Number(cleaned);
  if (Number.isNaN(n)) return 0;
  return Math.round(n * 100);
}

export function formatHoursBp(bp: number): string {
  const whole = Math.trunc(bp / 100);
  const frac = Math.abs(bp % 100);
  if (frac === 0) return `${whole}\u00a0h`;
  return `${whole},${frac.toString().padStart(2, "0")}\u00a0h`;
}

export function parseHoursToBp(input: string): number {
  const cleaned = input.trim().replace(/\s/g, "").replace(",", ".").replace(/h$/i, "");
  if (!cleaned) return 0;
  const n = Number(cleaned);
  if (Number.isNaN(n)) return 0;
  return Math.round(n * 100);
}

export function formatRateBp(bp: number): string {
  const whole = Math.floor(bp / 100);
  const frac = bp % 100;
  return `${whole},${frac.toString().padStart(2, "0")}\u00a0%`;
}

export function parseRateToBp(input: string): number {
  const cleaned = input.trim().replace(/\s/g, "").replace(",", ".").replace("%", "");
  if (!cleaned) return 0;
  const n = Number(cleaned);
  if (Number.isNaN(n)) return 0;
  return Math.round(n * 100);
}

/** Calcul local miroir du backend (tests / preview). */
export function computePayslipAmounts(
  grossCents: number,
  employeeRateBp: number,
  employerRateBp: number,
  pasRateBp = 0,
): {
  employeeChargesCents: number;
  employerChargesCents: number;
  pasCents: number;
  netCents: number;
} {
  const employeeChargesCents = Math.trunc((grossCents * employeeRateBp) / 10_000);
  const employerChargesCents = Math.trunc((grossCents * employerRateBp) / 10_000);
  const taxable = grossCents - employeeChargesCents;
  const pasCents = Math.trunc((Math.max(0, taxable) * pasRateBp) / 10_000);
  return {
    employeeChargesCents,
    employerChargesCents,
    pasCents,
    netCents: taxable - pasCents,
  };
}

export function employeeDisplayName(
  firstName: string,
  lastName: string,
): string {
  return `${firstName.trim()} ${lastName.trim()}`.trim();
}

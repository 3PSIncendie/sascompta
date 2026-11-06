/** Format cents as French euros: 123456 → "1 234,56 €" */
export function formatEuros(cents: number): string {
  const neg = cents < 0;
  const abs = Math.abs(cents);
  const euros = Math.floor(abs / 100);
  const rem = abs % 100;
  const grouped = euros
    .toString()
    .replace(/\B(?=(\d{3})+(?!\d))/g, "\u00a0");
  const s = `${grouped},${rem.toString().padStart(2, "0")}\u00a0€`;
  return neg ? `−${s}` : s;
}

export function parseEurosToCents(input: string): number {
  const cleaned = input.trim().replace(/\s/g, "").replace(",", ".").replace("€", "");
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

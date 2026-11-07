import type { EntryLineInput, VatCode } from "./types";

/** Sens de l’opération pour la saisie guidée. */
export type GuidedKind = "achat" | "vente";

export type GuidedEntryInput = {
  kind: GuidedKind;
  /** Compte de charge (6xx) ou produit (7xx). */
  htAccount: string;
  /** Montant hors taxes en centimes. */
  htCents: number;
  /** false = pas de TVA (franchise / exonéré). */
  withVat: boolean;
  /** Code TVA (T20C / T20D…). Requis si withVat. */
  vatCode: string | null;
  /** Taux en points de base (2000 = 20 %). Requis si withVat. */
  rateBp: number;
  /** Contrepartie (401, 411, 512…). */
  counterpartAccount: string;
  /** Tiers optionnel sur la ligne de contrepartie. */
  thirdPartyId?: number | null;
};

export type GuidedEntryResult = {
  lines: EntryLineInput[];
  htCents: number;
  vatCents: number;
  ttcCents: number;
  vatAccount: string | null;
};

export const DEFAULT_COUNTERPART: Record<GuidedKind, string> = {
  achat: "401",
  vente: "411",
};

export const VAT_ACCOUNT: Record<GuidedKind, string> = {
  achat: "44566",
  vente: "44571",
};

/** TVA arrondie au centime le plus proche (méthode française usuelle). */
export function computeVatCents(htCents: number, rateBp: number): number {
  if (htCents <= 0 || rateBp <= 0) return 0;
  return Math.round((htCents * rateBp) / 10_000);
}

export function defaultVatCodeFor(
  kind: GuidedKind,
  rateBp: number,
  codes: VatCode[],
): VatCode | null {
  const direction = kind === "achat" ? "deductible" : "collected";
  return (
    codes.find(
      (c) => c.active && c.direction === direction && c.rateBp === rateBp,
    ) ??
    codes.find((c) => c.active && c.direction === direction) ??
    null
  );
}

export function vatCodesForKind(kind: GuidedKind, codes: VatCode[]): VatCode[] {
  const direction = kind === "achat" ? "deductible" : "collected";
  return codes.filter((c) => c.active && c.direction === direction);
}

/**
 * À partir d’une seule intention (HT + compte + TVA), génère les lignes
 * équilibrées : HT, éventuelle 445x, contrepartie TTC.
 * Les lignes restent ensuite éditables dans la saisie classique.
 */
export function buildGuidedLines(input: GuidedEntryInput): GuidedEntryResult {
  const ht = Math.max(0, Math.round(input.htCents));
  const vat =
    input.withVat && input.rateBp > 0
      ? computeVatCents(ht, input.rateBp)
      : 0;
  const ttc = ht + vat;
  const vatAccount = input.withVat && vat > 0 ? VAT_ACCOUNT[input.kind] : null;
  const vatCode =
    input.withVat && vat > 0 ? input.vatCode : null;

  const lines: EntryLineInput[] = [];

  if (input.kind === "vente") {
    // Crédit produit HT
    lines.push({
      accountNumber: input.htAccount,
      debitCents: 0,
      creditCents: ht,
      vatCode,
      thirdPartyId: null,
    });
    if (vatAccount && vat > 0) {
      lines.push({
        accountNumber: vatAccount,
        debitCents: 0,
        creditCents: vat,
        vatCode,
        thirdPartyId: null,
      });
    }
    // Débit client / banque TTC
    lines.push({
      accountNumber: input.counterpartAccount,
      debitCents: ttc,
      creditCents: 0,
      vatCode: null,
      thirdPartyId: input.thirdPartyId ?? null,
    });
  } else {
    // Débit charge HT
    lines.push({
      accountNumber: input.htAccount,
      debitCents: ht,
      creditCents: 0,
      vatCode,
      thirdPartyId: null,
    });
    if (vatAccount && vat > 0) {
      lines.push({
        accountNumber: vatAccount,
        debitCents: vat,
        creditCents: 0,
        vatCode,
        thirdPartyId: null,
      });
    }
    // Crédit fournisseur / banque TTC
    lines.push({
      accountNumber: input.counterpartAccount,
      debitCents: 0,
      creditCents: ttc,
      vatCode: null,
      thirdPartyId: input.thirdPartyId ?? null,
    });
  }

  return { lines, htCents: ht, vatCents: vat, ttcCents: ttc, vatAccount };
}

export function suggestHtAccounts(
  kind: GuidedKind,
  accounts: { number: string; label: string }[],
): { number: string; label: string }[] {
  const prefix = kind === "achat" ? "6" : "7";
  return accounts.filter((a) => a.number.startsWith(prefix));
}

export function eurosStringsFromLines(lines: EntryLineInput[]): {
  debitStr: string[];
  creditStr: string[];
} {
  return {
    debitStr: lines.map((l) =>
      l.debitCents ? (l.debitCents / 100).toFixed(2) : "",
    ),
    creditStr: lines.map((l) =>
      l.creditCents ? (l.creditCents / 100).toFixed(2) : "",
    ),
  };
}

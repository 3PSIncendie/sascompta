import type { EntryLineInput, VatCode } from "./types";

/** Sens de l’opération pour la saisie guidée. */
export type GuidedKind = "achat" | "vente";

/**
 * Régime TVA de l’intention — choix clair pour débutants BTP / SAS.
 * - normale : TVA collectée (vente) ou déductible (achat)
 * - sans : franchise, hors champ, exonération classique
 * - autoliquidation_sous_traitance : achat sous-traitance chantier (art. 283)
 * - autoliquidation_chantier : vente travaux / chantier en direct (art. 283)
 */
export type GuidedVatMode =
  | "normale"
  | "sans"
  | "autoliquidation_sous_traitance"
  | "autoliquidation_chantier";

export type GuidedEntryInput = {
  kind: GuidedKind;
  /** Compte de charge (6xx) ou produit (7xx). */
  htAccount: string;
  /** Montant hors taxes en centimes. */
  htCents: number;
  /** Régime TVA (remplace l’ancien booléen withVat). */
  vatMode: GuidedVatMode;
  /** Code TVA. Requis sauf mode « sans ». */
  vatCode: string | null;
  /** Taux en points de base (2000 = 20 %). 0 si sans TVA. */
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
  /** Compte TVA principal (déductible ou collectée). */
  vatAccount: string | null;
  /** Compte TVA due (autoliquidation achat) — 4452. */
  vatDueAccount: string | null;
  vatMode: GuidedVatMode;
};

export const DEFAULT_COUNTERPART: Record<GuidedKind, string> = {
  achat: "401",
  vente: "411",
};

export const VAT_ACCOUNT: Record<GuidedKind, string> = {
  achat: "44566",
  vente: "44571",
};

/** TVA due par autoliquidation (BTP / art. 283) — côté donneur d’ordre. */
export const VAT_DUE_ACCOUNT = "4452";

/** Compte charge sous-traitance (PCG). */
export const SUBCONTRACT_HT_ACCOUNT = "604";

export const VAT_MODE_OPTIONS: {
  id: GuidedVatMode;
  label: string;
  kinds: GuidedKind[];
  shortHelp: string;
}[] = [
  {
    id: "normale",
    label: "TVA classique",
    kinds: ["achat", "vente"],
    shortHelp: "TVA facturée / récupérable au taux choisi.",
  },
  {
    id: "sans",
    label: "Sans TVA (franchise / exonéré)",
    kinds: ["achat", "vente"],
    shortHelp: "Pas de ventilation 445… (franchise en base, hors champ…).",
  },
  {
    id: "autoliquidation_sous_traitance",
    label: "Autoliquidation — sous-traitance chantier (art. 283)",
    kinds: ["achat"],
    shortHelp:
      "Facture sous-traitant HT : TVA due (4452) + déductible (44566), fournisseur au HT.",
  },
  {
    id: "autoliquidation_chantier",
    label: "Autoliquidation — chantier / travaux BTP (art. 283)",
    kinds: ["vente"],
    shortHelp:
      "Vous facturez les travaux HT : le client assujetti autoliquide ; pas de 44571.",
  },
];

export function vatModesForKind(kind: GuidedKind) {
  return VAT_MODE_OPTIONS.filter((o) => o.kinds.includes(kind));
}

/** TVA arrondie au centime le plus proche (méthode française usuelle). */
export function computeVatCents(htCents: number, rateBp: number): number {
  if (htCents <= 0 || rateBp <= 0) return 0;
  return Math.round((htCents * rateBp) / 10_000);
}

export function defaultVatCodeFor(
  kind: GuidedKind,
  rateBp: number,
  codes: VatCode[],
  mode: GuidedVatMode = "normale",
): VatCode | null {
  if (mode === "sans") {
    return codes.find((c) => c.active && c.direction === "exempt") ?? null;
  }
  if (
    mode === "autoliquidation_sous_traitance" ||
    mode === "autoliquidation_chantier"
  ) {
    return (
      codes.find(
        (c) => c.active && c.direction === "reverse" && c.rateBp === rateBp,
      ) ??
      codes.find((c) => c.active && c.direction === "reverse") ??
      null
    );
  }
  const direction = kind === "achat" ? "deductible" : "collected";
  return (
    codes.find(
      (c) => c.active && c.direction === direction && c.rateBp === rateBp,
    ) ??
    codes.find((c) => c.active && c.direction === direction) ??
    null
  );
}

export function vatCodesForKind(
  kind: GuidedKind,
  codes: VatCode[],
  mode: GuidedVatMode = "normale",
): VatCode[] {
  if (mode === "sans") {
    return codes.filter((c) => c.active && c.direction === "exempt");
  }
  if (
    mode === "autoliquidation_sous_traitance" ||
    mode === "autoliquidation_chantier"
  ) {
    return codes.filter((c) => c.active && c.direction === "reverse");
  }
  const direction = kind === "achat" ? "deductible" : "collected";
  return codes.filter((c) => c.active && c.direction === direction);
}

export function preferredHtAccount(kind: GuidedKind, mode: GuidedVatMode): string {
  if (kind === "achat" && mode === "autoliquidation_sous_traitance") {
    return SUBCONTRACT_HT_ACCOUNT;
  }
  return kind === "vente" ? "706" : "606";
}

/**
 * À partir d’une seule intention (HT + compte + régime TVA), génère les lignes
 * équilibrées. Les lignes restent ensuite éditables dans la saisie classique.
 */
export function buildGuidedLines(input: GuidedEntryInput): GuidedEntryResult {
  const ht = Math.max(0, Math.round(input.htCents));
  const mode = input.vatMode;
  const needsVatAmount =
    mode === "normale" || mode === "autoliquidation_sous_traitance";
  const vat =
    needsVatAmount && input.rateBp > 0
      ? computeVatCents(ht, input.rateBp)
      : 0;
  const vatCode = mode === "sans" ? null : input.vatCode;

  if (mode === "autoliquidation_sous_traitance") {
    // Donneur d’ordre : charge HT + TVA déductible + TVA due + fournisseur HT
    const lines: EntryLineInput[] = [
      {
        accountNumber: input.htAccount,
        debitCents: ht,
        creditCents: 0,
        vatCode,
        thirdPartyId: null,
      },
    ];
    if (vat > 0) {
      lines.push({
        accountNumber: VAT_ACCOUNT.achat,
        debitCents: vat,
        creditCents: 0,
        vatCode,
        thirdPartyId: null,
      });
      lines.push({
        accountNumber: VAT_DUE_ACCOUNT,
        debitCents: 0,
        creditCents: vat,
        vatCode,
        thirdPartyId: null,
      });
    }
    lines.push({
      accountNumber: input.counterpartAccount,
      debitCents: 0,
      creditCents: ht,
      vatCode: null,
      thirdPartyId: input.thirdPartyId ?? null,
    });
    return {
      lines,
      htCents: ht,
      vatCents: vat,
      ttcCents: ht,
      vatAccount: vat > 0 ? VAT_ACCOUNT.achat : null,
      vatDueAccount: vat > 0 ? VAT_DUE_ACCOUNT : null,
      vatMode: mode,
    };
  }

  if (mode === "autoliquidation_chantier") {
    // Prestataire BTP : facture HT, client autoliquide — pas de 44571
    const lines: EntryLineInput[] = [
      {
        accountNumber: input.htAccount,
        debitCents: 0,
        creditCents: ht,
        vatCode,
        thirdPartyId: null,
      },
      {
        accountNumber: input.counterpartAccount,
        debitCents: ht,
        creditCents: 0,
        vatCode: null,
        thirdPartyId: input.thirdPartyId ?? null,
      },
    ];
    return {
      lines,
      htCents: ht,
      vatCents: 0,
      ttcCents: ht,
      vatAccount: null,
      vatDueAccount: null,
      vatMode: mode,
    };
  }

  // normale | sans
  const withVat = mode === "normale" && vat > 0;
  const ttc = ht + (withVat ? vat : 0);
  const vatAccount = withVat ? VAT_ACCOUNT[input.kind] : null;
  const code = withVat ? vatCode : mode === "sans" ? null : vatCode;

  const lines: EntryLineInput[] = [];

  if (input.kind === "vente") {
    lines.push({
      accountNumber: input.htAccount,
      debitCents: 0,
      creditCents: ht,
      vatCode: code,
      thirdPartyId: null,
    });
    if (vatAccount && vat > 0) {
      lines.push({
        accountNumber: vatAccount,
        debitCents: 0,
        creditCents: vat,
        vatCode: code,
        thirdPartyId: null,
      });
    }
    lines.push({
      accountNumber: input.counterpartAccount,
      debitCents: ttc,
      creditCents: 0,
      vatCode: null,
      thirdPartyId: input.thirdPartyId ?? null,
    });
  } else {
    lines.push({
      accountNumber: input.htAccount,
      debitCents: ht,
      creditCents: 0,
      vatCode: code,
      thirdPartyId: null,
    });
    if (vatAccount && vat > 0) {
      lines.push({
        accountNumber: vatAccount,
        debitCents: vat,
        creditCents: 0,
        vatCode: code,
        thirdPartyId: null,
      });
    }
    lines.push({
      accountNumber: input.counterpartAccount,
      debitCents: 0,
      creditCents: ttc,
      vatCode: null,
      thirdPartyId: input.thirdPartyId ?? null,
    });
  }

  return {
    lines,
    htCents: ht,
    vatCents: withVat ? vat : 0,
    ttcCents: ttc,
    vatAccount,
    vatDueAccount: null,
    vatMode: mode,
  };
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

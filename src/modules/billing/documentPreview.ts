import { formatEuros, parseEurosToCents } from "../../lib/money";
import type { Organization, ThirdParty } from "../../lib/types";
import {
  draftLinesToInput,
  legalMentionsFor,
  summarizeQuoteLines,
  type DraftLine,
} from "./quotes";

export type BillingDocKind = "quote" | "invoice";

export type PreviewMetaRow = { label: string; value: string };

export type PreviewLine = {
  label: string;
  description: string | null;
  quantityLabel: string;
  unitPriceLabel: string;
  vatLabel: string;
  htLabel: string;
};

export type PreviewParty = {
  name: string;
  addressLines: string[];
  email: string | null;
  idLine: string | null;
};

export type BillingDocumentPreviewModel = {
  title: string;
  number: string;
  meta: PreviewMetaRow[];
  org: PreviewParty;
  client: PreviewParty;
  lines: PreviewLine[];
  htLabel: string;
  vatLabel: string;
  ttcLabel: string;
  notes: string | null;
  legalMentions: string | null;
  footerLeft: string;
};

/** YYYY-MM-DD → DD/MM/YYYY (miroir PDF Rust). */
export function formatDateFr(iso: string): string {
  const t = iso.trim();
  if (/^\d{4}-\d{2}-\d{2}/.test(t)) {
    return `${t.slice(8, 10)}/${t.slice(5, 7)}/${t.slice(0, 4)}`;
  }
  return t;
}

/** Quantité milli → affichage FR (1 / 1,5 / 2,250). */
export function formatQtyFr(quantityMilli: number): string {
  if (quantityMilli % 1000 === 0) {
    return String(Math.trunc(quantityMilli / 1000));
  }
  const whole = Math.trunc(quantityMilli / 1000);
  const frac = Math.abs(quantityMilli % 1000)
    .toString()
    .padStart(3, "0")
    .replace(/0+$/, "");
  return `${whole},${frac}`;
}

/** Taux bp → « 20 % » / « 5,50 % ». */
export function formatVatPctFr(bp: number): string {
  if (bp % 100 === 0) return `${Math.trunc(bp / 100)} %`;
  return `${Math.trunc(bp / 100)},${String(bp % 100).padStart(2, "0")} %`;
}

export function spacedSiren(siren: string): string {
  const digits = siren.replace(/\D/g, "");
  if (digits.length === 9) {
    return `${digits.slice(0, 3)} ${digits.slice(3, 6)} ${digits.slice(6, 9)}`;
  }
  return siren.trim();
}

export function cityFromAddress(address: string | null | undefined): string | null {
  if (!address) return null;
  const last = address
    .split("\n")
    .map((l) => l.trim())
    .filter(Boolean)
    .at(-1);
  if (!last) return null;
  const tokens = last
    .split(/[,\s]+/)
    .filter(Boolean)
    .filter((t) => !/^\d+$/.test(t))
    .filter((t) => t.length > 1 && t.toUpperCase() !== "FR");
  return tokens[0] ?? null;
}

/** Pied de page identité (miroir `commercial_footer_left`). */
export function commercialFooterLeft(org: Organization): string {
  const city = cityFromAddress(org.address);
  const siren = spacedSiren(org.siren);
  if (city && siren) {
    return `${org.name}, ${org.legalForm} · ${city} ${siren}`;
  }
  if (city) return `${org.name}, ${org.legalForm} · ${city}`;
  if (siren) return `${org.name}, ${org.legalForm} · ${siren}`;
  return `${org.name}, ${org.legalForm}`;
}

function partyFromOrg(org: Organization): PreviewParty {
  return {
    name: org.name,
    addressLines: (org.address ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
    email: org.email?.trim() || null,
    idLine: org.siret?.trim() || spacedSiren(org.siren) || null,
  };
}

function partyFromClient(client: ThirdParty | null | undefined): PreviewParty {
  if (!client) {
    return {
      name: "Client à choisir…",
      addressLines: [],
      email: null,
      idLine: null,
    };
  }
  return {
    name: client.name,
    addressLines: (client.address ?? "")
      .split("\n")
      .map((l) => l.trim())
      .filter(Boolean),
    email: client.email?.trim() || null,
    idLine: client.siret?.trim() || null,
  };
}

export function invoiceTitleForKind(kind: string): string {
  switch (kind) {
    case "deposit":
      return "Facture d'acompte";
    case "balance":
      return "Facture de solde";
    case "credit_note":
      return "Avoir";
    default:
      return "Facture";
  }
}

export type BuildBillingPreviewInput = {
  docKind: BillingDocKind;
  /** Titre document (Devis / Facture / …). */
  title: string;
  number: string;
  issueDate: string;
  /** Quote: validUntil ; Invoice: dueDate. */
  secondaryDate: string;
  secondaryDateLabel: string;
  org: Organization;
  client: ThirdParty | null | undefined;
  vatMode: string;
  notes: string;
  lines: DraftLine[];
  extraMeta?: PreviewMetaRow[];
};

/** Construit le modèle d’aperçu live à partir des champs du formulaire. */
export function buildBillingDocumentPreview(
  input: BuildBillingPreviewInput,
): BillingDocumentPreviewModel {
  const mapped = draftLinesToInput(input.lines, parseEurosToCents);
  const totals = summarizeQuoteLines(mapped, input.vatMode);
  const legal = legalMentionsFor(input.vatMode) || null;

  const meta: PreviewMetaRow[] = [
    {
      label:
        input.docKind === "quote" ? "Numéro de devis" : "Numéro de facture",
      value: input.number,
    },
    {
      label: "Date d'émission",
      value: input.issueDate ? formatDateFr(input.issueDate) : "—",
    },
  ];
  if (input.secondaryDate.trim()) {
    meta.push({
      label: input.secondaryDateLabel,
      value: formatDateFr(input.secondaryDate),
    });
  }
  if (input.extraMeta?.length) {
    meta.push(...input.extraMeta);
  }

  const lines: PreviewLine[] = mapped.map((l) => ({
    label: l.label,
    description: l.description ?? null,
    quantityLabel: formatQtyFr(l.quantityMilli),
    unitPriceLabel: formatEuros(l.unitPriceCents),
    vatLabel: formatVatPctFr(l.vatRateBp),
    htLabel: formatEuros(l.htCents ?? 0),
  }));

  return {
    title: input.title,
    number: input.number,
    meta,
    org: partyFromOrg(input.org),
    client: partyFromClient(input.client),
    lines,
    htLabel: formatEuros(totals.htCents),
    vatLabel: formatEuros(totals.vatCents),
    ttcLabel: formatEuros(totals.ttcCents),
    notes: input.notes.trim() || null,
    legalMentions: legal,
    footerLeft: commercialFooterLeft(input.org),
  };
}

/** Numéro provisoire pour un brouillon (émission réelle à l’enregistrement / émission). */
export function draftPreviewNumber(
  kind: "DEV" | "FACT",
  issueDate: string,
  existing: string | null | undefined,
): string {
  if (existing?.trim()) return existing.trim();
  const iso = issueDate.trim() || new Date().toISOString().slice(0, 10);
  const ym = /^\d{4}-\d{2}/.test(iso) ? iso.slice(0, 7) : "AAAA-MM";
  return `BROUILLON-${kind}-${ym}`;
}

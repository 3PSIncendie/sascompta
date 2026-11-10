/**
 * Import documents — registre extensible (lot 1 = facture fournisseur → ACH).
 *
 * Pour ajouter un type plus tard : 1) classifier Rust, 2) entrée DOC_TYPE_REGISTRY,
 * 3) template de lignes (buildProposedLines), 4) activer `postingSupported`.
 */

import {
  buildGuidedLines,
  type GuidedVatMode,
} from "./guidedEntry";
import type { EntryLineInput } from "./types";

export type DocumentDocType =
  | "supplier_invoice"
  | "customer_invoice"
  | "expense_receipt"
  | "bank_statement"
  | "ticket"
  | "unknown";

export type DocumentImportStatus =
  | "pending_review"
  | "confirmed"
  | "discarded";

export type DocumentExtraction = {
  rawTextPreview?: string;
  extractionMethod?: string;
  docType?: string;
  confidence?: number;
  issueDate?: string | null;
  invoiceNumber?: string | null;
  supplierName?: string | null;
  supplierSiret?: string | null;
  matchedThirdPartyId?: number | null;
  matchedThirdPartyCode?: string | null;
  matchedThirdPartyName?: string | null;
  htCents?: number | null;
  vatCents?: number | null;
  ttcCents?: number | null;
  rateBp?: number | null;
  vatMode?: GuidedVatMode | string | null;
  vatCode?: string | null;
  htAccount?: string | null;
  journalCode?: string | null;
  reverseCharge?: boolean;
  postingSupported?: boolean;
  notes?: string;
  confirmedDocType?: string;
  confirmedHtCents?: number;
  confirmedVatMode?: string;
  confirmedEntryId?: number;
};

export type DocumentImport = {
  id: number;
  docType: DocumentDocType | string;
  sourceFileName: string;
  storedPath: string;
  mimeType: string | null;
  extractionMethod: string;
  extraction: DocumentExtraction;
  confidence: number;
  status: DocumentImportStatus | string;
  entryId: number | null;
  createdAt: string;
  confirmedAt: string | null;
};

export type ConfirmDocumentImportInput = {
  docType: string;
  entryDate: string;
  pieceRef: string;
  label: string;
  journalCode: string;
  htCents: number;
  vatCents?: number | null;
  rateBp: number;
  vatMode: string;
  vatCode: string;
  htAccount: string;
  thirdPartyId?: number | null;
  createSupplierName?: string | null;
  createSupplierCode?: string | null;
  createSupplierSiret?: string | null;
};

export type DocTypeMeta = {
  id: DocumentDocType;
  label: string;
  shortHelp: string;
  /** Lot 1 : seul supplier_invoice poste un brouillon. */
  postingSupported: boolean;
  defaultJournal: string;
  comingSoonHint?: string;
};

/** Registre typé — extension = ajouter une entrée + template. */
export const DOC_TYPE_REGISTRY: DocTypeMeta[] = [
  {
    id: "supplier_invoice",
    label: "Facture fournisseur",
    shortHelp:
      "Achat : charge + TVA déductible (ou autoliquidation BTP) + fournisseur 401 → journal ACH.",
    postingSupported: true,
    defaultJournal: "ACH",
  },
  {
    id: "customer_invoice",
    label: "Facture client",
    shortHelp: "Vente : produit + TVA collectée + client 411 → journal VTE.",
    postingSupported: false,
    defaultJournal: "VTE",
    comingSoonHint:
      "Modèle d’écriture VTE prévu au lot suivant. Pour l’instant, choisissez « Facture fournisseur » ou saisissez à la main.",
  },
  {
    id: "expense_receipt",
    label: "Note de frais / reçu",
    shortHelp: "Frais : charge 6xx + TVA + contrepartie (401 / banque).",
    postingSupported: false,
    defaultJournal: "ACH",
    comingSoonHint: "Template frais à venir — extension fine du même écran de revue.",
  },
  {
    id: "bank_statement",
    label: "Relevé bancaire",
    shortHelp: "Import multi-lignes journal de banque (BQ).",
    postingSupported: false,
    defaultJournal: "BQ",
    comingSoonHint: "Relevés bancaires : lot ultérieur (plusieurs écritures).",
  },
  {
    id: "ticket",
    label: "Ticket de caisse",
    shortHelp: "Petit ticket commerçant → souvent frais OD/ACH.",
    postingSupported: false,
    defaultJournal: "OD",
    comingSoonHint: "Tickets : même pipeline, classifier + template dédiés plus tard.",
  },
  {
    id: "unknown",
    label: "Type inconnu — à préciser",
    shortHelp: "Confiance trop basse : choisissez le type avant de confirmer.",
    postingSupported: false,
    defaultJournal: "OD",
    comingSoonHint: "Indiquez le type détecté manuellement.",
  },
];

export function docTypeMeta(id: string): DocTypeMeta {
  return (
    DOC_TYPE_REGISTRY.find((d) => d.id === id) ??
    DOC_TYPE_REGISTRY.find((d) => d.id === "unknown")!
  );
}

export function confidenceLabel(confidence: number): string {
  if (confidence >= 0.75) return "Élevée";
  if (confidence >= 0.45) return "Moyenne — vérifiez";
  return "Faible — complètez à la main";
}

export function centsToEuroInput(cents: number | null | undefined): string {
  if (cents == null || Number.isNaN(cents)) return "";
  return (cents / 100).toFixed(2).replace(".", ",");
}

export type ReviewFormState = {
  docType: string;
  entryDate: string;
  pieceRef: string;
  label: string;
  journalCode: string;
  htStr: string;
  vatStr: string;
  rateBp: number;
  vatMode: GuidedVatMode;
  vatCode: string;
  htAccount: string;
  thirdPartyId: number | null;
  createSupplierName: string;
  createSupplierCode: string;
  createSupplierSiret: string;
};

export function formFromImport(doc: DocumentImport): ReviewFormState {
  const x = doc.extraction ?? {};
  const vatMode = (x.vatMode as GuidedVatMode) || "normale";
  const ht = x.htCents ?? null;
  const vat = x.vatCents ?? null;
  const supplier =
    x.matchedThirdPartyName || x.supplierName || "";
  return {
    docType: doc.docType || "supplier_invoice",
    entryDate: x.issueDate || new Date().toISOString().slice(0, 10),
    pieceRef: x.invoiceNumber || "",
    label: x.invoiceNumber
      ? `Import facture ${x.invoiceNumber}`
      : supplier
        ? `Import facture ${supplier}`
        : "Import facture fournisseur",
    journalCode: x.journalCode || docTypeMeta(doc.docType).defaultJournal || "ACH",
    htStr: centsToEuroInput(ht),
    vatStr: centsToEuroInput(vat),
    rateBp: x.rateBp ?? 2000,
    vatMode,
    vatCode: x.vatCode || "T20D",
    htAccount: x.htAccount || (vatMode === "autoliquidation_sous_traitance" ? "604" : "606"),
    thirdPartyId: x.matchedThirdPartyId ?? null,
    createSupplierName: x.matchedThirdPartyId ? "" : x.supplierName || "",
    createSupplierCode: "",
    createSupplierSiret: x.supplierSiret || "",
  };
}

/** Aperçu des lignes proposées (même logique que la saisie guidée ACH). */
export function buildProposedLines(form: ReviewFormState): EntryLineInput[] {
  if (form.docType !== "supplier_invoice") return [];
  const htCents = Math.round(
    Number(String(form.htStr).replace(/\s/g, "").replace(",", ".")) * 100,
  );
  if (!Number.isFinite(htCents) || htCents <= 0) return [];
  const result = buildGuidedLines({
    kind: "achat",
    htAccount: form.htAccount || "606",
    htCents,
    vatMode: form.vatMode,
    vatCode: form.vatCode,
    rateBp: form.vatMode === "sans" ? 0 : form.rateBp,
    counterpartAccount: "401",
    thirdPartyId: form.thirdPartyId,
  });
  return result.lines;
}

export function toConfirmInput(
  form: ReviewFormState,
  htCents: number,
  vatCents: number | null,
): ConfirmDocumentImportInput {
  return {
    docType: form.docType,
    entryDate: form.entryDate,
    pieceRef: form.pieceRef,
    label: form.label,
    journalCode: form.journalCode || "ACH",
    htCents,
    vatCents,
    rateBp: form.rateBp,
    vatMode: form.vatMode,
    vatCode: form.vatCode,
    htAccount: form.htAccount,
    thirdPartyId: form.thirdPartyId,
    createSupplierName: form.thirdPartyId
      ? null
      : form.createSupplierName || null,
    createSupplierCode: form.thirdPartyId
      ? null
      : form.createSupplierCode || null,
    createSupplierSiret: form.thirdPartyId
      ? null
      : form.createSupplierSiret || null,
  };
}

/** Aperçu navigateur (`?preview`) sans backend Tauri. */
export function previewDocumentImports(): DocumentImport[] {
  return [
    {
      id: 1,
      docType: "supplier_invoice",
      sourceFileName: "FA-2026-0042-delta.pdf",
      storedPath: "/preview/imports/FA-2026-0042-delta.pdf",
      mimeType: "application/pdf",
      extractionMethod: "pdf_text",
      extraction: {
        issueDate: "2026-03-15",
        invoiceNumber: "FA-2026-0042",
        supplierName: "FOURNITURES DELTA SAS",
        supplierSiret: "12345678900012",
        htCents: 10_000,
        vatCents: 2_000,
        ttcCents: 12_000,
        rateBp: 2000,
        vatMode: "normale",
        vatCode: "T20D",
        htAccount: "606",
        journalCode: "ACH",
        postingSupported: true,
        confidence: 0.82,
        notes: "Aperçu — vérifiez avant confirmation.",
        rawTextPreview: "FOURNITURES DELTA SAS\nFACTURE N° FA-2026-0042\nTotal TTC 120,00",
      },
      confidence: 0.82,
      status: "pending_review",
      entryId: null,
      createdAt: "2026-03-15T10:00:00Z",
      confirmedAt: null,
    },
  ];
}

/**
 * Import documents — registre extensible (facture fournisseur, client, frais,
 * tickets, relevés → brouillon journal avec revue humaine).
 *
 * Pour ajouter un type : 1) classifier Rust, 2) entrée DOC_TYPE_REGISTRY,
 * 3) template de lignes (buildProposedLines + Rust), 4) `postingSupported`.
 */

import {
  buildGuidedLines,
  defaultJournalForGuided,
  FRAIS_HT_ACCOUNT,
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

export type BankDirection = "debit" | "credit";

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
  counterpartAccount?: string | null;
  journalCode?: string | null;
  bankDirection?: BankDirection | string | null;
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
  counterpartAccount?: string | null;
  bankDirection?: string | null;
  thirdPartyId?: number | null;
  createSupplierName?: string | null;
  createSupplierCode?: string | null;
  createSupplierSiret?: string | null;
};

export type DocTypeMeta = {
  id: DocumentDocType;
  label: string;
  shortHelp: string;
  postingSupported: boolean;
  defaultJournal: string;
  /** Compte HT / produit / contrepartie mouvement (relevé). */
  defaultHtAccount: string;
  defaultCounterpart: string;
  partyKind: "supplier" | "customer" | "none";
  partyRequired: boolean;
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
    defaultHtAccount: "606",
    defaultCounterpart: "401",
    partyKind: "supplier",
    partyRequired: true,
  },
  {
    id: "customer_invoice",
    label: "Facture client",
    shortHelp: "Vente : produit + TVA collectée + client 411 → journal VTE.",
    postingSupported: true,
    defaultJournal: "VTE",
    defaultHtAccount: "706",
    defaultCounterpart: "411",
    partyKind: "customer",
    partyRequired: true,
  },
  {
    id: "expense_receipt",
    label: "Note de frais / reçu",
    shortHelp:
      "Frais : charge 625 + TVA ; contrepartie 401 (ACH) ou banque/caisse (OD).",
    postingSupported: true,
    defaultJournal: "ACH",
    defaultHtAccount: FRAIS_HT_ACCOUNT,
    defaultCounterpart: "401",
    partyKind: "supplier",
    partyRequired: false,
  },
  {
    id: "bank_statement",
    label: "Relevé bancaire",
    shortHelp:
      "Un mouvement → brouillon BQ (512 ↔ contrepartie, souvent 471). Le PDF reste en pièce jointe.",
    postingSupported: true,
    defaultJournal: "BQ",
    defaultHtAccount: "471",
    defaultCounterpart: "512",
    partyKind: "none",
    partyRequired: false,
  },
  {
    id: "ticket",
    label: "Ticket de caisse",
    shortHelp: "Petit ticket commerçant → frais OD (caisse 530) ou ACH si 401.",
    postingSupported: true,
    defaultJournal: "OD",
    defaultHtAccount: FRAIS_HT_ACCOUNT,
    defaultCounterpart: "530",
    partyKind: "supplier",
    partyRequired: false,
  },
  {
    id: "unknown",
    label: "Type inconnu — à préciser",
    shortHelp: "Confiance trop basse : choisissez le type avant de confirmer.",
    postingSupported: false,
    defaultJournal: "OD",
    defaultHtAccount: "606",
    defaultCounterpart: "401",
    partyKind: "none",
    partyRequired: false,
    comingSoonHint: "Indiquez le type détecté manuellement pour proposer un brouillon.",
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
  counterpartAccount: string;
  bankDirection: BankDirection;
  thirdPartyId: number | null;
  createSupplierName: string;
  createSupplierCode: string;
  createSupplierSiret: string;
};

function defaultLabel(docType: string, piece: string, party: string): string {
  const meta = docTypeMeta(docType);
  if (piece) return `Import ${meta.label.toLowerCase()} ${piece}`;
  if (party) return `Import ${meta.label.toLowerCase()} ${party}`;
  return `Import ${meta.label.toLowerCase()}`;
}

export function formFromImport(doc: DocumentImport): ReviewFormState {
  const x = doc.extraction ?? {};
  const meta = docTypeMeta(doc.docType);
  const vatMode = (x.vatMode as GuidedVatMode) || (meta.id === "bank_statement" ? "sans" : "normale");
  const ht = x.htCents ?? null;
  const vat = x.vatCents ?? null;
  const party = x.matchedThirdPartyName || x.supplierName || "";
  const counterpart =
    x.counterpartAccount || meta.defaultCounterpart || "401";
  return {
    docType: doc.docType || "supplier_invoice",
    entryDate: x.issueDate || new Date().toISOString().slice(0, 10),
    pieceRef: x.invoiceNumber || "",
    label: defaultLabel(doc.docType, x.invoiceNumber || "", party),
    journalCode: x.journalCode || meta.defaultJournal || "ACH",
    htStr: centsToEuroInput(ht),
    vatStr: centsToEuroInput(vat),
    rateBp: x.rateBp ?? 2000,
    vatMode,
    vatCode:
      x.vatCode ||
      (doc.docType === "customer_invoice"
        ? "T20C"
        : doc.docType === "bank_statement"
          ? "TEX"
          : "T20D"),
    htAccount: x.htAccount || meta.defaultHtAccount,
    counterpartAccount: counterpart,
    bankDirection: (x.bankDirection as BankDirection) || "debit",
    thirdPartyId: x.matchedThirdPartyId ?? null,
    createSupplierName: x.matchedThirdPartyId ? "" : x.supplierName || "",
    createSupplierCode: "",
    createSupplierSiret: x.supplierSiret || "",
  };
}

/** Applique les défauts compte / journal quand l’utilisateur change de type. */
export function applyDocTypeDefaults(form: ReviewFormState, docType: string): ReviewFormState {
  const meta = docTypeMeta(docType);
  const vatMode: GuidedVatMode =
    docType === "bank_statement"
      ? "sans"
      : docType === "customer_invoice" && form.vatMode === "autoliquidation_sous_traitance"
        ? "normale"
        : docType === "supplier_invoice" && form.vatMode === "autoliquidation_chantier"
          ? "normale"
          : form.vatMode === "autoliquidation_sous_traitance" &&
              (docType === "expense_receipt" || docType === "ticket")
            ? "normale"
            : form.vatMode;
  const vatCode =
    docType === "customer_invoice"
      ? vatMode === "sans" || vatMode === "autoliquidation_chantier"
        ? vatMode === "sans"
          ? "TEX"
          : form.vatCode || "T20C"
        : "T20C"
      : docType === "bank_statement"
        ? "TEX"
        : vatMode === "autoliquidation_sous_traitance"
          ? "T20R"
          : vatMode === "sans"
            ? "TEX"
            : "T20D";
  return {
    ...form,
    docType,
    journalCode: meta.defaultJournal,
    htAccount:
      docType === "supplier_invoice" && vatMode === "autoliquidation_sous_traitance"
        ? "604"
        : meta.defaultHtAccount,
    counterpartAccount: meta.defaultCounterpart,
    vatMode,
    vatCode,
    bankDirection: form.bankDirection || "debit",
  };
}

function parseHtCents(htStr: string): number {
  return Math.round(
    Number(String(htStr).replace(/\s/g, "").replace(",", ".")) * 100,
  );
}

/** Aperçu des lignes proposées (miroir des templates Rust / saisie guidée). */
export function buildProposedLines(form: ReviewFormState): EntryLineInput[] {
  const meta = docTypeMeta(form.docType);
  if (!meta.postingSupported) return [];
  const htCents = parseHtCents(form.htStr);
  if (!Number.isFinite(htCents) || htCents <= 0) return [];

  if (form.docType === "supplier_invoice") {
    return buildGuidedLines({
      kind: "achat",
      htAccount: form.htAccount || "606",
      htCents,
      vatMode: form.vatMode,
      vatCode: form.vatCode,
      rateBp: form.vatMode === "sans" ? 0 : form.rateBp,
      counterpartAccount: "401",
      thirdPartyId: form.thirdPartyId,
    }).lines;
  }

  if (form.docType === "customer_invoice") {
    const mode =
      form.vatMode === "autoliquidation_chantier" ? form.vatMode : form.vatMode;
    return buildGuidedLines({
      kind: "vente",
      htAccount: form.htAccount || "706",
      htCents,
      vatMode: mode,
      vatCode: form.vatCode,
      rateBp:
        form.vatMode === "sans" || form.vatMode === "autoliquidation_chantier"
          ? 0
          : form.rateBp,
      counterpartAccount: "411",
      thirdPartyId: form.thirdPartyId,
    }).lines;
  }

  if (form.docType === "expense_receipt" || form.docType === "ticket") {
    const counterpart = form.counterpartAccount || meta.defaultCounterpart;
    return buildGuidedLines({
      kind: "frais",
      htAccount: form.htAccount || FRAIS_HT_ACCOUNT,
      htCents,
      vatMode: form.vatMode === "normale" ? "normale" : "sans",
      vatCode: form.vatCode,
      rateBp: form.vatMode === "normale" ? form.rateBp : 0,
      counterpartAccount: counterpart,
      thirdPartyId: counterpart.startsWith("401") ? form.thirdPartyId : null,
    }).lines;
  }

  if (form.docType === "bank_statement") {
    const bank = (form.counterpartAccount || "512").startsWith("512")
      ? form.counterpartAccount || "512"
      : "512";
    const other =
      form.htAccount && !form.htAccount.startsWith("512")
        ? form.htAccount
        : "471";
    if (form.bankDirection === "credit") {
      return [
        {
          accountNumber: bank,
          debitCents: htCents,
          creditCents: 0,
          vatCode: null,
          thirdPartyId: null,
        },
        {
          accountNumber: other,
          debitCents: 0,
          creditCents: htCents,
          vatCode: null,
          thirdPartyId: form.thirdPartyId,
        },
      ];
    }
    return [
      {
        accountNumber: other,
        debitCents: htCents,
        creditCents: 0,
        vatCode: null,
        thirdPartyId: form.thirdPartyId,
      },
      {
        accountNumber: bank,
        debitCents: 0,
        creditCents: htCents,
        vatCode: null,
        thirdPartyId: null,
      },
    ];
  }

  return [];
}

export function suggestedJournal(form: ReviewFormState): string {
  if (form.docType === "expense_receipt" || form.docType === "ticket") {
    return defaultJournalForGuided("frais", form.counterpartAccount || "401");
  }
  return docTypeMeta(form.docType).defaultJournal;
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
    journalCode: form.journalCode || suggestedJournal(form),
    htCents,
    vatCents,
    rateBp: form.rateBp,
    vatMode: form.vatMode,
    vatCode: form.vatCode,
    htAccount: form.htAccount,
    counterpartAccount: form.counterpartAccount || null,
    bankDirection:
      form.docType === "bank_statement" ? form.bankDirection : null,
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
        counterpartAccount: "401",
        journalCode: "ACH",
        postingSupported: true,
        confidence: 0.82,
        notes: "Aperçu — vérifiez avant confirmation.",
        rawTextPreview:
          "FOURNITURES DELTA SAS\nFACTURE N° FA-2026-0042\nTotal TTC 120,00",
      },
      confidence: 0.82,
      status: "pending_review",
      entryId: null,
      createdAt: "2026-03-15T10:00:00Z",
      confirmedAt: null,
    },
    {
      id: 2,
      docType: "ticket",
      sourceFileName: "ticket-boulangerie.pdf",
      storedPath: "/preview/imports/ticket-boulangerie.pdf",
      mimeType: "application/pdf",
      extractionMethod: "pdf_text",
      extraction: {
        issueDate: "2026-03-16",
        supplierName: "Boulangerie du Parc",
        htCents: 417,
        vatCents: 83,
        ttcCents: 500,
        rateBp: 2000,
        vatMode: "normale",
        vatCode: "T20D",
        htAccount: "625",
        counterpartAccount: "530",
        journalCode: "OD",
        postingSupported: true,
        notes: "Ticket — contrepartie caisse 530 par défaut.",
        rawTextPreview: "TICKET DE CAISSE\nCB ****1234\nTotal 5,00",
      },
      confidence: 0.7,
      status: "pending_review",
      entryId: null,
      createdAt: "2026-03-16T10:00:00Z",
      confirmedAt: null,
    },
    {
      id: 3,
      docType: "bank_statement",
      sourceFileName: "releve-mars.pdf",
      storedPath: "/preview/imports/releve-mars.pdf",
      mimeType: "application/pdf",
      extractionMethod: "pdf_text",
      extraction: {
        issueDate: "2026-03-31",
        invoiceNumber: "REL-03",
        htCents: 15_000,
        vatMode: "sans",
        vatCode: "TEX",
        htAccount: "471",
        counterpartAccount: "512",
        journalCode: "BQ",
        bankDirection: "debit",
        postingSupported: true,
        notes:
          "Relevé : saisissez un mouvement (montant + sens). Multi-lignes auto = suite.",
        rawTextPreview: "RELEVÉ DE COMPTE\nIBAN FR76…\nNouveau solde",
      },
      confidence: 0.78,
      status: "pending_review",
      entryId: null,
      createdAt: "2026-03-31T10:00:00Z",
      confirmedAt: null,
    },
  ];
}

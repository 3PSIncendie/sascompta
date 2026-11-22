import { describe, expect, it } from "vitest";
import {
  applyDocTypeDefaults,
  buildProposedLines,
  confidenceLabel,
  confirmSupported,
  DOC_TYPE_REGISTRY,
  docTypeMeta,
  formFromImport,
  toConfirmInput,
  type DocumentImport,
} from "./documentImport";

const sample: DocumentImport = {
  id: 7,
  docType: "supplier_invoice",
  sourceFileName: "fa.pdf",
  storedPath: "/tmp/fa.pdf",
  mimeType: "application/pdf",
  extractionMethod: "pdf_text",
  extraction: {
    issueDate: "2026-03-15",
    invoiceNumber: "FA-42",
    supplierName: "Delta SAS",
    htCents: 10_000,
    vatCents: 2_000,
    rateBp: 2000,
    vatMode: "normale",
    vatCode: "T20D",
    htAccount: "606",
    journalCode: "ACH",
    matchedThirdPartyId: 3,
  },
  confidence: 0.8,
  status: "pending_review",
  entryId: null,
  createdAt: "2026-03-15T10:00:00Z",
  confirmedAt: null,
};

describe("documentImport registry", () => {
  it("exposes posting for practical document types", () => {
    expect(DOC_TYPE_REGISTRY.length).toBeGreaterThanOrEqual(6);
    expect(docTypeMeta("supplier_invoice").postingSupported).toBe(true);
    expect(docTypeMeta("customer_invoice").postingSupported).toBe(true);
    expect(docTypeMeta("customer_quote").postingSupported).toBe(false);
    expect(docTypeMeta("customer_quote").createsQuote).toBe(true);
    expect(confirmSupported(docTypeMeta("customer_quote"))).toBe(true);
    expect(docTypeMeta("customer_order").postingSupported).toBe(false);
    expect(docTypeMeta("customer_order").createsOrder).toBe(true);
    expect(confirmSupported(docTypeMeta("customer_order"))).toBe(true);
    expect(docTypeMeta("expense_receipt").postingSupported).toBe(true);
    expect(docTypeMeta("bank_statement").postingSupported).toBe(true);
    expect(docTypeMeta("ticket").postingSupported).toBe(true);
    expect(docTypeMeta("unknown").postingSupported).toBe(false);
    expect(confirmSupported(docTypeMeta("unknown"))).toBe(false);
  });

  it("maps devis extraction into review form without ledger lines", () => {
    const devis: DocumentImport = {
      ...sample,
      docType: "customer_quote",
      extraction: {
        issueDate: "2024-06-01",
        invoiceNumber: "D-2024-0012",
        supplierName: "ACME SAS",
        htCents: 120_000,
        vatCents: 24_000,
        rateBp: 2000,
        vatMode: "normale",
        vatCode: "T20C",
        htAccount: "706",
        journalCode: "DEV",
      },
    };
    const form = formFromImport(devis);
    expect(form.docType).toBe("customer_quote");
    expect(form.pieceRef).toBe("D-2024-0012");
    expect(form.journalCode).toBe("DEV");
    expect(form.vatCode).toBe("T20C");
    expect(buildProposedLines(form)).toEqual([]);
  });

  it("maps commande extraction into review form without ledger lines", () => {
    const order: DocumentImport = {
      ...sample,
      docType: "customer_order",
      extraction: {
        issueDate: "2024-07-01",
        invoiceNumber: "BC-2024-0088",
        supplierName: "ACME SAS",
        htCents: 80_000,
        vatCents: 16_000,
        rateBp: 2000,
        vatMode: "normale",
        vatCode: "T20C",
        htAccount: "706",
        journalCode: "CMD",
      },
    };
    const form = formFromImport(order);
    expect(form.docType).toBe("customer_order");
    expect(form.pieceRef).toBe("BC-2024-0088");
    expect(form.journalCode).toBe("CMD");
    expect(form.vatCode).toBe("T20C");
    expect(buildProposedLines(form)).toEqual([]);
  });

  it("maps extraction into review form", () => {
    const form = formFromImport(sample);
    expect(form.docType).toBe("supplier_invoice");
    expect(form.pieceRef).toBe("FA-42");
    expect(form.htStr).toBe("100,00");
    expect(form.thirdPartyId).toBe(3);
    expect(form.journalCode).toBe("ACH");
  });

  it("builds balanced ACH lines via guided template", () => {
    const form = formFromImport(sample);
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(lines.length).toBe(3);
    expect(d).toBe(c);
    expect(d).toBe(12_000);
    expect(lines.some((l) => l.accountNumber === "44566")).toBe(true);
    expect(lines.some((l) => l.accountNumber === "401")).toBe(true);
  });

  it("builds reverse-charge ACH template (604 + 44566 + 4452 + 401 HT)", () => {
    const form = {
      ...formFromImport(sample),
      vatMode: "autoliquidation_sous_traitance" as const,
      vatCode: "T20R",
      htAccount: "604",
      vatStr: "",
    };
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(lines.some((l) => l.accountNumber === "4452")).toBe(true);
    expect(lines.find((l) => l.accountNumber === "401")?.creditCents).toBe(
      10_000,
    );
  });

  it("builds VTE lines for customer invoice", () => {
    const form = applyDocTypeDefaults(formFromImport(sample), "customer_invoice");
    form.htStr = "100,00";
    form.vatMode = "normale";
    form.vatCode = "T20C";
    form.rateBp = 2000;
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(lines.some((l) => l.accountNumber === "411")).toBe(true);
    expect(lines.some((l) => l.accountNumber === "44571")).toBe(true);
    expect(form.journalCode).toBe("VTE");
  });

  it("maps fixture F-2026-1132 (3PSI) to VTE + autoliquidation chantier HT=TTC", () => {
    const fixture: DocumentImport = {
      ...sample,
      docType: "customer_invoice",
      sourceFileName: "2611-F-2026-1132-3PSI.pdf",
      extraction: {
        issueDate: "2026-11-13",
        invoiceNumber: "F-2026-1132",
        supplierName: "ATLANTIQUE AUTOMATISME INCENDIE",
        htCents: 150_000,
        vatCents: 0,
        ttcCents: 150_000,
        rateBp: 0,
        vatMode: "autoliquidation_chantier",
        vatCode: "T20C",
        htAccount: "706",
        counterpartAccount: "411",
        journalCode: "VTE",
        reverseCharge: true,
      },
      confidence: 0.85,
    };
    const form = formFromImport(fixture);
    expect(form.docType).toBe("customer_invoice");
    expect(form.journalCode).toBe("VTE");
    expect(form.htStr).toBe("1500,00");
    expect(form.vatStr).toBe("0,00");
    expect(form.vatMode).toBe("autoliquidation_chantier");
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(d).toBe(150_000);
    expect(lines.some((l) => l.accountNumber === "44571")).toBe(false);
    expect(lines.some((l) => l.accountNumber === "411")).toBe(true);
    expect(lines.some((l) => l.accountNumber === "706")).toBe(true);
  });

  it("builds OD ticket lines with caisse counterpart", () => {
    const form = applyDocTypeDefaults(formFromImport(sample), "ticket");
    form.htStr = "10,00";
    form.vatMode = "normale";
    form.rateBp = 2000;
    form.counterpartAccount = "530";
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(lines.some((l) => l.accountNumber === "530")).toBe(true);
    expect(lines.some((l) => l.accountNumber === "625")).toBe(true);
  });

  it("builds BQ bank movement lines", () => {
    const form = applyDocTypeDefaults(formFromImport(sample), "bank_statement");
    form.htStr = "150,00";
    form.htAccount = "471";
    form.counterpartAccount = "512";
    form.bankDirection = "debit";
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(d).toBe(15_000);
    expect(lines.some((l) => l.accountNumber === "512")).toBe(true);
  });

  it("serializes confirm payload with counterpart", () => {
    const form = formFromImport(sample);
    const input = toConfirmInput(form, 10_000, 2_000);
    expect(input.docType).toBe("supplier_invoice");
    expect(input.htCents).toBe(10_000);
    expect(input.vatCents).toBe(2_000);
    expect(input.thirdPartyId).toBe(3);
    expect(input.counterpartAccount).toBeTruthy();
    expect(input.isCreditNote).toBe(false);
    expect(form.isAvoir).toBe(false);
  });

  it("does not auto-check avoir from PDF hint (manual selection)", () => {
    const hinted: DocumentImport = {
      ...sample,
      extraction: { ...sample.extraction, creditNoteHint: true },
    };
    expect(formFromImport(hinted).isAvoir).toBe(false);
    const confirmed: DocumentImport = {
      ...sample,
      status: "confirmed",
      extraction: {
        ...sample.extraction,
        creditNoteHint: true,
        confirmedIsCreditNote: true,
      },
    };
    expect(formFromImport(confirmed).isAvoir).toBe(true);
  });

  it("reverses ACH lines when avoir is selected", () => {
    const form = { ...formFromImport(sample), isAvoir: true };
    const lines = buildProposedLines(form);
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    expect(d).toBe(c);
    expect(lines.find((l) => l.accountNumber === "401")?.debitCents).toBe(
      12_000,
    );
    expect(lines.find((l) => l.accountNumber === "606")?.creditCents).toBe(
      10_000,
    );
    expect(toConfirmInput(form, 10_000, 2_000).isCreditNote).toBe(true);
  });

  it("reverses VTE lines when customer avoir is selected", () => {
    const form = {
      ...applyDocTypeDefaults(formFromImport(sample), "customer_invoice"),
      htStr: "100,00",
      vatMode: "normale" as const,
      vatCode: "T20C",
      rateBp: 2000,
      isAvoir: true,
    };
    const lines = buildProposedLines(form);
    expect(lines.find((l) => l.accountNumber === "411")?.creditCents).toBe(
      12_000,
    );
    expect(lines.find((l) => l.accountNumber === "706")?.debitCents).toBe(
      10_000,
    );
  });

  it("labels confidence for beginners", () => {
    expect(confidenceLabel(0.9)).toMatch(/Élevée/);
    expect(confidenceLabel(0.5)).toMatch(/Moyenne/);
    expect(confidenceLabel(0.2)).toMatch(/Faible/);
  });
});

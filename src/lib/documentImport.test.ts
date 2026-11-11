import { describe, expect, it } from "vitest";
import {
  applyDocTypeDefaults,
  buildProposedLines,
  confidenceLabel,
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
    expect(DOC_TYPE_REGISTRY.length).toBeGreaterThanOrEqual(5);
    expect(docTypeMeta("supplier_invoice").postingSupported).toBe(true);
    expect(docTypeMeta("customer_invoice").postingSupported).toBe(true);
    expect(docTypeMeta("expense_receipt").postingSupported).toBe(true);
    expect(docTypeMeta("bank_statement").postingSupported).toBe(true);
    expect(docTypeMeta("ticket").postingSupported).toBe(true);
    expect(docTypeMeta("unknown").postingSupported).toBe(false);
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
  });

  it("labels confidence for beginners", () => {
    expect(confidenceLabel(0.9)).toMatch(/Élevée/);
    expect(confidenceLabel(0.5)).toMatch(/Moyenne/);
    expect(confidenceLabel(0.2)).toMatch(/Faible/);
  });
});

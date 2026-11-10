import { describe, expect, it } from "vitest";
import {
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
  it("exposes extensible doc types with only supplier posting in lot 1", () => {
    expect(DOC_TYPE_REGISTRY.length).toBeGreaterThanOrEqual(5);
    expect(docTypeMeta("supplier_invoice").postingSupported).toBe(true);
    expect(docTypeMeta("bank_statement").postingSupported).toBe(false);
    expect(docTypeMeta("ticket").comingSoonHint).toBeTruthy();
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

  it("serializes confirm payload", () => {
    const form = formFromImport(sample);
    const input = toConfirmInput(form, 10_000, 2_000);
    expect(input.docType).toBe("supplier_invoice");
    expect(input.htCents).toBe(10_000);
    expect(input.vatCents).toBe(2_000);
    expect(input.thirdPartyId).toBe(3);
  });

  it("labels confidence for beginners", () => {
    expect(confidenceLabel(0.9)).toMatch(/Élevée/);
    expect(confidenceLabel(0.5)).toMatch(/Moyenne/);
    expect(confidenceLabel(0.2)).toMatch(/Faible/);
  });
});

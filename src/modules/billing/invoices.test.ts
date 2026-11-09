import { describe, expect, it } from "vitest";
import {
  buildInvoiceCreateInput,
  defaultAccountForKind,
  emptyDraftLine,
  kindLabel,
  statusLabel,
} from "./invoices";

describe("billing invoices helpers", () => {
  it("labels FR", () => {
    expect(kindLabel("deposit")).toBe("Acompte");
    expect(kindLabel("credit_note")).toBe("Avoir");
    expect(statusLabel("issued")).toBe("Émise");
  });

  it("compte 4191 pour acompte", () => {
    expect(defaultAccountForKind("deposit")).toBe("4191");
    expect(defaultAccountForKind("standard")).toBe("706");
  });

  it("buildInvoiceCreateInput", () => {
    const line = emptyDraftLine(2000, "T20C");
    line.label = "Prestation";
    line.quantity = "1";
    line.unitPriceEuros = "100";
    const input = buildInvoiceCreateInput({
      kind: "standard",
      thirdPartyId: 3,
      issueDate: "2026-04-01",
      dueDate: "2026-04-30",
      serviceDate: "",
      vatMode: "normale",
      notes: "",
      lines: [line],
      parseEuros: (s) => Math.round(Number(s.replace(",", ".")) * 100),
    });
    expect(input.kind).toBe("standard");
    expect(input.lines[0].htCents).toBe(10_000);
    expect(input.dueDate).toBe("2026-04-30");
  });
});

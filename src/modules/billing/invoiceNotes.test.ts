import { describe, expect, it } from "vitest";
import {
  initialInvoiceNotesMode,
  permanentInvoiceNotes,
} from "./invoiceNotes";

describe("invoiceNotes", () => {
  it("lit la note permanente société", () => {
    expect(permanentInvoiceNotes(null)).toBe("");
    expect(
      permanentInvoiceNotes({ invoiceBillingNotes: "  Paiement 30 j.  " }),
    ).toBe("Paiement 30 j.");
  });

  it("choisit le mode initial selon l’égalité avec le modèle", () => {
    expect(initialInvoiceNotesMode("", "Paiement 30 j.")).toBe("permanent");
    expect(initialInvoiceNotesMode("Paiement 30 j.", "Paiement 30 j.")).toBe(
      "permanent",
    );
    expect(
      initialInvoiceNotesMode("Remise exceptionnelle", "Paiement 30 j."),
    ).toBe("once");
  });
});

import { describe, expect, it } from "vitest";
import type { Organization, ThirdParty } from "../../lib/types";
import { emptyDraftLine } from "./quotes";
import {
  buildBillingDocumentPreview,
  cityFromAddress,
  commercialFooterLeft,
  draftPreviewNumber,
  formatDateFr,
  formatQtyFr,
  formatVatPctFr,
  invoiceTitleForKind,
  spacedSiren,
} from "./documentPreview";

const org: Organization = {
  id: 1,
  name: "Demo SAS",
  siren: "123456782",
  siret: "12345678200002",
  legalForm: "SAS",
  vatRegime: "reel_mensuel",
  address: "12 rue des Lilas\n75011 Paris",
  capitalCents: 100_000,
  email: "contact@demo-sas.fr",
  iban: "FR7616958000014398156414622",
  bic: "QNTOFRP1XXX",
  invoiceBillingNotes: null,
};

const client: ThirdParty = {
  id: 3,
  code: "C001",
  name: "Client Gamma",
  kind: "customer",
  accountNumber: "411",
  active: true,
  siret: "12345678200045",
  email: "contact@gamma.fr",
  address: "8 avenue du Port\n33000 Bordeaux",
};

describe("documentPreview helpers", () => {
  it("formate dates / qté / TVA comme le PDF", () => {
    expect(formatDateFr("2026-03-15")).toBe("15/03/2026");
    expect(formatQtyFr(1000)).toBe("1");
    expect(formatQtyFr(1500)).toBe("1,5");
    expect(formatQtyFr(2250)).toBe("2,25");
    expect(formatVatPctFr(2000)).toBe("20 %");
    expect(formatVatPctFr(550)).toBe("5,50 %");
  });

  it("compose le pied identité", () => {
    expect(spacedSiren("123456782")).toBe("123 456 782");
    expect(cityFromAddress(org.address)).toBe("Paris");
    expect(commercialFooterLeft(org)).toBe(
      "Demo SAS, SAS · Paris 123 456 782",
    );
  });

  it("titre facture selon le kind", () => {
    expect(invoiceTitleForKind("standard")).toBe("Facture");
    expect(invoiceTitleForKind("deposit")).toBe("Facture d'acompte");
    expect(invoiceTitleForKind("credit_note")).toBe("Avoir");
  });

  it("numéro provisoire brouillon", () => {
    expect(draftPreviewNumber("DEV", "2026-03-15", "2026-03-0001")).toBe(
      "2026-03-0001",
    );
    expect(draftPreviewNumber("FACT", "2026-04-01", null)).toBe(
      "BROUILLON-FACT-2026-04",
    );
  });

  it("construit un aperçu devis à partir du brouillon", () => {
    const line = emptyDraftLine(2000, "T20C");
    line.label = "Centrale détection";
    line.description = "Fourniture\nMise en service";
    line.quantity = "1";
    line.unitPriceEuros = "2500,00";
    const model = buildBillingDocumentPreview({
      docKind: "quote",
      title: "Devis",
      number: "BROUILLON-DEV-2026-03",
      issueDate: "2026-03-01",
      secondaryDate: "2026-03-31",
      secondaryDateLabel: "Date d'expiration",
      org,
      client,
      vatMode: "normale",
      notes: "Atelier",
      lines: [line],
    });
    expect(model.title).toBe("Devis");
    expect(model.meta[0]?.value).toBe("BROUILLON-DEV-2026-03");
    expect(model.meta[1]?.value).toBe("01/03/2026");
    expect(model.meta[2]?.label).toBe("Date d'expiration");
    expect(model.client.name).toBe("Client Gamma");
    expect(model.client.email).toBe("contact@gamma.fr");
    expect(model.lines).toHaveLength(1);
    expect(model.lines[0]?.description).toContain("Mise en service");
    expect(model.htLabel).toMatch(/2\u00a0500,00/);
    expect(model.vatLabel).toMatch(/500,00/);
    expect(model.ttcLabel).toMatch(/3\u00a0000,00/);
    expect(model.notes).toBe("Atelier");
    expect(model.footerLeft).toContain("Demo SAS");
    expect(model.org.email).toBe("contact@demo-sas.fr");
  });

  it("injecte les mentions légales BTP (sans bandeau paiement)", () => {
    const line = emptyDraftLine(2000, "T20R");
    line.label = "Travaux";
    line.unitPriceEuros = "100";
    const model = buildBillingDocumentPreview({
      docKind: "invoice",
      title: "Facture",
      number: "BROUILLON-FACT-2026-04",
      issueDate: "2026-04-01",
      secondaryDate: "",
      secondaryDateLabel: "Date d'échéance",
      org,
      client: null,
      vatMode: "autoliquidation_chantier",
      notes: "Paiement à 30 jours",
      lines: [line],
    });
    expect(model.client.name).toMatch(/choisir/);
    expect(model.legalMentions).toMatch(/283/);
    expect(model.vatLabel).toMatch(/0,00/);
    expect(model.notes).toBe("Paiement à 30 jours");
  });
});

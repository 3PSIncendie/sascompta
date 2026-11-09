import { describe, expect, it } from "vitest";
import {
  suggestThirdPartyCode,
  toCreateThirdPartyInput,
  validateCreateThirdParty,
} from "./thirdParty";

describe("thirdParty helpers", () => {
  it("suggère le prochain code client / fournisseur", () => {
    expect(suggestThirdPartyCode([], "customer")).toBe("C001");
    expect(
      suggestThirdPartyCode(
        [
          { code: "C001", kind: "customer" },
          { code: "C002", kind: "customer" },
          { code: "F001", kind: "supplier" },
        ],
        "customer",
      ),
    ).toBe("C003");
    expect(
      suggestThirdPartyCode([{ code: "F009", kind: "supplier" }], "supplier"),
    ).toBe("F010");
  });

  it("valide SIREN/SIRET et e-mail", () => {
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "Acme",
        kind: "customer",
        siret: "",
        email: "",
      }),
    ).toBeNull();
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "Acme",
        kind: "customer",
        siret: "123 456 782",
        email: "",
      }),
    ).toBeNull();
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "Acme",
        kind: "customer",
        siret: "12345678200045",
        email: "a@b.fr",
      }),
    ).toBeNull();
    expect(
      validateCreateThirdParty({
        code: "",
        name: "Acme",
        kind: "customer",
        siret: "",
        email: "",
      }),
    ).toMatch(/code/i);
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "A",
        kind: "customer",
        siret: "",
        email: "",
      }),
    ).toMatch(/raison sociale/i);
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "Acme",
        kind: "customer",
        siret: "12345",
        email: "",
      }),
    ).toMatch(/SIREN|SIRET/);
    expect(
      validateCreateThirdParty({
        code: "C003",
        name: "Acme",
        kind: "customer",
        siret: "",
        email: "pas-un-mail",
      }),
    ).toMatch(/e-mail/i);
  });

  it("normalise l’entrée API", () => {
    expect(
      toCreateThirdPartyInput({
        code: " C003 ",
        name: "  Gamma SAS ",
        kind: "customer",
        siret: "123 456 782 00045",
        email: " contact@gamma.fr ",
        address: " 12 rue du Port, 75001 Paris ",
        routingId: " ",
      }),
    ).toEqual({
      code: "C003",
      name: "Gamma SAS",
      kind: "customer",
      siret: "12345678200045",
      email: "contact@gamma.fr",
      address: "12 rue du Port, 75001 Paris",
      routingId: null,
    });
  });
});

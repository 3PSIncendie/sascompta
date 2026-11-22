import { describe, expect, it } from "vitest";
import {
  DEFAULT_FACTORING_FEE_ACCOUNT,
  factoringNetCents,
  isFactoringMethod,
  methodLabel,
  normalizeBankRef,
  reminderLevelLabel,
} from "./payments";

describe("payments helpers", () => {
  it("labels modes et niveaux", () => {
    expect(methodLabel("virement")).toBe("Virement");
    expect(methodLabel("affacturage")).toBe("Affacturage");
    expect(methodLabel("inconnu")).toBe("inconnu");
    expect(reminderLevelLabel(15)).toBe("J+15");
  });

  it("normalizeBankRef laisse le n° de virement optionnel", () => {
    expect(normalizeBankRef(undefined)).toBeNull();
    expect(normalizeBankRef(null)).toBeNull();
    expect(normalizeBankRef("")).toBeNull();
    expect(normalizeBankRef("   ")).toBeNull();
    expect(normalizeBankRef("VIR-42")).toBe("VIR-42");
    expect(normalizeBankRef("  VIR-42  ")).toBe("VIR-42");
  });

  it("calcule le net affacturage", () => {
    expect(factoringNetCents(12_000, 360)).toBe(11_640);
    expect(factoringNetCents(100, 100)).toBe(0);
    expect(DEFAULT_FACTORING_FEE_ACCOUNT).toBe("622");
    expect(isFactoringMethod("affacturage")).toBe(true);
    expect(isFactoringMethod("virement")).toBe(false);
  });
});

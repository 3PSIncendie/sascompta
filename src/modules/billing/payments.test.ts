import { describe, expect, it } from "vitest";
import {
  methodLabel,
  normalizeBankRef,
  reminderLevelLabel,
} from "./payments";

describe("payments helpers", () => {
  it("labels modes et niveaux", () => {
    expect(methodLabel("virement")).toBe("Virement");
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
});

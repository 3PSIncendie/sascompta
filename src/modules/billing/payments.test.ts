import { describe, expect, it } from "vitest";
import { methodLabel, reminderLevelLabel } from "./payments";

describe("payments helpers", () => {
  it("labels modes et niveaux", () => {
    expect(methodLabel("virement")).toBe("Virement");
    expect(methodLabel("inconnu")).toBe("inconnu");
    expect(reminderLevelLabel(15)).toBe("J+15");
  });
});

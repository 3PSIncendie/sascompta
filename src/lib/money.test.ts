import { describe, expect, it } from "vitest";
import { formatEuros, parseEurosToCents, formatRateBp } from "./money";

describe("money", () => {
  it("formats cents", () => {
    expect(formatEuros(260000)).toContain("2");
    expect(formatEuros(260000)).toContain("600,00");
    expect(formatEuros(0)).toBe("0,00\u00a0€");
  });

  it("parses euros", () => {
    expect(parseEurosToCents("12,50")).toBe(1250);
    expect(parseEurosToCents("1000")).toBe(100000);
  });

  it("formats rate bp", () => {
    expect(formatRateBp(2000)).toBe("20,00\u00a0%");
    expect(formatRateBp(550)).toBe("5,50\u00a0%");
  });
});

import { describe, expect, it } from "vitest";
import {
  computePayslipAmounts,
  formatDaysBp,
  formatHoursBp,
  parseDaysToBp,
  parseHoursToBp,
  parseRateToBp,
} from "./format";

describe("hr format helpers", () => {
  it("formats and parses days bp", () => {
    expect(formatDaysBp(2500)).toBe("25\u00a0j");
    expect(formatDaysBp(250)).toBe("2,50\u00a0j");
    expect(parseDaysToBp("5")).toBe(500);
    expect(parseDaysToBp("2,5")).toBe(250);
  });

  it("formats and parses hours bp", () => {
    expect(formatHoursBp(3500)).toBe("35\u00a0h");
    expect(parseHoursToBp("35")).toBe(3500);
    expect(parseHoursToBp("37,5")).toBe(3750);
  });

  it("mirrors backend payslip math", () => {
    const r = computePayslipAmounts(280_000, 2200, 4200, 0);
    expect(r.employeeChargesCents).toBe(61_600);
    expect(r.employerChargesCents).toBe(117_600);
    expect(r.netCents).toBe(218_400);
    expect(parseRateToBp("22")).toBe(2200);
  });
});

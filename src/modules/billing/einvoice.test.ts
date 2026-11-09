import { describe, expect, it } from "vitest";
import { phaseLabel, queueStatusLabel } from "./einvoice";

describe("einvoice helpers", () => {
  it("labels phases et statuts", () => {
    expect(phaseLabel("receive_only")).toMatch(/Réception/);
    expect(queueStatusLabel("acked")).toBe("Ack stub");
    expect(queueStatusLabel("custom")).toBe("custom");
  });
});

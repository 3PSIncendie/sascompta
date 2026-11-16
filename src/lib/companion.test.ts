import { describe, expect, it } from "vitest";
import {
  companionEndpoint,
  pairingExpiresLabel,
  previewCompanionStatus,
} from "./companion";

describe("companion helpers", () => {
  it("preview idle vs running", () => {
    expect(previewCompanionStatus(false).running).toBe(false);
    const live = previewCompanionStatus(true);
    expect(live.running).toBe(true);
    expect(live.pairingCode).toHaveLength(6);
    expect(live.qrPayload).toContain("sascompta");
  });

  it("formats endpoint", () => {
    expect(companionEndpoint("192.168.1.10", 17832)).toBe(
      "http://192.168.1.10:17832",
    );
    expect(companionEndpoint(null, 17832)).toBeNull();
  });

  it("pairing expiry label", () => {
    expect(pairingExpiresLabel(null)).toBe("—");
    const soon = new Date(Date.now() + 90_000).toISOString();
    expect(pairingExpiresLabel(soon)).toMatch(/min|s/);
  });
});

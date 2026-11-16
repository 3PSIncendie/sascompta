import { describe, expect, it } from "vitest";
import {
  buildPairingPayload,
  companionEndpoint,
  pairingExpiresLabel,
  pairingIsExpired,
  parsePairingPayload,
  previewCompanionStatus,
  svgForInnerHtml,
} from "./companion";

describe("companion helpers", () => {
  it("preview idle vs running", () => {
    expect(previewCompanionStatus(false).running).toBe(false);
    const live = previewCompanionStatus(true);
    expect(live.running).toBe(true);
    expect(live.pairingCode).toHaveLength(6);
    expect(live.qrPayload).toContain("sascompta://pair");
    expect(live.qrPayload).toContain("host=192.168.1.42");
    expect(live.qrPayload).toContain("port=17832");
    expect(live.qrPayload).toContain("code=482913");
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
    expect(pairingIsExpired(new Date(Date.now() - 1000).toISOString())).toBe(
      true,
    );
  });

  it("round-trips URI pairing payload", () => {
    const raw = buildPairingPayload({
      host: "10.0.0.5",
      port: 17832,
      code: "001234",
    });
    const parsed = parsePairingPayload(raw);
    expect(parsed).toEqual({
      v: 1,
      app: "sascompta",
      host: "10.0.0.5",
      port: 17832,
      code: "001234",
    });
  });

  it("parses legacy JSON QR payload", () => {
    const raw = JSON.stringify({
      v: 1,
      app: "sascompta",
      host: "192.168.1.42",
      port: 17832,
      code: "482913",
      org: "Demo SAS",
    });
    expect(parsePairingPayload(raw)).toMatchObject({
      host: "192.168.1.42",
      port: 17832,
      code: "482913",
    });
  });

  it("rejects incomplete payloads", () => {
    expect(() => parsePairingPayload("{}")).toThrow(/invalide/i);
    expect(() => parsePairingPayload("not-a-qr")).toThrow(/invalide/i);
  });

  it("strips XML prologue from QR SVG", () => {
    const raw =
      '<?xml version="1.0" standalone="yes"?><svg xmlns="http://www.w3.org/2000/svg"></svg>';
    expect(svgForInnerHtml(raw)).toBe(
      '<svg xmlns="http://www.w3.org/2000/svg"></svg>',
    );
    expect(svgForInnerHtml(null)).toBeNull();
  });
});

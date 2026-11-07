import { describe, expect, it } from "vitest";
import { formatUpdaterCheckError } from "./updater";

describe("formatUpdaterCheckError", () => {
  it("maps missing / private release JSON to an actionable French message", () => {
    const msg = formatUpdaterCheckError(
      new Error("Could not fetch a valid release JSON from the remote"),
    );
    expect(msg).toMatch(/release publique/i);
    expect(msg).toMatch(/latest\.json/);
    expect(msg).toMatch(/public/);
  });

  it("maps HTTP 404 style messages the same way", () => {
    const msg = formatUpdaterCheckError(new Error("request failed with status 404"));
    expect(msg).toMatch(/latest\.json/);
  });

  it("maps network failures", () => {
    const msg = formatUpdaterCheckError(new Error("error sending request for url"));
    expect(msg).toMatch(/réseau/i);
  });

  it("falls back to a generic French message", () => {
    expect(formatUpdaterCheckError(null)).toBe(
      "Impossible de vérifier les mises à jour.",
    );
  });

  it("preserves unknown error details", () => {
    const msg = formatUpdaterCheckError(new Error("signature invalid"));
    expect(msg).toContain("signature invalid");
  });

  it("is idempotent for already-mapped messages", () => {
    const once = formatUpdaterCheckError(
      new Error("Could not fetch a valid release JSON from the remote"),
    );
    expect(formatUpdaterCheckError(new Error(once))).toBe(once);
  });
});

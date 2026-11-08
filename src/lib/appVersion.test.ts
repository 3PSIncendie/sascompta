import { describe, expect, it } from "vitest";
import { PACKAGE_VERSION } from "./appVersion";
import packageJson from "../../package.json";

describe("PACKAGE_VERSION", () => {
  it("matches package.json", () => {
    expect(PACKAGE_VERSION).toBe(packageJson.version);
    expect(PACKAGE_VERSION).toMatch(/^\d+\.\d+\.\d+/);
  });
});

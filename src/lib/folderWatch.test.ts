import { describe, expect, it } from "vitest";
import {
  folderWatchPathLabel,
  previewFolderWatchStatus,
} from "./folderWatch";

describe("folderWatch helpers", () => {
  it("preview idle vs live", () => {
    const idle = previewFolderWatchStatus(false);
    expect(idle.running).toBe(false);
    expect(idle.defaultPath).toContain("SasCompta-import");
    const live = previewFolderWatchStatus(true);
    expect(live.running).toBe(true);
    expect(live.receivedCount).toBeGreaterThan(0);
  });

  it("path label falls back", () => {
    expect(folderWatchPathLabel(null, "~/Documents/SasCompta-import")).toBe(
      "~/Documents/SasCompta-import",
    );
    expect(folderWatchPathLabel("  ")).toBe("—");
    expect(folderWatchPathLabel("/tmp/watch")).toBe("/tmp/watch");
  });
});

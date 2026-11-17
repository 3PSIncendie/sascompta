import { describe, expect, it } from "vitest";
import {
  folderWatchPathLabel,
  folderWatchPathsSummary,
  mergeWatchPath,
  previewFolderWatchStatus,
} from "./folderWatch";

describe("folderWatch helpers", () => {
  it("preview idle vs live (multi)", () => {
    const idle = previewFolderWatchStatus(false);
    expect(idle.running).toBe(false);
    expect(idle.defaultPath).toContain("SasCompta-import");
    expect(idle.paths.length).toBeGreaterThanOrEqual(1);
    expect(idle.path).toBe(idle.paths[0]);
    const live = previewFolderWatchStatus(true);
    expect(live.running).toBe(true);
    expect(live.paths.length).toBeGreaterThanOrEqual(2);
    expect(live.receivedCount).toBeGreaterThan(0);
  });

  it("path label falls back", () => {
    expect(folderWatchPathLabel(null, "~/Documents/SasCompta-import")).toBe(
      "~/Documents/SasCompta-import",
    );
    expect(folderWatchPathLabel("  ")).toBe("—");
    expect(folderWatchPathLabel("/tmp/watch")).toBe("/tmp/watch");
  });

  it("paths summary", () => {
    expect(folderWatchPathsSummary([])).toBe("—");
    expect(folderWatchPathsSummary([], "~/Documents/SasCompta-import")).toBe(
      "~/Documents/SasCompta-import",
    );
    expect(folderWatchPathsSummary(["/a"])).toBe("/a");
    expect(folderWatchPathsSummary(["/a", "/b"])).toBe("2 dossiers");
  });

  it("mergeWatchPath dedupes", () => {
    expect(mergeWatchPath(["/a"], "/b")).toEqual(["/a", "/b"]);
    expect(mergeWatchPath(["/a"], "/a")).toEqual(["/a"]);
    expect(mergeWatchPath(["/a"], "  ")).toEqual(["/a"]);
  });
});

import { check, type Update } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

/** True when running inside the Tauri webview (not browser preview). */
export function isTauriRuntime(): boolean {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export type UpdateStatus =
  | { kind: "idle" }
  | { kind: "checking" }
  | { kind: "up-to-date" }
  | { kind: "available"; update: Update }
  | { kind: "downloading"; version: string; progress: number | null }
  | { kind: "installing"; version: string }
  | { kind: "error"; message: string };

export async function checkForAppUpdate(): Promise<Update | null> {
  if (!isTauriRuntime()) return null;
  return check();
}

export async function downloadAndInstallUpdate(
  update: Update,
  onProgress?: (ratio: number | null) => void,
): Promise<void> {
  let downloaded = 0;
  let contentLength: number | null = null;

  await update.downloadAndInstall((event) => {
    switch (event.event) {
      case "Started":
        contentLength = event.data.contentLength ?? null;
        onProgress?.(contentLength && contentLength > 0 ? 0 : null);
        break;
      case "Progress":
        downloaded += event.data.chunkLength;
        if (contentLength && contentLength > 0) {
          onProgress?.(Math.min(1, downloaded / contentLength));
        } else {
          onProgress?.(null);
        }
        break;
      case "Finished":
        onProgress?.(1);
        break;
    }
  });

  // Windows exits during install; relaunch is for macOS/Linux.
  await relaunch();
}

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

/** GitHub Releases endpoint used by `tauri.conf.json` (must be publicly reachable). */
export const UPDATER_LATEST_JSON_URL =
  "https://github.com/3PSIncendie/sascompta/releases/latest/download/latest.json";

/**
 * Map raw Tauri updater / network errors to an actionable French message.
 * GitHub returns 404 for both missing releases and private repos without auth;
 * the desktop updater cannot authenticate, so both need a public `latest.json`.
 */
export function formatUpdaterCheckError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err ?? "");
  // Already mapped (e.g. re-thrown from checkForAppUpdate).
  if (raw.startsWith("Impossible de vérifier les mises à jour")) {
    return raw;
  }
  const lower = raw.toLowerCase();

  if (
    lower.includes("could not fetch a valid release json") ||
    lower.includes("release not found") ||
    /\b404\b/.test(lower) ||
    lower.includes("not found")
  ) {
    return (
      "Impossible de vérifier les mises à jour : aucune release publique " +
      "(fichier latest.json). Publiez un tag v* et rendez le dépôt GitHub public " +
      "(les assets de release privés renvoient 404 sans authentification)."
    );
  }

  if (
    lower.includes("error sending request") ||
    lower.includes("error trying to connect") ||
    lower.includes("timed out") ||
    lower.includes("timeout") ||
    lower.includes("dns") ||
    lower.includes("network") ||
    lower.includes("connection")
  ) {
    return "Impossible de vérifier les mises à jour : problème réseau. Réessayez plus tard.";
  }

  if (raw.trim()) {
    return `Impossible de vérifier les mises à jour : ${raw.trim()}`;
  }

  return "Impossible de vérifier les mises à jour.";
}

export async function checkForAppUpdate(): Promise<Update | null> {
  if (!isTauriRuntime()) return null;
  try {
    return await check();
  } catch (err) {
    throw new Error(formatUpdaterCheckError(err), { cause: err });
  }
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

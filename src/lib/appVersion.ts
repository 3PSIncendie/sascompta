import { getVersion } from "@tauri-apps/api/app";
import packageJson from "../../package.json";
import { isTauriRuntime } from "./updater";

/** Version from package.json (aligned with tauri.conf) for web preview. */
export const PACKAGE_VERSION: string = packageJson.version;

/**
 * Prefer the running Tauri app version; fall back to package.json in browser preview.
 */
export async function resolveAppVersion(): Promise<string> {
  if (isTauriRuntime()) {
    try {
      return await getVersion();
    } catch {
      // Webview without app API — use package fallback.
    }
  }
  return PACKAGE_VERSION;
}

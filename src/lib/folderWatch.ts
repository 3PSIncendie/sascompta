/** Surveillance dossier PC → file Import docs — types + aperçu navigateur. */

export type FolderWatchStatus = {
  running: boolean;
  path: string | null;
  defaultPath: string;
  receivedCount: number;
  skippedCount: number;
  lastError: string | null;
};

export function previewFolderWatchStatus(running = false): FolderWatchStatus {
  const defaultPath = "~/Documents/SasCompta-import";
  if (!running) {
    return {
      running: false,
      path: defaultPath,
      defaultPath,
      receivedCount: 0,
      skippedCount: 0,
      lastError: null,
    };
  }
  return {
    running: true,
    path: defaultPath,
    defaultPath,
    receivedCount: 2,
    skippedCount: 1,
    lastError: null,
  };
}

export function folderWatchPathLabel(
  path: string | null | undefined,
  fallback?: string | null,
): string {
  const p = (path || fallback || "").trim();
  return p || "—";
}

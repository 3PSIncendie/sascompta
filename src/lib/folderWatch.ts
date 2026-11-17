/** Surveillance dossiers PC → file Import docs — types + aperçu navigateur. */

export type FolderWatchStatus = {
  running: boolean;
  /** Chemins surveillés (ou mémorisés si arrêté). */
  paths: string[];
  /** Premier chemin (compat). */
  path: string | null;
  defaultPath: string;
  receivedCount: number;
  skippedCount: number;
  lastError: string | null;
};

export function previewFolderWatchStatus(running = false): FolderWatchStatus {
  const defaultPath = "~/Documents/SasCompta-import";
  const second = "~/Documents/Scans-factures";
  if (!running) {
    return {
      running: false,
      paths: [defaultPath],
      path: defaultPath,
      defaultPath,
      receivedCount: 0,
      skippedCount: 0,
      lastError: null,
    };
  }
  return {
    running: true,
    paths: [defaultPath, second],
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

/** Libellé court pour une liste de dossiers. */
export function folderWatchPathsSummary(
  paths: string[] | null | undefined,
  fallback?: string | null,
): string {
  const list = (paths || []).map((p) => p.trim()).filter(Boolean);
  if (list.length === 0) {
    return folderWatchPathLabel(null, fallback);
  }
  if (list.length === 1) return list[0];
  return `${list.length} dossiers`;
}

/** Fusionne un chemin dans la liste sans doublon (ordre conservé). */
export function mergeWatchPath(
  paths: string[],
  next: string | null | undefined,
): string[] {
  const trimmed = (next || "").trim();
  if (!trimmed) return [...paths];
  if (paths.some((p) => p === trimmed)) return [...paths];
  return [...paths, trimmed];
}

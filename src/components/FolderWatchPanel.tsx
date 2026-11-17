import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FieldHelp } from "./FieldHelp";
import { api } from "../lib/api";
import {
  folderWatchPathLabel,
  mergeWatchPath,
  previewFolderWatchStatus,
  type FolderWatchStatus,
} from "../lib/folderWatch";
import { isTauriRuntime } from "../lib/updater";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");
const PREVIEW_LIVE =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("folderWatch") === "1";

type Props = {
  compact?: boolean;
  onDocumentReceived?: () => void;
};

function pathsFromStatus(s: FolderWatchStatus): string[] {
  if (s.paths?.length) return [...s.paths];
  if (s.path) return [s.path];
  return [];
}

export function FolderWatchPanel({
  compact = false,
  onDocumentReceived,
}: Props) {
  const [status, setStatus] = useState<FolderWatchStatus>(() =>
    previewFolderWatchStatus(PREVIEW && PREVIEW_LIVE),
  );
  const [paths, setPaths] = useState<string[]>(() =>
    pathsFromStatus(previewFolderWatchStatus(PREVIEW && PREVIEW_LIVE)),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);

  const applyStatus = useCallback((s: FolderWatchStatus) => {
    setStatus(s);
    setPaths(pathsFromStatus(s));
  }, []);

  const refresh = useCallback(async () => {
    if (PREVIEW || !isTauriRuntime()) {
      return;
    }
    const s = await api.folderWatchStatus();
    applyStatus(s);
  }, [applyStatus]);

  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  useEffect(() => {
    if (PREVIEW || !isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{
          id: number;
          sourceFileName: string;
          docType: string;
        }>("folder-watch-document-received", (ev) => {
          setFlash(
            `Nouveau fichier : ${ev.payload.sourceFileName} (#${ev.payload.id}) — à revoir avant écriture.`,
          );
          void refresh();
          onDocumentReceived?.();
        }),
      )
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        /* ignore */
      });
    return () => {
      unlisten?.();
    };
  }, [onDocumentReceived, refresh]);

  async function addFolder() {
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const demo =
          paths.length === 0
            ? status.defaultPath || "~/Documents/SasCompta-import"
            : "~/Documents/Scans-factures";
        const next = mergeWatchPath(paths, demo);
        setPaths(next);
        setFlash(`Aperçu — dossier ajouté : ${demo}`);
        return;
      }
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Ajouter un dossier surveillé",
      });
      if (typeof selected !== "string" || !selected) return;
      const s = await api.folderWatchAddPath(selected);
      applyStatus(s);
      setFlash(`Dossier ajouté : ${selected}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function removeFolder(target: string) {
    setError(null);
    setBusy(true);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const next = paths.filter((p) => p !== target);
        setPaths(next);
        if (status.running && next.length === 0) {
          setStatus(previewFolderWatchStatus(false));
        } else if (status.running) {
          setStatus({
            ...status,
            paths: next,
            path: next[0] ?? null,
          });
        }
        setFlash("Dossier retiré de la liste (aperçu).");
        return;
      }
      const s = await api.folderWatchRemovePath(target);
      applyStatus(s);
      if (!s.paths.length) {
        setPaths([]);
      }
      setFlash(
        s.running
          ? "Dossier retiré — surveillance mise à jour."
          : "Dossier retiré de la liste.",
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function start() {
    setBusy(true);
    setError(null);
    setFlash(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const base =
          paths.length > 0
            ? paths
            : [status.defaultPath || "~/Documents/SasCompta-import"];
        const live = previewFolderWatchStatus(true);
        setStatus({ ...live, paths: base, path: base[0] ?? null });
        setPaths(base);
        setFlash(
          "Aperçu navigateur — la surveillance réelle s’active dans l’app desktop.",
        );
        return;
      }
      let list = paths;
      if (!list.length) {
        const def =
          status.defaultPath || (await api.folderWatchDefaultPath());
        list = [def];
        setPaths(list);
      }
      const s = await api.folderWatchStart(list);
      applyStatus(s);
      setFlash(
        `Surveillance active — ${s.paths.length} dossier${s.paths.length > 1 ? "s" : ""} (PDF / images → file Import).`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function stop() {
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const idle = previewFolderWatchStatus(false);
        setStatus({ ...idle, paths, path: paths[0] ?? idle.path });
        setFlash("Surveillance arrêtée (aperçu).");
        return;
      }
      const s = await api.folderWatchStop();
      applyStatus(s);
      setFlash("Surveillance de dossiers arrêtée.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className={`folder-watch${compact ? " is-compact" : ""}`}>
      <header className="panel-head">
        <div>
          <h2>
            Dossiers surveillés <FieldHelp topic="dossierSurveille" compact />
          </h2>
          <p className="muted">
            Déposez des PDF ou photos dans un ou plusieurs dossiers de ce PC
            (sous-dossiers inclus) — SasCompta les copie et les enfile ici pour{" "}
            <strong>revue</strong> (mêmes classifieurs multi-types que l’import
            manuel). Aucun cloud. La liste est mémorisée.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {flash && <p className="ok">{flash}</p>}
      {status.lastError && status.running && (
        <p className="error muted tiny">Dernier souci : {status.lastError}</p>
      )}

      <ul className="folder-watch-list">
        {paths.map((p) => (
          <li key={p}>
            <span className="mono">{folderWatchPathLabel(p)}</span>
            <button
              type="button"
              className="ghost danger-ghost tiny-btn"
              disabled={busy}
              onClick={() => void removeFolder(p)}
              aria-label={`Retirer ${p}`}
            >
              Retirer
            </button>
          </li>
        ))}
        {paths.length === 0 && (
          <li className="folder-watch-empty muted">
            Aucun dossier encore — à l’activation, le défaut{" "}
            <strong className="mono">
              {status.defaultPath || "~/Documents/SasCompta-import"}
            </strong>{" "}
            sera créé et surveillé. Vous pouvez en ajouter d’autres avant.
          </li>
        )}
      </ul>

      <div className="btn-row folder-watch-actions">
        <button
          type="button"
          className="ghost"
          disabled={busy}
          onClick={() => void addFolder()}
        >
          Ajouter un dossier…
        </button>
        {!status.running ? (
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void start()}
          >
            Activer la surveillance
          </button>
        ) : (
          <button
            type="button"
            className="ghost danger-ghost"
            disabled={busy}
            onClick={() => void stop()}
          >
            Arrêter la surveillance
          </button>
        )}
      </div>

      {status.running && (
        <div className="folder-watch-live">
          <div className="meta-grid folder-watch-meta">
            <div className="meta-item">
              <span>Dossiers</span>
              <strong>{status.paths.length || paths.length}</strong>
            </div>
            <div className="meta-item">
              <span>Importés cette session</span>
              <strong>{status.receivedCount}</strong>
            </div>
            <div className="meta-item">
              <span>Ignorés (déjà connus)</span>
              <strong>{status.skippedCount}</strong>
            </div>
          </div>
          <p className="muted tiny">
            La surveillance parcourt aussi les sous-dossiers. Un même fichier
            (même contenu) n’est pas réimporté. Chaque nouveauté apparaît dans
            la file « En revue » avant toute écriture. Ajouter ou retirer un
            dossier met à jour la surveillance sans tout redémarrer
            manuellement.
          </p>
        </div>
      )}
    </section>
  );
}

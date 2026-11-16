import { useCallback, useEffect, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FieldHelp } from "./FieldHelp";
import { api } from "../lib/api";
import {
  folderWatchPathLabel,
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

export function FolderWatchPanel({
  compact = false,
  onDocumentReceived,
}: Props) {
  const [status, setStatus] = useState<FolderWatchStatus>(() =>
    previewFolderWatchStatus(PREVIEW && PREVIEW_LIVE),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [pickedPath, setPickedPath] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (PREVIEW || !isTauriRuntime()) {
      return;
    }
    const s = await api.folderWatchStatus();
    setStatus(s);
  }, []);

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

  async function pickFolder() {
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const demo = status.defaultPath || "~/Documents/SasCompta-import";
        setPickedPath(demo);
        setFlash(`Aperçu — dossier choisi : ${demo}`);
        return;
      }
      const selected = await open({
        directory: true,
        multiple: false,
        title: "Choisir le dossier surveillé",
      });
      if (typeof selected === "string" && selected) {
        setPickedPath(selected);
        setFlash(`Dossier choisi : ${selected}`);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function start() {
    setBusy(true);
    setError(null);
    setFlash(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus(previewFolderWatchStatus(true));
        setFlash(
          "Aperçu navigateur — la surveillance réelle s’active dans l’app desktop.",
        );
        return;
      }
      const path =
        pickedPath ||
        status.path ||
        status.defaultPath ||
        (await api.folderWatchDefaultPath());
      const s = await api.folderWatchStart(path);
      setStatus(s);
      setPickedPath(s.path);
      setFlash(
        `Surveillance active — déposez PDF ou images dans ${folderWatchPathLabel(s.path)}.`,
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
        setStatus(previewFolderWatchStatus(false));
        setFlash("Surveillance arrêtée (aperçu).");
        return;
      }
      const s = await api.folderWatchStop();
      setStatus(s);
      setFlash("Surveillance de dossier arrêtée.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const displayPath = folderWatchPathLabel(
    pickedPath || status.path,
    status.defaultPath,
  );

  return (
    <section className={`folder-watch${compact ? " is-compact" : ""}`}>
      <header className="panel-head">
        <div>
          <h2>
            Dossier surveillé <FieldHelp topic="dossierSurveille" compact />
          </h2>
          <p className="muted">
            Déposez des PDF ou photos dans un dossier de ce PC — SasCompta les
            copie et les enfile ici pour <strong>revue</strong> (mêmes
            classifieurs multi-types que l’import manuel). Aucun cloud.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {flash && <p className="ok">{flash}</p>}
      {status.lastError && status.running && (
        <p className="error muted tiny">Dernier souci : {status.lastError}</p>
      )}

      {!status.running ? (
        <div className="folder-watch-idle">
          <p>
            Dossier par défaut :{" "}
            <strong className="mono">{displayPath}</strong>. Vous pouvez en
            choisir un autre avant d’activer.
          </p>
          <div className="btn-row">
            <button
              type="button"
              className="ghost"
              disabled={busy}
              onClick={() => void pickFolder()}
            >
              Choisir un dossier…
            </button>
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => void start()}
            >
              Activer la surveillance
            </button>
          </div>
        </div>
      ) : (
        <div className="folder-watch-live">
          <div className="meta-grid folder-watch-meta">
            <div className="meta-item">
              <span>Dossier</span>
              <strong className="mono">{displayPath}</strong>
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
            Un même fichier (même contenu) n’est pas réimporté. Chaque nouveauté
            apparaît dans la file « En revue » avant toute écriture.
          </p>
          <div className="btn-row">
            <button
              type="button"
              className="ghost"
              disabled={busy}
              onClick={() => void pickFolder()}
            >
              Changer de dossier…
            </button>
            <button
              type="button"
              className="ghost danger-ghost"
              disabled={busy}
              onClick={() => void stop()}
            >
              Arrêter la surveillance
            </button>
          </div>
          {pickedPath && pickedPath !== status.path && (
            <div className="btn-row" style={{ marginTop: "0.5rem" }}>
              <button
                type="button"
                className="primary"
                disabled={busy}
                onClick={() => void start()}
              >
                Redémarrer sur ce dossier
              </button>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

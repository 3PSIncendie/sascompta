import { useEffect, useState } from "react";
import {
  checkForAppUpdate,
  downloadAndInstallUpdate,
  formatUpdaterCheckError,
  isTauriRuntime,
  type UpdateStatus,
} from "../lib/updater";
import type { Update } from "@tauri-apps/plugin-updater";

interface Props {
  /** Compact control for the sidebar footer. */
  variant?: "banner" | "button";
  /** When true, run a silent check shortly after mount. */
  checkOnStartup?: boolean;
}

function statusLabel(status: UpdateStatus): string {
  switch (status.kind) {
    case "idle":
      return "Vérifier les mises à jour";
    case "checking":
      return "Recherche…";
    case "up-to-date":
      return "À jour";
    case "available":
      return `Mettre à jour (v${status.update.version})`;
    case "downloading": {
      const pct =
        status.progress == null
          ? ""
          : ` ${Math.round(status.progress * 100)} %`;
      return `Téléchargement v${status.version}${pct}…`;
    }
    case "installing":
      return `Installation v${status.version}…`;
    case "error":
      return "Réessayer la mise à jour";
  }
}

export function UpdateChecker({
  variant = "button",
  checkOnStartup = false,
}: Props) {
  const [status, setStatus] = useState<UpdateStatus>({ kind: "idle" });
  const [pending, setPending] = useState<Update | null>(null);

  useEffect(() => {
    if (!checkOnStartup || !isTauriRuntime()) return;
    let cancelled = false;

    void (async () => {
      setStatus({ kind: "checking" });
      try {
        const update = await checkForAppUpdate();
        if (cancelled) return;
        if (update) {
          setPending(update);
          setStatus({ kind: "available", update });
        } else {
          setStatus({ kind: "up-to-date" });
        }
      } catch (err) {
        if (cancelled) return;
        // Silent on startup: network / missing release is not an error UX.
        console.warn("Vérification auto-update:", err);
        setStatus({ kind: "idle" });
      }
    })();

    return () => {
      cancelled = true;
    };
  }, [checkOnStartup]);

  async function runCheck() {
    if (!isTauriRuntime()) {
      setStatus({
        kind: "error",
        message: "Les mises à jour sont disponibles uniquement dans l’app desktop.",
      });
      return;
    }
    setStatus({ kind: "checking" });
    try {
      const update = await checkForAppUpdate();
      if (update) {
        setPending(update);
        setStatus({ kind: "available", update });
      } else {
        setPending(null);
        setStatus({ kind: "up-to-date" });
      }
    } catch (err) {
      setStatus({ kind: "error", message: formatUpdaterCheckError(err) });
    }
  }

  async function runInstall(update: Update) {
    setStatus({ kind: "downloading", version: update.version, progress: null });
    try {
      await downloadAndInstallUpdate(update, (ratio) => {
        setStatus({
          kind: "downloading",
          version: update.version,
          progress: ratio,
        });
      });
      setStatus({ kind: "installing", version: update.version });
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Échec du téléchargement ou de l’installation.";
      setStatus({ kind: "error", message });
    }
  }

  const busy =
    status.kind === "checking" ||
    status.kind === "downloading" ||
    status.kind === "installing";

  if (variant === "banner") {
    if (status.kind !== "available" && status.kind !== "downloading" && status.kind !== "installing" && status.kind !== "error") {
      return null;
    }
    return (
      <div className="update-banner" role="status">
        <div className="update-banner-text">
          {status.kind === "available" && (
            <>
              <strong>Mise à jour disponible</strong>
              <span>
                Version {status.update.version}
                {status.update.body ? ` — ${status.update.body}` : ""}
              </span>
            </>
          )}
          {(status.kind === "downloading" || status.kind === "installing") && (
            <>
              <strong>{statusLabel(status)}</strong>
              <span>L’application redémarrera après l’installation.</span>
            </>
          )}
          {status.kind === "error" && (
            <>
              <strong>Mise à jour</strong>
              <span>{status.message}</span>
            </>
          )}
        </div>
        {status.kind === "available" && (
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void runInstall(status.update)}
          >
            Installer
          </button>
        )}
        {status.kind === "error" && (
          <button type="button" className="secondary" onClick={() => void runCheck()}>
            Réessayer
          </button>
        )}
      </div>
    );
  }

  const canInstall = status.kind === "available" || pending != null;
  const label = statusLabel(
    status.kind === "idle" && pending
      ? { kind: "available", update: pending }
      : status,
  );

  return (
    <div className="update-control">
      <button
        type="button"
        className="ghost sidebar-update"
        disabled={busy}
        onClick={() => {
          if (status.kind === "available") {
            void runInstall(status.update);
          } else if (pending && status.kind !== "error") {
            void runInstall(pending);
          } else {
            void runCheck();
          }
        }}
      >
        {canInstall && status.kind !== "checking" ? label : status.kind === "up-to-date" ? "À jour — revérifier" : label}
      </button>
      {status.kind === "error" && (
        <p className="update-error" title={status.message}>
          {status.message}
        </p>
      )}
      {status.kind === "up-to-date" && (
        <p className="update-ok">SasCompta est à jour.</p>
      )}
    </div>
  );
}

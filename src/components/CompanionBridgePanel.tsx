import { useCallback, useEffect, useRef, useState } from "react";
import { CompanionQr } from "./CompanionQr";
import { FieldHelp } from "./FieldHelp";
import { api } from "../lib/api";
import {
  companionEndpoint,
  companionHostUnreachableFromPhone,
  pairingExpiresLabel,
  pairingIsExpired,
  previewCompanionStatus,
  type CompanionStatus,
} from "../lib/companion";
import { isTauriRuntime } from "../lib/updater";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");
const PREVIEW_LIVE =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("companion") === "1";

type Props = {
  compact?: boolean;
  onDocumentReceived?: () => void;
};

export function CompanionBridgePanel({
  compact = false,
  onDocumentReceived,
}: Props) {
  const [status, setStatus] = useState<CompanionStatus>(() =>
    previewCompanionStatus(PREVIEW && PREVIEW_LIVE),
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [flash, setFlash] = useState<string | null>(null);
  const [tick, setTick] = useState(0);
  const rotatingExpired = useRef(false);

  const refresh = useCallback(async () => {
    if (PREVIEW || !isTauriRuntime()) {
      return;
    }
    const s = await api.companionStatus();
    setStatus(s);
  }, []);

  useEffect(() => {
    void refresh().catch((e) => setError(String(e)));
  }, [refresh]);

  useEffect(() => {
    if (!status.running) return;
    const id = window.setInterval(() => setTick((n) => n + 1), 1000);
    return () => window.clearInterval(id);
  }, [status.running]);

  // Code expiré → régénérer automatiquement pour que le QR reste scannable.
  useEffect(() => {
    if (!status.running || !pairingIsExpired(status.pairingExpiresAt)) {
      rotatingExpired.current = false;
      return;
    }
    if (rotatingExpired.current || busy) return;
    rotatingExpired.current = true;
    if (PREVIEW || !isTauriRuntime()) {
      const s = previewCompanionStatus(true);
      s.pairingCode = "917204";
      s.qrPayload = `http://${s.host}:${s.port}/launch?v=1&code=917204`;
      setStatus(s);
      setFlash("Code d’appairage renouvelé (aperçu).");
      rotatingExpired.current = false;
      return;
    }
    void api
      .companionRotatePairing()
      .then((s) => {
        setStatus(s);
        setFlash("Code d’appairage renouvelé — rescanner le QR.");
      })
      .catch((e) => setError(String(e)))
      .finally(() => {
        rotatingExpired.current = false;
      });
  }, [status.running, status.pairingExpiresAt, busy, tick]);

  useEffect(() => {
    if (PREVIEW || !isTauriRuntime()) return;
    let unlisten: (() => void) | undefined;
    void import("@tauri-apps/api/event")
      .then(({ listen }) =>
        listen<{
          id: number;
          sourceFileName: string;
          docType: string;
        }>("companion-document-received", (ev) => {
          setFlash(
            `Reçu depuis le téléphone : ${ev.payload.sourceFileName} (#${ev.payload.id}) — à revoir avant écriture.`,
          );
          void refresh();
          onDocumentReceived?.();
        }),
      )
      .then((fn) => {
        unlisten = fn;
      })
      .catch(() => {
        /* ignore if event API unavailable */
      });
    return () => {
      unlisten?.();
    };
  }, [onDocumentReceived, refresh]);

  async function start() {
    setBusy(true);
    setError(null);
    setFlash(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus(previewCompanionStatus(true));
        setFlash(
          "Aperçu navigateur — le pont LAN réel s’active dans l’app desktop.",
        );
        return;
      }
      const s = await api.companionStart();
      setStatus(s);
      setFlash(
        "Pont Wi‑Fi local démarré. Sur Android : scannez ce QR avec la caméra (Chrome) — envoi photo sans Expo. Vérifiez l’IP si la page ne charge pas.",
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
        setStatus(previewCompanionStatus(false));
        return;
      }
      const s = await api.companionStop();
      setStatus(s);
      setFlash("Pont compagnon arrêté.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function rotate() {
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const s = previewCompanionStatus(true);
        s.pairingCode = "917204";
        setStatus(s);
        return;
      }
      const s = await api.companionRotatePairing();
      setStatus(s);
      setFlash("Nouveau code d’appairage — validez-le sur le téléphone.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function selectHost(host: string) {
    if (!host || host === status.host) return;
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        const s = previewCompanionStatus(true);
        s.host = host;
        s.hostIsLoopback = companionHostUnreachableFromPhone(host);
        s.qrPayload = `http://${host}:${s.port}/launch?v=1&code=${s.pairingCode}`;
        setStatus(s);
        setFlash(`QR mis à jour pour ${host} (aperçu).`);
        return;
      }
      const s = await api.companionSetAdvertisedHost(host);
      setStatus(s);
      setFlash(
        `QR mis à jour pour ${host} — rescanner depuis le téléphone (même Wi‑Fi).`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const endpoint = companionEndpoint(status.host, status.port);
  const hostBad = companionHostUnreachableFromPhone(status.host);
  const lanChoices = (status.lanAddresses?.length
    ? status.lanAddresses
    : status.host
      ? [status.host]
      : []
  ).filter(Boolean);
  void tick;

  return (
    <section className={`companion-bridge${compact ? " is-compact" : ""}`}>
      <header className="panel-head">
        <div>
          <h2>
            Compagnon mobile{" "}
            <FieldHelp topic="compagnonMobile" compact />
          </h2>
          <p className="muted">
            Envoyez une photo ou un PDF depuis votre téléphone vers ce PC —{" "}
            <strong>Wi‑Fi local uniquement</strong>, aucun cloud. Le document
            arrive ici en revue avant toute écriture.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {flash && <p className="ok">{flash}</p>}

      {!status.running ? (
        <div className="companion-idle">
          <p>
            Activez le pont tant que SasCompta reste ouvert sur ce réseau. Le
            téléphone et le PC doivent être sur le <strong>même Wi‑Fi</strong>.
          </p>
          <div className="btn-row">
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => void start()}
            >
              Activer le pont Wi‑Fi
            </button>
          </div>
        </div>
      ) : (
        <div className="companion-live">
          <div className="companion-live-grid">
            <div className="companion-qr-wrap" aria-label="QR d’appairage">
              <CompanionQr
                payload={status.qrPayload}
                backendSvg={status.qrSvg}
              />
            </div>
            <div className="companion-meta">
              <div className="meta-grid companion-meta-grid">
                <div className="meta-item">
                  <span>Code d’appairage</span>
                  <strong className="companion-code">
                    {status.pairingCode}
                  </strong>
                </div>
                <div className="meta-item">
                  <span>Valide encore</span>
                  <strong>
                    {pairingExpiresLabel(status.pairingExpiresAt)}
                  </strong>
                </div>
                <div className="meta-item">
                  <span>Adresse LAN (QR)</span>
                  <strong className="mono">{endpoint}</strong>
                </div>
                <div className="meta-item">
                  <span>Écoute</span>
                  <strong className="mono">
                    {status.bindAddress || "0.0.0.0"}:{status.port}
                  </strong>
                </div>
                <div className="meta-item">
                  <span>Société</span>
                  <strong>{status.orgName || "—"}</strong>
                </div>
                <div className="meta-item">
                  <span>Téléphone</span>
                  <strong>
                    {status.sessionPaired ? "Appairé" : "En attente"}
                  </strong>
                </div>
                <div className="meta-item">
                  <span>Reçus cette session</span>
                  <strong>{status.receivedCount}</strong>
                </div>
              </div>
              {hostBad && (
                <p className="error">
                  L’IP du QR est <strong>127.0.0.1</strong> (ce PC seulement) —
                  le téléphone ne pourra pas ouvrir l’URL. Choisissez l’IP Wi‑Fi
                  ci‑dessous, ou vérifiez que le PC est bien sur le réseau.
                </p>
              )}
              {lanChoices.length > 1 && (
                <div className="companion-lan-pick">
                  <p className="muted tiny">
                    Si Chrome Android affiche « site inaccessible », l’IP auto
                    est peut‑être un VPN / Hyper‑V. Choisissez l’IP du{" "}
                    <strong>Wi‑Fi</strong> (souvent <code>192.168.…</code>) :
                  </p>
                  <div className="btn-row companion-lan-btns">
                    {lanChoices.map((ip) => (
                      <button
                        key={ip}
                        type="button"
                        className={
                          ip === status.host ? "primary" : "ghost"
                        }
                        disabled={busy || ip === status.host}
                        onClick={() => void selectHost(ip)}
                      >
                        {ip}
                      </button>
                    ))}
                  </div>
                </div>
              )}
              <p className="muted tiny">
                Android : scannez ce QR avec la <strong>caméra / Chrome</strong>{" "}
                → page d’envoi photo/PDF (HTTP local, pas de cloud). Pare-feu
                Windows : autoriser SasCompta (TCP {status.port}) en entrée.
                Expo Go reste optionnel pour l’app React Native. Chaque envoi
                arrive dans « En revue ».
              </p>
              <div className="btn-row">
                <button
                  type="button"
                  className="ghost"
                  disabled={busy}
                  onClick={() => void rotate()}
                >
                  Nouveau code
                </button>
                <button
                  type="button"
                  className="ghost danger-ghost"
                  disabled={busy}
                  onClick={() => void stop()}
                >
                  Arrêter le pont
                </button>
              </div>
            </div>
          </div>
        </div>
      )}
    </section>
  );
}

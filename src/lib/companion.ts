/** Pont compagnon mobile (Wi‑Fi local) — types + aperçu navigateur. */

export type CompanionStatus = {
  running: boolean;
  host: string | null;
  port: number | null;
  orgName: string | null;
  pairingCode: string | null;
  pairingExpiresAt: string | null;
  sessionPaired: boolean;
  qrPayload: string | null;
  qrSvg: string | null;
  receivedCount: number;
};

export function previewCompanionStatus(running = false): CompanionStatus {
  if (!running) {
    return {
      running: false,
      host: null,
      port: null,
      orgName: null,
      pairingCode: null,
      pairingExpiresAt: null,
      sessionPaired: false,
      qrPayload: null,
      qrSvg: null,
      receivedCount: 0,
    };
  }
  const expires = new Date(Date.now() + 5 * 60 * 1000).toISOString();
  const payload = JSON.stringify({
    v: 1,
    app: "sascompta",
    host: "192.168.1.42",
    port: 17832,
    code: "482913",
    org: "Demo SAS",
    expiresAt: expires,
  });
  return {
    running: true,
    host: "192.168.1.42",
    port: 17832,
    orgName: "Demo SAS",
    pairingCode: "482913",
    pairingExpiresAt: expires,
    sessionPaired: false,
    qrPayload: payload,
    qrSvg: null,
    receivedCount: 0,
  };
}

export function pairingExpiresLabel(iso: string | null | undefined): string {
  if (!iso) return "—";
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return iso;
  const sec = Math.max(0, Math.round((t - Date.now()) / 1000));
  if (sec <= 0) return "expiré";
  const m = Math.floor(sec / 60);
  const s = sec % 60;
  return m > 0 ? `${m} min ${s.toString().padStart(2, "0")} s` : `${s} s`;
}

export function companionEndpoint(
  host: string | null | undefined,
  port: number | null | undefined,
): string | null {
  if (!host || !port) return null;
  return `http://${host}:${port}`;
}

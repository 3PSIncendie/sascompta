import type { DocTypeHint, PairingPayload, Session } from "./types";

export function baseUrl(host: string, port: number): string {
  return `http://${host}:${port}`;
}

export async function fetchInfo(
  host: string,
  port: number,
): Promise<{ orgName?: string; pairingOpen?: boolean }> {
  const res = await fetch(`${baseUrl(host, port)}/v1/info`);
  if (!res.ok) {
    throw new Error(`PC injoignable (HTTP ${res.status}). Même Wi‑Fi ?`);
  }
  return res.json();
}

export async function pair(
  host: string,
  port: number,
  code: string,
): Promise<Session> {
  const res = await fetch(`${baseUrl(host, port)}/v1/pair`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ code: code.trim() }),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(
      typeof data.error === "string"
        ? data.error
        : `Appairage refusé (HTTP ${res.status})`,
    );
  }
  return {
    host,
    port,
    token: String(data.token),
    orgName: String(data.orgName || "Société"),
  };
}

export async function uploadDocument(
  session: Session,
  bytes: Uint8Array,
  opts: {
    fileName: string;
    mimeType: string;
    docType: DocTypeHint;
  },
): Promise<{ id: number; message?: string }> {
  const res = await fetch(`${baseUrl(session.host, session.port)}/v1/upload`, {
    method: "POST",
    headers: {
      Authorization: `Bearer ${session.token}`,
      "Content-Type": opts.mimeType || "application/octet-stream",
      "X-Doc-Type": opts.docType,
      "X-File-Name": opts.fileName,
    },
    body: bytes,
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    throw new Error(
      typeof data.error === "string"
        ? data.error
        : `Envoi refusé (HTTP ${res.status})`,
    );
  }
  return { id: Number(data.id), message: data.message };
}

export function parsePairingPayload(raw: string): PairingPayload {
  const data = JSON.parse(raw) as PairingPayload;
  if (!data.host || !data.port || !data.code) {
    throw new Error("QR invalide (host / port / code manquants).");
  }
  return data;
}

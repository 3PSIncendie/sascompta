import type { DocTypeHint, PairingPayload, Session } from "./types";

export function baseUrl(host: string, port: number): string {
  return `http://${host}:${port}`;
}

function networkHint(err: unknown): string {
  const msg = err instanceof Error ? err.message : String(err);
  if (/Network request failed|Failed to fetch|Cleartext|CLEARTEXT|ATS/i.test(msg)) {
    return "PC injoignable en HTTP local — même Wi‑Fi ? Pare-feu PC ? (Android : cleartext LAN requis.)";
  }
  return msg;
}

export async function fetchInfo(
  host: string,
  port: number,
): Promise<{ orgName?: string; pairingOpen?: boolean }> {
  let res: Response;
  try {
    res = await fetch(`${baseUrl(host, port)}/v1/info`);
  } catch (e) {
    throw new Error(networkHint(e));
  }
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
  let res: Response;
  try {
    res = await fetch(`${baseUrl(host, port)}/v1/pair`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ code: code.trim() }),
    });
  } catch (e) {
    throw new Error(networkHint(e));
  }
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
  let res: Response;
  try {
    res = await fetch(`${baseUrl(session.host, session.port)}/v1/upload`, {
      method: "POST",
      headers: {
        Authorization: `Bearer ${session.token}`,
        "Content-Type": opts.mimeType || "application/octet-stream",
        "X-Doc-Type": opts.docType,
        "X-File-Name": opts.fileName,
      },
      body: bytes,
    });
  } catch (e) {
    throw new Error(networkHint(e));
  }
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

/** Si le QR est un Intent Android, récupère l’URL de repli (page /launch). */
function unwrapAndroidIntent(raw: string): string {
  const text = raw.trim();
  if (!/^intent:/i.test(text)) return text;
  const m = text.match(/S\.browser_fallback_url=([^;]+)/i);
  if (!m?.[1]) {
    throw new Error(
      "Intent Android sans URL de repli — ouvrez Expo Go, puis saisissez IP + code.",
    );
  }
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return m[1];
  }
}

/**
 * Parse le QR desktop :
 * - `http(s)://host:port/launch?code=…` (courant — ouvre Expo Go via la page)
 * - `sascompta://pair?…` (legacy)
 * - Intent Android + JSON legacy
 */
export function parsePairingPayload(raw: string): PairingPayload {
  const text = unwrapAndroidIntent(String(raw ?? "").trim());
  if (!text) {
    throw new Error("QR vide.");
  }

  if (text.startsWith("{")) {
    let data: Record<string, unknown>;
    try {
      data = JSON.parse(text) as Record<string, unknown>;
    } catch {
      throw new Error("QR JSON invalide.");
    }
    const host = String(data.host ?? "").trim();
    const port = Number(data.port);
    const code = String(data.code ?? "").trim();
    if (!host || !Number.isFinite(port) || port <= 0 || !code) {
      throw new Error("QR invalide (host / port / code manquants).");
    }
    return {
      v: Number(data.v) || 1,
      app: typeof data.app === "string" ? data.app : undefined,
      host,
      port,
      code,
      org: typeof data.org === "string" ? data.org : undefined,
      expiresAt: typeof data.expiresAt === "string" ? data.expiresAt : undefined,
    };
  }

  let url: URL;
  try {
    url = new URL(text);
  } catch {
    throw new Error("QR invalide (format non reconnu).");
  }

  const code = (
    url.searchParams.get("code") ||
    url.searchParams.get("c") ||
    ""
  ).trim();

  const host = (
    url.searchParams.get("host") ||
    (url.protocol === "http:" || url.protocol === "https:"
      ? url.hostname
      : "") ||
    ""
  ).trim();

  let port = Number(url.searchParams.get("port"));
  if (!Number.isFinite(port) || port <= 0) {
    if (url.port) {
      port = Number(url.port);
    } else if (url.protocol === "http:") {
      port = 80;
    } else if (url.protocol === "https:") {
      port = 443;
    }
  }

  if (!host || !Number.isFinite(port) || port <= 0 || !code) {
    throw new Error("QR invalide (host / port / code manquants).");
  }
  return {
    v: Number(url.searchParams.get("v")) || 1,
    app: "sascompta",
    host,
    port,
    code,
  };
}

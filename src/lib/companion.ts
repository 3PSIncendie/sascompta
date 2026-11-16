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

export type PairingPayload = {
  v: number;
  app?: string;
  host: string;
  port: number;
  code: string;
  org?: string;
  expiresAt?: string;
};

/** Payload QR compact (ASCII) — scannable + parseable côté téléphone. */
export function buildPairingPayload(input: {
  host: string;
  port: number;
  code: string;
  org?: string;
  expiresAt?: string;
}): string {
  const params = new URLSearchParams();
  params.set("v", "1");
  params.set("host", input.host);
  params.set("port", String(input.port));
  params.set("code", input.code);
  // org / expiresAt volontairement hors QR (densité + accents) — /v1/info les fournit.
  return `sascompta://pair?${params.toString()}`;
}

/**
 * Accepte le format URI `sascompta://pair?...` et l’ancien JSON `{host,port,code}`.
 */
export function parsePairingPayload(raw: string): PairingPayload {
  const text = raw.trim();
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

  // URI sascompta://pair?...  (aussi tolère http(s)://…?host=&port=&code=)
  let url: URL;
  try {
    url = new URL(text);
  } catch {
    throw new Error("QR invalide (format non reconnu).");
  }
  const host = (url.searchParams.get("host") || "").trim();
  const port = Number(url.searchParams.get("port"));
  const code = (url.searchParams.get("code") || "").trim();
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

/** Retire le prologue XML pour injection HTML sûre (WebView / innerHTML). */
export function svgForInnerHtml(svg: string | null | undefined): string | null {
  if (!svg) return null;
  const idx = svg.indexOf("<svg");
  if (idx < 0) return null;
  return svg.slice(idx);
}

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
  const payload = buildPairingPayload({
    host: "192.168.1.42",
    port: 17832,
    code: "482913",
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

export function pairingIsExpired(iso: string | null | undefined): boolean {
  if (!iso) return false;
  const t = Date.parse(iso);
  if (Number.isNaN(t)) return false;
  return t <= Date.now();
}

export function companionEndpoint(
  host: string | null | undefined,
  port: number | null | undefined,
): string | null {
  if (!host || !port) return null;
  return `http://${host}:${port}`;
}

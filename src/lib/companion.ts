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

/**
 * Payload QR : URL HTTP du pont (`/launch`), pas `sascompta://`.
 * Sur Android, un schéma custom cherche une APK native absente alors qu’Expo Go suffit.
 */
export function buildPairingPayload(input: {
  host: string;
  port: number;
  code: string;
  org?: string;
  expiresAt?: string;
}): string {
  const params = new URLSearchParams();
  params.set("v", "1");
  params.set("code", input.code);
  // host/port dans l’autorité HTTP — densité QR + ouverture navigateur → Expo Go.
  return `http://${input.host}:${input.port}/launch?${params.toString()}`;
}

/** Si le QR est un Intent Android, récupère l’URL de repli (page /launch). */
function unwrapAndroidIntent(raw: string): string {
  const text = raw.trim();
  if (!/^intent:/i.test(text)) return text;
  const m = text.match(/S\.browser_fallback_url=([^;]+)/i);
  if (!m?.[1]) {
    throw new Error(
      "Intent Android sans URL de repli — ouvrez Expo Go, puis scannez le QR dans l’app compagnon.",
    );
  }
  try {
    return decodeURIComponent(m[1]);
  } catch {
    return m[1];
  }
}

/**
 * Accepte :
 * - `http(s)://{host}:{port}/launch?code=…` (courant, Android / Expo Go)
 * - `sascompta://pair?host=&port=&code=` (legacy SC-61)
 * - Intent Android avec `S.browser_fallback_url=…`
 * - ancien JSON `{host,port,code}`
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

  // http(s) /launch : host/port depuis l’autorité si absents de la query.
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

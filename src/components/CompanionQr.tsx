import { useEffect, useState } from "react";
import QRCode from "qrcode";
import { svgForInnerHtml } from "../lib/companion";

type Props = {
  payload: string | null;
  /** SVG fourni par le backend (optionnel) — fallback si le rendu client échoue. */
  backendSvg?: string | null;
};

/**
 * Affiche le QR d’appairage à partir du payload texte.
 * Le rendu client (canvas → data URL) est la source de vérité : fiable en
 * aperçu navigateur et sous Tauri, sans dépendre d’un SVG XML injecté.
 */
export function CompanionQr({ payload, backendSvg = null }: Props) {
  const [dataUrl, setDataUrl] = useState<string | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    setDataUrl(null);
    setFailed(false);
    if (!payload) {
      return;
    }
    void QRCode.toDataURL(payload, {
      errorCorrectionLevel: "M",
      margin: 2,
      width: 220,
      color: { dark: "#1a3d2e", light: "#ffffff" },
    })
      .then((url) => {
        if (!cancelled) setDataUrl(url);
      })
      .catch(() => {
        if (!cancelled) setFailed(true);
      });
    return () => {
      cancelled = true;
    };
  }, [payload]);

  if (dataUrl) {
    return (
      <div className="companion-qr">
        <img src={dataUrl} alt="QR d’appairage SasCompta" width={180} height={180} />
      </div>
    );
  }

  const safeSvg = failed ? svgForInnerHtml(backendSvg) : null;
  if (safeSvg) {
    return (
      <div
        className="companion-qr"
        dangerouslySetInnerHTML={{ __html: safeSvg }}
      />
    );
  }

  if (!payload) {
    return (
      <div className="companion-qr companion-qr-fallback">
        <p className="muted">
          QR indisponible — saisissez l’adresse et le code dans l’app.
        </p>
      </div>
    );
  }

  return (
    <div className="companion-qr companion-qr-fallback">
      <p className="muted">Génération du QR…</p>
    </div>
  );
}

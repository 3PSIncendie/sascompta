import type { BillingDocumentPreviewModel } from "../modules/billing/documentPreview";
import logo3psi from "../assets/brand/3psi-logo.png";

export type BillingDocumentPreviewProps = {
  model: BillingDocumentPreviewModel;
  /** Libellé accessibilité (ex. « Aperçu du devis »). */
  label?: string;
};

/**
 * Aperçu papier live (miroir du PDF commercial) — noir/blanc dans un cadre pastel.
 */
export function BillingDocumentPreview({
  model,
  label = "Aperçu du document",
}: BillingDocumentPreviewProps) {
  return (
    <aside className="billing-live-preview" aria-label={label}>
      <div className="billing-live-preview-chrome">
        <span className="billing-live-preview-badge">Aperçu live</span>
        <span className="muted small">Se met à jour à chaque saisie</span>
      </div>
      <div className="billing-doc-sheet-scroll">
        <article className="billing-doc-sheet" aria-live="polite">
          <header className="billing-doc-head">
            <div className="billing-doc-head-row">
              <h2 className="billing-doc-title">{model.title}</h2>
              <img
                className="billing-doc-logo"
                src={logo3psi}
                alt="3PSI"
                width={280}
                height={280}
              />
            </div>
            <dl className="billing-doc-meta">
              {model.meta.map((row) => (
                <div key={row.label} className="billing-doc-meta-row">
                  <dt>{row.label}</dt>
                  <dd>{row.value}</dd>
                </div>
              ))}
            </dl>
          </header>

          <div className="billing-doc-parties">
            <div className="billing-doc-party">
              <strong>{model.org.name}</strong>
              {model.org.addressLines.map((line) => (
                <div key={line}>{line}</div>
              ))}
              {model.org.idLine ? <div>{model.org.idLine}</div> : null}
            </div>
            <div className="billing-doc-party">
              <strong>{model.client.name}</strong>
              {model.client.addressLines.map((line) => (
                <div key={line}>{line}</div>
              ))}
              {model.client.email ? <div>{model.client.email}</div> : null}
              {model.client.idLine ? <div>{model.client.idLine}</div> : null}
            </div>
          </div>

          <table className="billing-doc-table">
            <thead>
              <tr>
                <th className="col-desc">Description</th>
                <th className="col-num">Qté</th>
                <th className="col-num">Prix unitaire</th>
                <th className="col-num">TVA (%)</th>
                <th className="col-num">Total HT</th>
              </tr>
            </thead>
            <tbody>
              {model.lines.length === 0 ? (
                <tr>
                  <td colSpan={5} className="billing-doc-empty">
                    Ajoutez une ligne pour voir le détail…
                  </td>
                </tr>
              ) : (
                model.lines.map((line, i) => (
                  <tr key={`${line.label}-${i}`}>
                    <td className="col-desc">
                      <div className="billing-doc-line-label">{line.label}</div>
                      {line.description
                        ? line.description.split("\n").map((d, j) => (
                            <div key={j} className="billing-doc-line-desc">
                              {d}
                            </div>
                          ))
                        : null}
                    </td>
                    <td className="col-num">{line.quantityLabel}</td>
                    <td className="col-num">{line.unitPriceLabel}</td>
                    <td className="col-num">{line.vatLabel}</td>
                    <td className="col-num">{line.htLabel}</td>
                  </tr>
                ))
              )}
            </tbody>
          </table>

          <div className="billing-doc-totals">
            <div className="billing-doc-total-row">
              <span>Total HT</span>
              <strong>{model.htLabel}</strong>
            </div>
            <div className="billing-doc-total-row">
              <span>Montant total de la TVA</span>
              <strong>{model.vatLabel}</strong>
            </div>
            <div className="billing-doc-total-row billing-doc-ttc">
              <span>Total TTC</span>
              <strong>{model.ttcLabel}</strong>
            </div>
          </div>

          {model.notes || model.legalMentions ? (
            <div className="billing-doc-notes">
              {model.notes ? <p>{model.notes}</p> : null}
              {model.legalMentions ? (
                <p className="billing-doc-legal">{model.legalMentions}</p>
              ) : null}
            </div>
          ) : null}

          <footer className="billing-doc-footer">
            <span>{model.footerLeft}</span>
            <span>
              {model.number} · 1/1
            </span>
          </footer>
        </article>
      </div>
    </aside>
  );
}

import type { DraftLine } from "../modules/billing/quotes";
import { emptyDraftLine } from "../modules/billing/quotes";
import type { VatCode } from "../lib/types";

export type BillingLinesEditorProps = {
  lines: DraftLine[];
  onChange: (lines: DraftLine[]) => void;
  vatCodes: VatCode[];
  vatMode: string;
  /** Compte par défaut pour une nouvelle ligne. */
  defaultAccount?: string;
};

export function BillingLinesEditor({
  lines,
  onChange,
  vatCodes,
  vatMode,
  defaultAccount = "706",
}: BillingLinesEditorProps) {
  function updateLine(key: string, patch: Partial<DraftLine>) {
    onChange(lines.map((l) => (l.key === key ? { ...l, ...patch } : l)));
  }

  function addLine() {
    const rate = vatCodes[0]?.rateBp ?? 2000;
    const code = vatCodes[0]?.code ?? "T20C";
    onChange([
      ...lines,
      { ...emptyDraftLine(rate, code), accountNumber: defaultAccount },
    ]);
  }

  function removeLine(key: string) {
    if (lines.length <= 1) {
      onChange([{ ...emptyDraftLine(), accountNumber: defaultAccount }]);
      return;
    }
    onChange(lines.filter((l) => l.key !== key));
  }

  return (
    <div className="billing-lines">
      <div className="billing-lines-head">
        <h3>Lignes</h3>
        <span className="muted small">
          {lines.length} ligne{lines.length > 1 ? "s" : ""} — titre + détails,
          sans limite
        </span>
      </div>
      <div className="billing-lines-list">
        {lines.map((l, index) => (
          <article key={l.key} className="billing-line-card">
            <header className="billing-line-card-head">
              <span className="billing-line-index">#{index + 1}</span>
              <button
                type="button"
                className="ghost"
                onClick={() => removeLine(l.key)}
              >
                Retirer
              </button>
            </header>
            <div className="billing-line-title-block">
              <label className="field">
                Titre
                <input
                  value={l.label}
                  onChange={(e) => updateLine(l.key, { label: e.target.value })}
                  placeholder="Titre de la ligne…"
                  required
                />
              </label>
              <label className="field">
                Détails
                <textarea
                  rows={2}
                  value={l.description}
                  onChange={(e) =>
                    updateLine(l.key, { description: e.target.value })
                  }
                  placeholder="Sous-lignes / description (optionnel)"
                />
              </label>
            </div>
            <div className="billing-line-metrics">
              <label className="field">
                Qté
                <input
                  value={l.quantity}
                  onChange={(e) =>
                    updateLine(l.key, { quantity: e.target.value })
                  }
                />
              </label>
              <label className="field">
                Unité
                <input
                  value={l.unit}
                  onChange={(e) => updateLine(l.key, { unit: e.target.value })}
                />
              </label>
              <label className="field">
                P.U. HT
                <input
                  value={l.unitPriceEuros}
                  onChange={(e) =>
                    updateLine(l.key, { unitPriceEuros: e.target.value })
                  }
                  placeholder="0,00"
                />
              </label>
              <label className="field">
                Compte
                <input
                  value={l.accountNumber}
                  onChange={(e) =>
                    updateLine(l.key, { accountNumber: e.target.value })
                  }
                />
              </label>
              <label className="field">
                TVA
                <select
                  value={l.vatCode}
                  onChange={(e) => {
                    const code = vatCodes.find((c) => c.code === e.target.value);
                    updateLine(l.key, {
                      vatCode: e.target.value,
                      vatRateBp: code?.rateBp ?? l.vatRateBp,
                    });
                  }}
                  disabled={vatMode === "sans"}
                >
                  {vatCodes.map((c) => (
                    <option key={c.code} value={c.code}>
                      {c.code}
                    </option>
                  ))}
                </select>
              </label>
            </div>
          </article>
        ))}
      </div>
      <div className="btn-row">
        <button type="button" className="secondary" onClick={addLine}>
          Ajouter une ligne
        </button>
      </div>
    </div>
  );
}

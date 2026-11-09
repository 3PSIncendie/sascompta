import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { LabelWithHelp } from "../components/FieldHelp";
import { NewThirdPartyModal } from "../components/NewThirdPartyModal";
import { api } from "../lib/api";
import { formatEuros, formatRateBp } from "../lib/money";
import type {
  AuxBalanceReport,
  ThirdParty,
  VatCode,
  VatReport,
} from "../lib/types";

type Tab = "ca3" | "aux" | "codes" | "tiers";

function directionLabel(direction: string): string {
  switch (direction) {
    case "collected":
      return "collectée";
    case "deductible":
      return "déductible";
    case "exempt":
      return "exonéré";
    case "reverse":
      return "autoliquidation";
    default:
      return direction;
  }
}

export function TaxScreen() {
  const [tab, setTab] = useState<Tab>("ca3");
  const [fromDate, setFromDate] = useState("2026-01-01");
  const [toDate, setToDate] = useState("2026-12-31");
  const [vat, setVat] = useState<VatReport | null>(null);
  const [aux, setAux] = useState<AuxBalanceReport | null>(null);
  const [codes, setCodes] = useState<VatCode[]>([]);
  const [tiers, setTiers] = useState<ThirdParty[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [createOpen, setCreateOpen] = useState(false);

  async function refresh() {
    setError(null);
    try {
      const [v, a, c, t] = await Promise.all([
        api.vatReport(fromDate, toDate),
        api.auxiliaryBalance(fromDate, toDate),
        api.listVatCodes(),
        api.listThirdParties(),
      ]);
      setVat(v);
      setAux(a);
      setCodes(c);
      setTiers(t);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function exportVat() {
    const path = await save({
      defaultPath: "tva-ca3-demo.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportVatCsv(fromDate, toDate, String(path));
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportAux() {
    const path = await save({
      defaultPath: "balance-auxiliaire-demo.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportAuxCsv(fromDate, toDate, String(path));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="panel">
      <h1>TVA &amp; tiers</h1>
      <p>
        Codes TVA, ventilation, balance auxiliaire et{" "}
        <LabelWithHelp topic="ca3">brouillon CA3</LabelWithHelp>.
      </p>
      <aside className="help-callout">
        <strong>Rappel</strong>
        <p>
          SasCompta prépare un brouillon (y compris{" "}
          <LabelWithHelp topic="autoliquidationBtp">
            autoliquidation BTP
          </LabelWithHelp>
          ). La déclaration officielle se fait sur le site des impôts.
          Surveillez les échéances dans le parcours guidé.
        </p>
      </aside>

      <div className="tabs">
        {(
          [
            ["ca3", "Brouillon CA3"],
            ["aux", "Balance auxiliaire"],
            ["codes", "Codes TVA"],
            ["tiers", "Tiers"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={tab === id ? "active" : ""}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>

      <div className="toolbar">
        <label>
          Du
          <input
            type="date"
            value={fromDate}
            onChange={(e) => setFromDate(e.target.value)}
          />
        </label>
        <label>
          Au
          <input
            type="date"
            value={toDate}
            onChange={(e) => setToDate(e.target.value)}
          />
        </label>
        <button type="button" className="primary" onClick={() => void refresh()}>
          Actualiser
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      {tab === "ca3" && vat && (
        <>
          <div className="summary-strip">
            <span>
              Collectée <strong>{formatEuros(vat.collectedCents)}</strong>
            </span>
            <span>
              Déductible <strong>{formatEuros(vat.deductibleCents)}</strong>
            </span>
            <span>
              Net à payer <strong>{formatEuros(vat.netCents)}</strong>
            </span>
            <button type="button" className="ghost" onClick={() => void exportVat()}>
              Export CSV
            </button>
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Libellé</th>
                  <th>Sens</th>
                  <th>Taux</th>
                  <th className="num">Base HT</th>
                  <th className="num">TVA</th>
                </tr>
              </thead>
              <tbody>
                {vat.lines.map((l) => (
                  <tr key={l.code}>
                    <td>{l.code}</td>
                    <td>{l.label}</td>
                    <td>{directionLabel(l.direction)}</td>
                    <td>{formatRateBp(l.rateBp)}</td>
                    <td className="num">{formatEuros(l.baseCents)}</td>
                    <td className="num">{formatEuros(l.vatCents)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {tab === "aux" && aux && (
        <>
          <div className="btn-row" style={{ marginBottom: "0.75rem" }}>
            <button type="button" className="ghost" onClick={() => void exportAux()}>
              Export CSV
            </button>
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Compte</th>
                  <th>Libellé</th>
                  <th>Auxiliaire</th>
                  <th>Libellé auxiliaire</th>
                  <th className="num">Débit</th>
                  <th className="num">Crédit</th>
                  <th className="num">Solde D</th>
                  <th className="num">Solde C</th>
                </tr>
              </thead>
              <tbody>
                {aux.rows.map((r) => (
                  <tr key={`${r.accountNumber}-${r.auxCode}`}>
                    <td>{r.accountNumber}</td>
                    <td>{r.accountLabel}</td>
                    <td>{r.auxCode}</td>
                    <td>{r.auxLabel}</td>
                    <td className="num">{formatEuros(r.debitCents)}</td>
                    <td className="num">{formatEuros(r.creditCents)}</td>
                    <td className="num">{formatEuros(r.soldeDebitCents)}</td>
                    <td className="num">{formatEuros(r.soldeCreditCents)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}

      {tab === "codes" && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Code</th>
                <th>Libellé</th>
                <th>Taux</th>
                <th>Sens</th>
              </tr>
            </thead>
            <tbody>
              {codes.map((c) => (
                <tr key={c.code}>
                  <td>{c.code}</td>
                  <td>{c.label}</td>
                  <td>{formatRateBp(c.rateBp)}</td>
                  <td>{directionLabel(c.direction)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {tab === "tiers" && (
        <>
          <div className="toolbar">
            <button
              type="button"
              className="primary"
              onClick={() => setCreateOpen(true)}
            >
              Nouveau tiers
            </button>
            <span className="muted small">
              Même formulaire que sur devis / factures (SIRET, adresse, e-mail).
            </span>
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Nom</th>
                  <th>Type</th>
                  <th>Compte</th>
                  <th>SIRET</th>
                  <th>E-mail</th>
                </tr>
              </thead>
              <tbody>
                {tiers.map((t) => (
                  <tr key={t.id}>
                    <td>{t.code}</td>
                    <td>{t.name}</td>
                    <td>{t.kind === "supplier" ? "Fournisseur" : "Client"}</td>
                    <td>{t.accountNumber}</td>
                    <td>{t.siret || "—"}</td>
                    <td>{t.email || "—"}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          <NewThirdPartyModal
            open={createOpen}
            onClose={() => setCreateOpen(false)}
            onCreated={() => {
              void refresh();
            }}
            existing={tiers}
            defaultKind="customer"
            allowAnyKind
          />
        </>
      )}
    </div>
  );
}

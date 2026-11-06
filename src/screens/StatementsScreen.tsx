import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import type { AnnualStatements, LedgerFilter } from "../lib/types";

type Tab = "bilan" | "cr" | "annexe";

export function StatementsScreen() {
  const [tab, setTab] = useState<Tab>("bilan");
  const [fromDate, setFromDate] = useState("2026-01-01");
  const [toDate, setToDate] = useState("2026-12-31");
  const [pack, setPack] = useState<AnnualStatements | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);

  function filter(): LedgerFilter {
    return {
      fromDate,
      toDate,
      journalCode: null,
      accountNumber: null,
    };
  }

  async function refresh() {
    setError(null);
    try {
      const p = await api.annualStatements(filter());
      setPack(p);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function exportCsv() {
    const defaults =
      tab === "bilan"
        ? "bilan.csv"
        : tab === "cr"
          ? "compte-resultat.csv"
          : "annexe.csv";
    const path = await save({
      defaultPath: defaults,
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      if (tab === "bilan") await api.exportBilanCsv(filter(), String(path));
      else if (tab === "cr") await api.exportCrCsv(filter(), String(path));
      else await api.exportAnnexeCsv(filter(), String(path));
      setStatus(`Export CSV : ${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportPdf() {
    const path = await save({
      defaultPath: "comptes-annuels.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    try {
      await api.exportAnnualPdf(filter(), String(path));
      setStatus(`PDF généré : ${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportPack() {
    const dir = await open({
      directory: true,
      multiple: false,
      title: "Dossier d'export des comptes annuels",
    });
    if (!dir) return;
    try {
      await api.exportAnnualPack(filter(), String(dir));
      setStatus(`Pack écrit dans ${dir}`);
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="panel">
      <header className="page-head">
        <div>
          <h1>États annuels</h1>
          <p className="muted">
            Bilan, compte de résultat et annexe socle (immos, dettes) — mapping
            PCG → postes.
          </p>
        </div>
        <div className="row-actions">
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
          <button type="button" onClick={() => void refresh()}>
            Actualiser
          </button>
          <button type="button" className="ghost" onClick={() => void exportCsv()}>
            Export CSV
          </button>
          <button type="button" className="ghost" onClick={() => void exportPdf()}>
            Export PDF
          </button>
          <button type="button" className="ghost" onClick={() => void exportPack()}>
            Pack (dossier)
          </button>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="muted">{status}</p>}

      {pack && (
        <p className="muted">
          {pack.organizationName} · SIREN {pack.siren} · résultat{" "}
          <strong>{formatEuros(pack.compteResultat.resultatCents)}</strong>
          {pack.bilan.ecartCents === 0
            ? " · bilan équilibré"
            : ` · écart bilan ${formatEuros(pack.bilan.ecartCents)}`}
        </p>
      )}

      <div className="tabs">
        <button
          type="button"
          className={tab === "bilan" ? "active" : ""}
          onClick={() => setTab("bilan")}
        >
          Bilan
        </button>
        <button
          type="button"
          className={tab === "cr" ? "active" : ""}
          onClick={() => setTab("cr")}
        >
          Compte de résultat
        </button>
        <button
          type="button"
          className={tab === "annexe" ? "active" : ""}
          onClick={() => setTab("annexe")}
        >
          Annexe
        </button>
      </div>

      {!pack && <p className="muted">Chargement…</p>}

      {pack && tab === "bilan" && (
        <div className="split-2">
          <div>
            <h2>Actif</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Poste</th>
                    <th>Comptes</th>
                    <th className="num">Montant</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.bilan.actif.map((l) => (
                    <tr key={l.code}>
                      <td>
                        <strong>{l.code}</strong> {l.label}
                        {l.note ? (
                          <div className="muted small">{l.note}</div>
                        ) : null}
                      </td>
                      <td className="muted">{l.accounts.join(", ")}</td>
                      <td className="num">{formatEuros(l.amountCents)}</td>
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <td colSpan={2}>Total actif</td>
                    <td className="num">
                      {formatEuros(pack.bilan.totalActifCents)}
                    </td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
          <div>
            <h2>Passif</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Poste</th>
                    <th>Comptes</th>
                    <th className="num">Montant</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.bilan.passif.map((l) => (
                    <tr key={l.code}>
                      <td>
                        <strong>{l.code}</strong> {l.label}
                        {l.note ? (
                          <div className="muted small">{l.note}</div>
                        ) : null}
                      </td>
                      <td className="muted">{l.accounts.join(", ")}</td>
                      <td className="num">{formatEuros(l.amountCents)}</td>
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <td colSpan={2}>Total passif</td>
                    <td className="num">
                      {formatEuros(pack.bilan.totalPassifCents)}
                    </td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
        </div>
      )}

      {pack && tab === "cr" && (
        <div className="split-2">
          <div>
            <h2>Produits</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Poste</th>
                    <th className="num">Montant</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.compteResultat.produits.map((l) => (
                    <tr key={l.code}>
                      <td>
                        <strong>{l.code}</strong> {l.label}
                        <div className="muted small">
                          {l.accounts.join(", ")}
                        </div>
                      </td>
                      <td className="num">{formatEuros(l.amountCents)}</td>
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <td>Total produits</td>
                    <td className="num">
                      {formatEuros(pack.compteResultat.totalProduitsCents)}
                    </td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
          <div>
            <h2>Charges</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Poste</th>
                    <th className="num">Montant</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.compteResultat.charges.map((l) => (
                    <tr key={l.code}>
                      <td>
                        <strong>{l.code}</strong> {l.label}
                        <div className="muted small">
                          {l.accounts.join(", ")}
                        </div>
                      </td>
                      <td className="num">{formatEuros(l.amountCents)}</td>
                    </tr>
                  ))}
                </tbody>
                <tfoot>
                  <tr>
                    <td>Total charges</td>
                    <td className="num">
                      {formatEuros(pack.compteResultat.totalChargesCents)}
                    </td>
                  </tr>
                  <tr>
                    <td>
                      <strong>Résultat</strong>
                    </td>
                    <td className="num">
                      <strong>
                        {formatEuros(pack.compteResultat.resultatCents)}
                      </strong>
                    </td>
                  </tr>
                </tfoot>
              </table>
            </div>
          </div>
        </div>
      )}

      {pack && tab === "annexe" && (
        <div className="stack">
          <section>
            <h2>Méthodes comptables</h2>
            <ul>
              {pack.annexe.methods.map((m) => (
                <li key={m}>{m}</li>
              ))}
            </ul>
          </section>
          <section>
            <h2>Immobilisations</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Code</th>
                    <th>Libellé</th>
                    <th>Compte</th>
                    <th className="num">Brut</th>
                    <th className="num">Amort.</th>
                    <th className="num">Net</th>
                    <th>Durée</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.annexe.immobilisations.map((a) => (
                    <tr key={a.code}>
                      <td>{a.code}</td>
                      <td>{a.label}</td>
                      <td>{a.accountNumber}</td>
                      <td className="num">{formatEuros(a.grossCents)}</td>
                      <td className="num">
                        {formatEuros(a.cumulativeAmortCents)}
                      </td>
                      <td className="num">{formatEuros(a.netCents)}</td>
                      <td>
                        {a.usefulLifeYears} ans ({a.method})
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
          <section>
            <h2>Dettes</h2>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Compte</th>
                    <th>Libellé</th>
                    <th>Nature</th>
                    <th className="num">Montant</th>
                  </tr>
                </thead>
                <tbody>
                  {pack.annexe.dettes.map((d) => (
                    <tr key={d.accountNumber}>
                      <td>{d.accountNumber}</td>
                      <td>{d.label}</td>
                      <td>{d.nature}</td>
                      <td className="num">{formatEuros(d.amountCents)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

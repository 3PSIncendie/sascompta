import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { sortLivreJournal } from "../lib/livreJournal";
import { formatEuros } from "../lib/money";
import type { Journal, LedgerFilter, LedgerMovement } from "../lib/types";

export function JournalsScreen() {
  const [tab, setTab] = useState<"livre" | "codes">("livre");
  const [journals, setJournals] = useState<Journal[]>([]);
  const [fromDate, setFromDate] = useState("2026-01-01");
  const [toDate, setToDate] = useState("2026-12-31");
  const [journalCode, setJournalCode] = useState("");
  const [movements, setMovements] = useState<LedgerMovement[]>([]);
  const [draftCount, setDraftCount] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .listJournals()
      .then(setJournals)
      .catch((e) => setError(String(e)));
  }, []);

  function filter(): LedgerFilter {
    return {
      fromDate,
      toDate,
      journalCode: journalCode || null,
      accountNumber: null,
    };
  }

  async function refresh() {
    setError(null);
    try {
      const [m, entries] = await Promise.all([
        api.ledgerMovements(filter()),
        api.listEntries(),
      ]);
      setMovements(sortLivreJournal(m));
      setDraftCount(entries.filter((e) => e.status === "draft").length);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const totals = useMemo(() => {
    let debit = 0;
    let credit = 0;
    for (const m of movements) {
      debit += m.debitCents;
      credit += m.creditCents;
    }
    return { debit, credit };
  }, [movements]);

  async function exportCsv() {
    const path = await save({
      defaultPath: journalCode
        ? `livre-journal-${journalCode}.csv`
        : "livre-journal.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportLedgerCsv(filter(), String(path));
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="panel">
      <h1>Livre journal</h1>
      <p>
        Mouvements des écritures <strong>validées</strong>, classés par journal
        puis date. Les brouillons (import confirmé, facture émise, saisie)
        restent dans Écritures jusqu’à validation.
      </p>

      <div className="tabs">
        <button
          type="button"
          className={tab === "livre" ? "active" : ""}
          onClick={() => setTab("livre")}
        >
          Livre journal
        </button>
        <button
          type="button"
          className={tab === "codes" ? "active" : ""}
          onClick={() => setTab("codes")}
        >
          Codes journal
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      {tab === "livre" && (
        <>
          {draftCount > 0 && (
            <aside className="help-callout" style={{ marginBottom: "1rem" }}>
              <strong>{draftCount} brouillon(s) en attente</strong>
              <p>
                Présents dans Écritures mais exclus du livre journal tant qu’ils
                ne sont pas validés (bouton « Valider »).
              </p>
            </aside>
          )}

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
            <label>
              Journal
              <select
                value={journalCode}
                onChange={(e) => setJournalCode(e.target.value)}
              >
                <option value="">Tous</option>
                {journals.map((j) => (
                  <option key={j.code} value={j.code}>
                    {j.code} — {j.label}
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="primary"
              onClick={() => void refresh()}
            >
              Actualiser
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() => void exportCsv()}
            >
              Export CSV
            </button>
          </div>

          <div className="summary-strip">
            <span>
              Lignes <strong>{movements.length}</strong>
            </span>
            <span>
              Total débit <strong>{formatEuros(totals.debit)}</strong>
            </span>
            <span>
              Total crédit <strong>{formatEuros(totals.credit)}</strong>
            </span>
          </div>

          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Date</th>
                  <th>Jnl</th>
                  <th>N° écriture</th>
                  <th>Pièce</th>
                  <th>Compte</th>
                  <th>Libellé</th>
                  <th>Tiers</th>
                  <th className="num">Débit</th>
                  <th className="num">Crédit</th>
                </tr>
              </thead>
              <tbody>
                {movements.length === 0 ? (
                  <tr>
                    <td colSpan={9} className="muted">
                      Aucun mouvement validé sur la période
                      {journalCode ? ` / journal ${journalCode}` : ""}.
                      {draftCount > 0
                        ? ` ${draftCount} brouillon(s) existent — validez-les dans Écritures.`
                        : ""}
                    </td>
                  </tr>
                ) : (
                  movements.map((m, i) => (
                    <tr key={`${m.entryId}-${m.accountNumber}-${i}`}>
                      <td>{m.entryDate}</td>
                      <td>{m.journalCode}</td>
                      <td>{m.ecritureNum ?? "—"}</td>
                      <td>{m.pieceRef}</td>
                      <td>{m.accountNumber}</td>
                      <td>{m.lineLabel ?? m.entryLabel}</td>
                      <td>{m.thirdPartyCode ?? ""}</td>
                      <td className="num">{formatEuros(m.debitCents)}</td>
                      <td className="num">{formatEuros(m.creditCents)}</td>
                    </tr>
                  ))
                )}
              </tbody>
            </table>
          </div>
        </>
      )}

      {tab === "codes" && (
        <>
          <p className="hint">
            Référentiel des codes (ACH, VTE, BQ, OD, AN). La consultation des
            mouvements se fait dans l’onglet Livre journal.
          </p>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Libellé</th>
                  <th>Type</th>
                </tr>
              </thead>
              <tbody>
                {journals.map((j) => (
                  <tr key={j.id}>
                    <td>
                      <strong>{j.code}</strong>
                    </td>
                    <td>{j.label}</td>
                    <td>{j.journalType}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      )}
    </div>
  );
}

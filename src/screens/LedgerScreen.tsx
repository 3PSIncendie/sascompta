import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import type {
  Account,
  BalanceReport,
  Journal,
  LedgerFilter,
  LedgerMovement,
} from "../lib/types";

export function LedgerScreen() {
  const [tab, setTab] = useState<"balance" | "ledger">("balance");
  const [journals, setJournals] = useState<Journal[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [fromDate, setFromDate] = useState("2026-01-01");
  const [toDate, setToDate] = useState("2026-12-31");
  const [journalCode, setJournalCode] = useState("");
  const [accountNumber, setAccountNumber] = useState("");
  const [balance, setBalance] = useState<BalanceReport | null>(null);
  const [movements, setMovements] = useState<LedgerMovement[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void Promise.all([api.listJournals(), api.listAccounts()])
      .then(([j, a]) => {
        setJournals(j);
        setAccounts(a);
      })
      .catch((e) => setError(String(e)));
  }, []);

  function filter(): LedgerFilter {
    return {
      fromDate,
      toDate,
      journalCode: journalCode || null,
      accountNumber: accountNumber || null,
    };
  }

  async function refresh() {
    setError(null);
    try {
      const f = filter();
      const [b, m] = await Promise.all([
        api.balanceReport(f),
        api.ledgerMovements(f),
      ]);
      setBalance(b);
      setMovements(m);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function exportCsv() {
    const path = await save({
      defaultPath: tab === "balance" ? "balance.csv" : "grand-livre.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      if (tab === "balance") {
        await api.exportBalanceCsv(filter(), String(path));
      } else {
        await api.exportLedgerCsv(filter(), String(path));
      }
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportPdf() {
    const path = await save({
      defaultPath: tab === "balance" ? "balance.pdf" : "grand-livre.pdf",
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    try {
      if (tab === "balance") {
        await api.exportBalancePdf(filter(), String(path), "Balance générale");
      } else {
        await api.exportLedgerPdf(filter(), String(path), "Grand livre");
      }
    } catch (e) {
      setError(String(e));
    }
  }

  return (
    <div className="panel">
      <h1>Grand livre</h1>
      <p>Consultation filtrée et exports CSV / PDF.</p>

      <div className="tabs">
        <button
          type="button"
          className={tab === "balance" ? "active" : ""}
          onClick={() => setTab("balance")}
        >
          Balance
        </button>
        <button
          type="button"
          className={tab === "ledger" ? "active" : ""}
          onClick={() => setTab("ledger")}
        >
          Grand livre
        </button>
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
        <label>
          Journal
          <select
            value={journalCode}
            onChange={(e) => setJournalCode(e.target.value)}
          >
            <option value="">Tous</option>
            {journals.map((j) => (
              <option key={j.code} value={j.code}>
                {j.code}
              </option>
            ))}
          </select>
        </label>
        <label>
          Compte
          <select
            value={accountNumber}
            onChange={(e) => setAccountNumber(e.target.value)}
          >
            <option value="">Tous</option>
            {accounts.map((a) => (
              <option key={a.number} value={a.number}>
                {a.number}
              </option>
            ))}
          </select>
        </label>
        <button type="button" className="primary" onClick={() => void refresh()}>
          Actualiser
        </button>
        <button type="button" className="ghost" onClick={() => void exportCsv()}>
          Export CSV
        </button>
        <button type="button" className="ghost" onClick={() => void exportPdf()}>
          Export PDF
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      {tab === "balance" && balance && (
        <>
          <div className="summary-strip">
            <span>
              Total débit <strong>{formatEuros(balance.totalDebitCents)}</strong>
            </span>
            <span>
              Total crédit <strong>{formatEuros(balance.totalCreditCents)}</strong>
            </span>
            <span>
              Écart{" "}
              <strong
                style={{
                  color:
                    balance.ecartCents === 0 ? "var(--ok)" : "var(--danger)",
                }}
              >
                {formatEuros(balance.ecartCents)}
              </strong>
            </span>
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Compte</th>
                  <th>Libellé</th>
                  <th className="num">Débit</th>
                  <th className="num">Crédit</th>
                  <th className="num">Solde D</th>
                  <th className="num">Solde C</th>
                </tr>
              </thead>
              <tbody>
                {balance.rows.map((r) => (
                  <tr key={r.accountNumber}>
                    <td>{r.accountNumber}</td>
                    <td>{r.accountLabel}</td>
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

      {tab === "ledger" && (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Date</th>
                <th>Jnl</th>
                <th>Pièce</th>
                <th>Compte</th>
                <th>Libellé</th>
                <th>Tiers</th>
                <th>TVA</th>
                <th className="num">Débit</th>
                <th className="num">Crédit</th>
              </tr>
            </thead>
            <tbody>
              {movements.map((m, i) => (
                <tr key={`${m.entryId}-${i}`}>
                  <td>{m.entryDate}</td>
                  <td>{m.journalCode}</td>
                  <td>{m.pieceRef}</td>
                  <td>{m.accountNumber}</td>
                  <td>{m.lineLabel ?? m.entryLabel}</td>
                  <td>{m.thirdPartyCode ?? ""}</td>
                  <td>{m.vatCode ?? ""}</td>
                  <td className="num">{formatEuros(m.debitCents)}</td>
                  <td className="num">{formatEuros(m.creditCents)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

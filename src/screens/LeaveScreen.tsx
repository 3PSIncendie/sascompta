import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { isTauriRuntime } from "../lib/updater";
import {
  formatDaysBp,
  LEAVE_STATUS_LABELS,
  parseDaysToBp,
} from "../modules/hr/format";
import {
  previewEmployees,
  previewLeaveBalances,
  previewLeaveRequests,
} from "../modules/hr/preview";
import type { Employee, LeaveBalance, LeaveRequest } from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

export function LeaveScreen() {
  const year = 2026;
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [balances, setBalances] = useState<LeaveBalance[]>([]);
  const [requests, setRequests] = useState<LeaveRequest[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [form, setForm] = useState({
    employeeId: 0,
    fromDate: "2026-08-01",
    toDate: "2026-08-05",
    days: "5",
    notes: "",
  });

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      setEmployees(previewEmployees());
      setBalances(previewLeaveBalances());
      setRequests(previewLeaveRequests());
      return;
    }
    try {
      const [emps, bals, reqs] = await Promise.all([
        api.listEmployees(),
        api.listLeaveBalances(year),
        api.listLeaveRequests(null),
      ]);
      setEmployees(emps);
      setBalances(bals);
      setRequests(reqs);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function onCreate(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : demande simulée.");
        return;
      }
      if (!form.employeeId) {
        setError("Choisissez un salarié");
        return;
      }
      const req = await api.createLeaveRequest({
        employeeId: form.employeeId,
        fromDate: form.fromDate,
        toDate: form.toDate,
        daysBp: parseDaysToBp(form.days),
        notes: form.notes || null,
      });
      setStatus(`Demande #${req.id} créée (en attente)`);
      setForm({ ...form, notes: "" });
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onSetStatus(id: number, next: string) {
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus(`Aperçu : statut → ${next}`);
        return;
      }
      const req = await api.setLeaveStatus(id, next);
      setStatus(
        `Demande #${req.id} : ${LEAVE_STATUS_LABELS[req.status] ?? req.status}`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <header className="page-head">
        <div>
          <p className="eyebrow">RH</p>
          <h1>Congés</h1>
          <p>
            Soldes {year}, pose et validation simple. Acquisition forfaitaire
            (25&nbsp;j) — règles complexes plus tard.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="ok">{status}</p>}

      <section className="panel">
        <h2>Soldes {year}</h2>
        <table>
          <thead>
            <tr>
              <th>Salarié</th>
              <th>Acquis</th>
              <th>Pris</th>
              <th>Restant</th>
            </tr>
          </thead>
          <tbody>
            {balances.map((b) => (
              <tr key={b.employeeId}>
                <td>
                  {b.employeeCode} — {b.employeeName}
                </td>
                <td>{formatDaysBp(b.acquiredDaysBp)}</td>
                <td>{formatDaysBp(b.takenDaysBp)}</td>
                <td>
                  <strong>{formatDaysBp(b.remainingDaysBp)}</strong>
                </td>
              </tr>
            ))}
            {balances.length === 0 && (
              <tr>
                <td colSpan={4}>Aucun solde — créez d’abord un salarié.</td>
              </tr>
            )}
          </tbody>
        </table>
      </section>

      <div className="grid-2">
        <section className="panel">
          <h2>Demandes</h2>
          <table>
            <thead>
              <tr>
                <th>Salarié</th>
                <th>Période</th>
                <th>Jours</th>
                <th>Statut</th>
                <th />
              </tr>
            </thead>
            <tbody>
              {requests.map((r) => (
                <tr key={r.id}>
                  <td>
                    {r.employeeCode}
                    <br />
                    <small>{r.employeeName}</small>
                  </td>
                  <td>
                    {r.fromDate} → {r.toDate}
                    {r.notes ? (
                      <>
                        <br />
                        <small>{r.notes}</small>
                      </>
                    ) : null}
                  </td>
                  <td>{formatDaysBp(r.daysBp)}</td>
                  <td>{LEAVE_STATUS_LABELS[r.status] ?? r.status}</td>
                  <td>
                    {r.status === "pending" && (
                      <div className="toolbar">
                        <button
                          type="button"
                          disabled={busy}
                          onClick={() => void onSetStatus(r.id, "approved")}
                        >
                          Approuver
                        </button>
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void onSetStatus(r.id, "refused")}
                        >
                          Refuser
                        </button>
                      </div>
                    )}
                    {r.status === "approved" && (
                      <button
                        type="button"
                        className="ghost"
                        disabled={busy}
                        onClick={() => void onSetStatus(r.id, "cancelled")}
                      >
                        Annuler
                      </button>
                    )}
                  </td>
                </tr>
              ))}
              {requests.length === 0 && (
                <tr>
                  <td colSpan={5}>Aucune demande.</td>
                </tr>
              )}
            </tbody>
          </table>
        </section>

        <section className="panel">
          <h2>Nouvelle demande</h2>
          <form className="stack" onSubmit={(e) => void onCreate(e)}>
            <label className="field">
              Salarié
              <select
                value={form.employeeId}
                onChange={(e) =>
                  setForm({ ...form, employeeId: Number(e.target.value) })
                }
                required
              >
                <option value={0}>— choisir —</option>
                {employees
                  .filter((emp) => emp.active)
                  .map((emp) => (
                    <option key={emp.id} value={emp.id}>
                      {emp.code} — {emp.firstName} {emp.lastName}
                    </option>
                  ))}
              </select>
            </label>
            <div className="grid-2">
              <label className="field">
                Du
                <input
                  type="date"
                  value={form.fromDate}
                  onChange={(e) =>
                    setForm({ ...form, fromDate: e.target.value })
                  }
                  required
                />
              </label>
              <label className="field">
                Au
                <input
                  type="date"
                  value={form.toDate}
                  onChange={(e) =>
                    setForm({ ...form, toDate: e.target.value })
                  }
                  required
                />
              </label>
            </div>
            <label className="field">
              Jours ouvrés
              <input
                value={form.days}
                onChange={(e) => setForm({ ...form, days: e.target.value })}
                required
              />
            </label>
            <label className="field">
              Notes
              <textarea
                rows={2}
                value={form.notes}
                onChange={(e) => setForm({ ...form, notes: e.target.value })}
              />
            </label>
            <button type="submit" disabled={busy}>
              Poser la demande
            </button>
          </form>
        </section>
      </div>
    </div>
  );
}

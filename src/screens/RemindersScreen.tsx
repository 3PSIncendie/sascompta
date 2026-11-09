import { useEffect, useState } from "react";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  previewReminders,
  reminderLevelLabel,
} from "../modules/billing/payments";
import type { Reminder } from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

export function RemindersScreen() {
  const [reminders, setReminders] = useState<Reminder[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [selected, setSelected] = useState<Reminder | null>(null);

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      setReminders(previewReminders());
      return;
    }
    try {
      setReminders(await api.listReminders());
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function onGenerate() {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("Génération relances (aperçu) — file déjà peuplée.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const created = await api.generateDueReminders();
      setStatus(
        created.length
          ? `${created.length} relance(s) créée(s) (J+7 / J+15 / J+30).`
          : "Aucune nouvelle relance — file à jour.",
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onMarkSent(id: number) {
    if (PREVIEW || !isTauriRuntime()) {
      setReminders((prev) =>
        prev.map((r) =>
          r.id === id ? { ...r, sentAt: new Date().toISOString() } : r,
        ),
      );
      setStatus("Relance marquée envoyée (aperçu).");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      await api.markReminderSent(id);
      setStatus("Relance marquée comme envoyée.");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const pending = reminders.filter((r) => !r.sentAt);
  const sent = reminders.filter((r) => r.sentAt);

  return (
    <div className="panel">
      <header className="panel-head">
        <div>
          <h1>Relances</h1>
          <p className="muted">
            File J+7 / J+15 / J+30 sur factures échues non soldées — envoi
            manuel / PDF (pas d’email automatique).
          </p>
        </div>
        <button
          type="button"
          className="primary"
          disabled={busy}
          onClick={() => void onGenerate()}
        >
          Générer les relances dues
        </button>
      </header>

      {error ? <p className="error">{error}</p> : null}
      {status ? <p className="hint">{status}</p> : null}

      <h2 className="section-title">À traiter ({pending.length})</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Niveau</th>
              <th>Facture</th>
              <th>Client</th>
              <th>Échéance</th>
              <th>Retard</th>
              <th className="num">Reste dû</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {pending.length === 0 ? (
              <tr>
                <td colSpan={7} className="muted">
                  Aucune relance en attente — générez la file ou soldez les
                  factures.
                </td>
              </tr>
            ) : (
              pending.map((r) => (
                <tr key={r.id}>
                  <td>
                    <span className={`pill-status status-reminder-${r.level}`}>
                      {reminderLevelLabel(r.level)}
                    </span>
                  </td>
                  <td>{r.invoiceNumber ?? `#${r.invoiceId}`}</td>
                  <td>
                    {r.thirdPartyCode} — {r.thirdPartyName}
                  </td>
                  <td>{r.dueDate ?? "—"}</td>
                  <td>{r.daysOverdue} j</td>
                  <td className="num">{formatEuros(r.amountDueCents)}</td>
                  <td>
                    <div className="btn-row tight">
                      <button
                        type="button"
                        className="ghost"
                        onClick={() => setSelected(r)}
                      >
                        Voir texte
                      </button>
                      <button
                        type="button"
                        className="primary"
                        disabled={busy}
                        onClick={() => void onMarkSent(r.id)}
                      >
                        Marquer envoyée
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <h2 className="section-title">Envoyées ({sent.length})</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Niveau</th>
              <th>Facture</th>
              <th>Client</th>
              <th>Envoyée le</th>
              <th className="num">Reste dû</th>
            </tr>
          </thead>
          <tbody>
            {sent.length === 0 ? (
              <tr>
                <td colSpan={5} className="muted">
                  Pas encore de relance envoyée.
                </td>
              </tr>
            ) : (
              sent.map((r) => (
                <tr key={r.id}>
                  <td>{reminderLevelLabel(r.level)}</td>
                  <td>{r.invoiceNumber ?? `#${r.invoiceId}`}</td>
                  <td>
                    {r.thirdPartyCode} — {r.thirdPartyName}
                  </td>
                  <td>{r.sentAt?.slice(0, 10) ?? "—"}</td>
                  <td className="num">{formatEuros(r.amountDueCents)}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {selected ? (
        <div className="panel nested-panel" style={{ marginTop: "1rem" }}>
          <header className="panel-head">
            <div>
              <h2>
                Texte — {reminderLevelLabel(selected.level)}{" "}
                {selected.invoiceNumber}
              </h2>
            </div>
            <button
              type="button"
              className="ghost"
              onClick={() => setSelected(null)}
            >
              Fermer
            </button>
          </header>
          <pre className="reminder-body">{selected.bodySnapshot}</pre>
        </div>
      ) : null}
    </div>
  );
}

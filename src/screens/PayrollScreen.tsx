import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  formatRateBp,
  PAYSLIP_STATUS_LABELS,
  parseRateToBp,
} from "../modules/hr/format";
import {
  previewPayslips,
  previewSocialRates,
} from "../modules/hr/preview";
import type { DsnExportRun, Payslip, SocialRateTable } from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

export function PayrollScreen() {
  const [periodYm, setPeriodYm] = useState("2026-03");
  const [payslips, setPayslips] = useState<Payslip[]>([]);
  const [rates, setRates] = useState<SocialRateTable[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dsn, setDsn] = useState<DsnExportRun | null>(null);

  const [rateForm, setRateForm] = useState({
    validFrom: "2026-01-01",
    employeeRate: "22",
    employerRate: "42",
    label: "Barème agrégé",
  });

  const selected = payslips.find((p) => p.id === selectedId) ?? null;

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      const slips = previewPayslips();
      setPayslips(slips);
      setRates(previewSocialRates());
      if (selectedId == null && slips[0]) setSelectedId(slips[0].id);
      return;
    }
    try {
      const [slips, social] = await Promise.all([
        api.listPayslips(periodYm),
        api.listSocialRates(),
      ]);
      setPayslips(slips);
      setRates(social);
      if (slips.length && !slips.some((p) => p.id === selectedId)) {
        setSelectedId(slips[0]?.id ?? null);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [periodYm]);

  async function onGenerate() {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : bulletins simulés pour la période.");
        return;
      }
      const slips = await api.generatePeriodPayslips(periodYm);
      setStatus(`${slips.length} bulletin(s) calculé(s) pour ${periodYm}`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onValidate(id: number) {
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : validation simulée.");
        return;
      }
      const p = await api.validatePayslip(id);
      setStatus(`Bulletin ${p.employeeCode} validé`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onBuildOd(id: number) {
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : OD brouillon simulée (641 / 421 / 431 / 645).");
        return;
      }
      const entry = await api.buildPayslipEntryDraft(id);
      setStatus(
        `OD brouillon #${entry.id} générée — à valider dans Écritures`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onSaveRates(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : barème simulé.");
        return;
      }
      await api.upsertSocialRate(
        rateForm.validFrom,
        parseRateToBp(rateForm.employeeRate),
        parseRateToBp(rateForm.employerRate),
        rateForm.label,
      );
      setStatus("Barème enregistré");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onExportDsn() {
    setBusy(true);
    setError(null);
    setDsn(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus(
          "Aperçu : export DSN-oriented (CSV + checklist) — dépôt net-entreprises = M10/M11.",
        );
        return;
      }
      const path = await save({
        defaultPath: `dsn-oriented-${periodYm}.csv`,
        filters: [{ name: "CSV", extensions: ["csv"] }],
      });
      if (!path) return;
      const run = await api.exportDsnOriented(periodYm, path);
      setDsn(run);
      setStatus(`Export DSN-oriented enregistré — ${path}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  let checklist: { code: string; ok: boolean; label: string }[] = [];
  if (dsn?.checklistJson) {
    try {
      checklist = JSON.parse(dsn.checklistJson) as typeof checklist;
    } catch {
      checklist = [];
    }
  }

  return (
    <div className="stack">
      <header className="page-head">
        <div>
          <p className="eyebrow">RH</p>
          <h1>Paie</h1>
          <p>
            Bulletins simplifiés (brut, cotisations agrégées, net), charges,
            écritures OD en brouillon. Export DSN-oriented assisté — pas un
            dépôt net-entreprises.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="ok">{status}</p>}

      <section className="panel">
        <div className="toolbar">
          <label className="field inline">
            Période
            <input
              type="month"
              value={periodYm}
              onChange={(e) => setPeriodYm(e.target.value)}
            />
          </label>
          <button type="button" disabled={busy} onClick={() => void onGenerate()}>
            Calculer la période
          </button>
          <button
            type="button"
            className="ghost"
            disabled={busy}
            onClick={() => void onExportDsn()}
          >
            Export DSN-oriented
          </button>
        </div>
        <p className="muted">
          Barème local versionné — à mettre à jour manuellement. Contrôle EC
          recommandé avant validation.
        </p>
      </section>

      <div className="grid-2">
        <section className="panel">
          <h2>Bulletins {periodYm}</h2>
          <table>
            <thead>
              <tr>
                <th>Salarié</th>
                <th>Brut</th>
                <th>Net</th>
                <th>Statut</th>
                <th>OD</th>
              </tr>
            </thead>
            <tbody>
              {payslips.map((p) => (
                <tr
                  key={p.id}
                  className={selectedId === p.id ? "selected" : ""}
                  onClick={() => setSelectedId(p.id)}
                >
                  <td>
                    {p.employeeCode}
                    <br />
                    <small>{p.employeeName}</small>
                  </td>
                  <td>{formatEuros(p.grossCents)}</td>
                  <td>{formatEuros(p.netCents)}</td>
                  <td>{PAYSLIP_STATUS_LABELS[p.status] ?? p.status}</td>
                  <td>{p.entryId ? `#${p.entryId}` : "—"}</td>
                </tr>
              ))}
              {payslips.length === 0 && (
                <tr>
                  <td colSpan={5}>
                    Aucun bulletin — lancez le calcul de période.
                  </td>
                </tr>
              )}
            </tbody>
          </table>
        </section>

        <section className="panel">
          <h2>Détail</h2>
          {!selected && <p className="muted">Sélectionnez un bulletin.</p>}
          {selected && (
            <div className="stack">
              <p>
                <strong>
                  {selected.employeeCode} — {selected.employeeName}
                </strong>
                <br />
                <small>
                  {selected.periodYm} ·{" "}
                  {PAYSLIP_STATUS_LABELS[selected.status] ?? selected.status}
                </small>
              </p>
              <table>
                <thead>
                  <tr>
                    <th>Ligne</th>
                    <th>Base</th>
                    <th>Taux</th>
                    <th>Salarié</th>
                    <th>Employeur</th>
                  </tr>
                </thead>
                <tbody>
                  {selected.lines.map((l) => (
                    <tr key={`${l.code}-${l.lineNo}`}>
                      <td>{l.label}</td>
                      <td>{formatEuros(l.baseCents)}</td>
                      <td>
                        {l.rateBp ? formatRateBp(l.rateBp) : "—"}
                      </td>
                      <td>{formatEuros(l.employeeCents)}</td>
                      <td>{formatEuros(l.employerCents)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
              <p>
                Cotis. salariales {formatEuros(selected.employeeChargesCents)} ·
                Cotis. patronales {formatEuros(selected.employerChargesCents)}
                {selected.pasCents > 0
                  ? ` · PAS ${formatEuros(selected.pasCents)}`
                  : ""}
              </p>
              <div className="toolbar">
                {selected.status === "draft" && (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void onValidate(selected.id)}
                  >
                    Valider le bulletin
                  </button>
                )}
                {selected.status === "validated" && (
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => void onBuildOd(selected.id)}
                  >
                    Générer OD brouillon
                  </button>
                )}
              </div>
            </div>
          )}
        </section>
      </div>

      <div className="grid-2">
        <section className="panel">
          <h2>Barème cotisations</h2>
          <table>
            <thead>
              <tr>
                <th>Valide depuis</th>
                <th>Libellé</th>
                <th>Payload</th>
              </tr>
            </thead>
            <tbody>
              {rates.map((r) => (
                <tr key={r.id}>
                  <td>{r.validFrom}</td>
                  <td>{r.label}</td>
                  <td>
                    <code>{r.payloadJson}</code>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
          <form className="stack" onSubmit={(e) => void onSaveRates(e)}>
            <div className="grid-2">
              <label className="field">
                Valide depuis
                <input
                  type="date"
                  value={rateForm.validFrom}
                  onChange={(e) =>
                    setRateForm({ ...rateForm, validFrom: e.target.value })
                  }
                />
              </label>
              <label className="field">
                Libellé
                <input
                  value={rateForm.label}
                  onChange={(e) =>
                    setRateForm({ ...rateForm, label: e.target.value })
                  }
                />
              </label>
              <label className="field">
                Taux salarial %
                <input
                  value={rateForm.employeeRate}
                  onChange={(e) =>
                    setRateForm({ ...rateForm, employeeRate: e.target.value })
                  }
                />
              </label>
              <label className="field">
                Taux patronal %
                <input
                  value={rateForm.employerRate}
                  onChange={(e) =>
                    setRateForm({ ...rateForm, employerRate: e.target.value })
                  }
                />
              </label>
            </div>
            <button type="submit" disabled={busy}>
              Enregistrer le barème
            </button>
          </form>
        </section>

        <section className="panel">
          <h2>DSN-oriented</h2>
          <p>
            Fichier plat CSV aligné sur les champs clés + checklist de
            contrôles. Le dépôt légal (norme DSN, authentification
            net-entreprises) reste hors MVP — jalons M10/M11.
          </p>
          {checklist.length > 0 && (
            <ul>
              {checklist.map((c) => (
                <li key={c.code}>
                  {c.ok ? "✓" : "✗"} {c.label}
                </li>
              ))}
            </ul>
          )}
        </section>
      </div>
    </div>
  );
}

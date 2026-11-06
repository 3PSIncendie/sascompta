import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import type {
  DepreciationRun,
  DepreciationScheduleRow,
  FixedAsset,
  GenerateDepreciationResult,
} from "../lib/types";

type Tab = "registre" | "plan" | "runs";

export function AssetsScreen() {
  const [tab, setTab] = useState<Tab>("registre");
  const [assets, setAssets] = useState<FixedAsset[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [schedule, setSchedule] = useState<DepreciationScheduleRow[]>([]);
  const [runs, setRuns] = useState<DepreciationRun[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [lastGen, setLastGen] = useState<GenerateDepreciationResult | null>(
    null,
  );

  const [form, setForm] = useState({
    code: "",
    label: "",
    accountNumber: "215",
    acquisitionDate: "2026-01-01",
    serviceDate: "2026-01-01",
    baseEuros: "5000.00",
    residualEuros: "0",
    years: "10",
  });

  async function refresh() {
    setError(null);
    try {
      const [a, r] = await Promise.all([
        api.listFixedAssets(),
        api.listDepreciationRuns(null),
      ]);
      setAssets(a);
      setRuns(r);
      if (a.length && selectedId == null) {
        setSelectedId(a[0].id);
      }
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (selectedId == null) {
      setSchedule([]);
      return;
    }
    void api
      .assetSchedule(selectedId)
      .then(setSchedule)
      .catch((e: unknown) => setError(String(e)));
  }, [selectedId]);

  function eurosToCents(s: string): number {
    const n = Number(s.replace(",", ".").replace(/\s/g, ""));
    if (!Number.isFinite(n)) return 0;
    return Math.round(n * 100);
  }

  async function onCreate(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    try {
      const asset = await api.createFixedAsset({
        code: form.code.trim(),
        label: form.label.trim(),
        accountNumber: form.accountNumber.trim() || "215",
        acquisitionDate: form.acquisitionDate,
        serviceDate: form.serviceDate,
        baseCents: eurosToCents(form.baseEuros),
        residualCents: eurosToCents(form.residualEuros),
        usefulLifeYears: Number(form.years) || 1,
        notes: null,
      });
      setSelectedId(asset.id);
      setStatus(`Bien ${asset.code} créé — dotation annuelle ${formatEuros(asset.annualDotationCents)}`);
      setForm({ ...form, code: "", label: "" });
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onGenerate() {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const result = await api.generateDepreciation(
        "2026",
        "2026-01-01",
        "2026-12-31",
      );
      setLastGen(result);
      setStatus(
        result.created.length
          ? `OD générée(s) : ${result.created.length} — total ${formatEuros(result.totalAmountCents)}`
          : `Rien à générer (${result.skipped.join(" ; ") || "déjà à jour"})`,
      );
      await refresh();
      setTab("runs");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function exportRegistre() {
    const path = await save({
      defaultPath: "registre-immos.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportAssetsCsv(String(path));
      setStatus(`Export registre : ${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  async function exportPlan() {
    if (selectedId == null) return;
    const path = await save({
      defaultPath: "plan-amortissement.csv",
      filters: [{ name: "CSV", extensions: ["csv"] }],
    });
    if (!path) return;
    try {
      await api.exportScheduleCsv(selectedId, String(path));
      setStatus(`Export plan : ${path}`);
    } catch (e) {
      setError(String(e));
    }
  }

  const selected = assets.find((a) => a.id === selectedId) ?? null;

  return (
    <div className="panel">
      <header className="panel-head">
        <div>
          <h1>Immobilisations</h1>
          <p className="muted">
            Registre, plan linéaire et génération d&apos;OD d&apos;amortissement.
          </p>
        </div>
        <div className="row gap">
          <button type="button" className="primary" disabled={busy} onClick={() => void onGenerate()}>
            Générer OD 2026
          </button>
        </div>
      </header>

      {error ? <p className="error">{error}</p> : null}
      {status ? <p className="hint">{status}</p> : null}
      {lastGen && lastGen.created.length > 0 ? (
        <p className="hint">
          Dernière génération : {lastGen.created.map((r) => `${r.assetCode} ${formatEuros(r.amountCents)} (${r.ecritureNum ?? "OD"})`).join(" · ")}
        </p>
      ) : null}

      <div className="tabs">
        {(
          [
            ["registre", "Registre"],
            ["plan", "Plan linéaire"],
            ["runs", "Dotations / OD"],
          ] as const
        ).map(([id, label]) => (
          <button
            key={id}
            type="button"
            className={tab === id ? "tab active" : "tab"}
            onClick={() => setTab(id)}
          >
            {label}
          </button>
        ))}
      </div>

      {tab === "registre" ? (
        <>
          <div className="row gap" style={{ marginBottom: "0.75rem" }}>
            <button type="button" className="ghost" onClick={() => void exportRegistre()}>
              Export CSV
            </button>
          </div>
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Libellé</th>
                  <th>Compte</th>
                  <th>Base</th>
                  <th>Durée</th>
                  <th>Dotation / an</th>
                  <th>Service</th>
                </tr>
              </thead>
              <tbody>
                {assets.map((a) => (
                  <tr
                    key={a.id}
                    className={selectedId === a.id ? "selected" : undefined}
                    onClick={() => setSelectedId(a.id)}
                    style={{ cursor: "pointer" }}
                  >
                    <td>
                      <code>{a.code}</code>
                    </td>
                    <td>{a.label}</td>
                    <td>{a.accountNumber}</td>
                    <td>{formatEuros(a.baseCents)}</td>
                    <td>{a.usefulLifeYears} ans</td>
                    <td>
                      <strong>{formatEuros(a.annualDotationCents)}</strong>
                    </td>
                    <td>{a.serviceDate}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>

          <form className="card form" onSubmit={onCreate} style={{ marginTop: "1rem" }}>
            <h3>Nouveau bien</h3>
            <div className="grid-2">
              <label>
                Code
                <input
                  value={form.code}
                  onChange={(e) => setForm({ ...form, code: e.target.value })}
                  required
                />
              </label>
              <label>
                Libellé
                <input
                  value={form.label}
                  onChange={(e) => setForm({ ...form, label: e.target.value })}
                  required
                />
              </label>
              <label>
                Compte immo
                <input
                  value={form.accountNumber}
                  onChange={(e) =>
                    setForm({ ...form, accountNumber: e.target.value })
                  }
                />
              </label>
              <label>
                Durée (années)
                <input
                  value={form.years}
                  onChange={(e) => setForm({ ...form, years: e.target.value })}
                  required
                />
              </label>
              <label>
                Base (€)
                <input
                  value={form.baseEuros}
                  onChange={(e) =>
                    setForm({ ...form, baseEuros: e.target.value })
                  }
                  required
                />
              </label>
              <label>
                Résidu (€)
                <input
                  value={form.residualEuros}
                  onChange={(e) =>
                    setForm({ ...form, residualEuros: e.target.value })
                  }
                />
              </label>
              <label>
                Acquisition
                <input
                  type="date"
                  value={form.acquisitionDate}
                  onChange={(e) =>
                    setForm({ ...form, acquisitionDate: e.target.value })
                  }
                />
              </label>
              <label>
                Mise en service
                <input
                  type="date"
                  value={form.serviceDate}
                  onChange={(e) =>
                    setForm({ ...form, serviceDate: e.target.value })
                  }
                />
              </label>
            </div>
            <button type="submit" className="primary" disabled={busy}>
              Ajouter au registre
            </button>
          </form>
        </>
      ) : null}

      {tab === "plan" ? (
        <>
          <div className="row gap" style={{ marginBottom: "0.75rem" }}>
            <label>
              Bien{" "}
              <select
                value={selectedId ?? ""}
                onChange={(e) =>
                  setSelectedId(e.target.value ? Number(e.target.value) : null)
                }
              >
                {assets.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.code} — {a.label}
                  </option>
                ))}
              </select>
            </label>
            <button type="button" className="ghost" onClick={() => void exportPlan()}>
              Export CSV
            </button>
          </div>
          {selected ? (
            <p className="hint">
              {selected.code} · base {formatEuros(selected.baseCents)} ·{" "}
              {selected.usefulLifeYears} ans · dotation annuelle{" "}
              <strong>{formatEuros(selected.annualDotationCents)}</strong>
            </p>
          ) : null}
          <div className="table-wrap">
            <table>
              <thead>
                <tr>
                  <th>#</th>
                  <th>Période</th>
                  <th>Dotation</th>
                  <th>Cumul</th>
                  <th>VNC</th>
                </tr>
              </thead>
              <tbody>
                {schedule.map((r) => (
                  <tr key={r.periodLabel}>
                    <td>{r.yearIndex}</td>
                    <td>{r.periodLabel}</td>
                    <td>{formatEuros(r.amountCents)}</td>
                    <td>{formatEuros(r.cumulativeCents)}</td>
                    <td>{formatEuros(r.netBookValueCents)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </>
      ) : null}

      {tab === "runs" ? (
        <div className="table-wrap">
          <table>
            <thead>
              <tr>
                <th>Période</th>
                <th>Bien</th>
                <th>Montant</th>
                <th>Journal</th>
                <th>Écriture</th>
              </tr>
            </thead>
            <tbody>
              {runs.length === 0 ? (
                <tr>
                  <td colSpan={5}>Aucune dotation générée.</td>
                </tr>
              ) : (
                runs.map((r) => (
                  <tr key={r.id}>
                    <td>{r.periodLabel}</td>
                    <td>
                      <code>{r.assetCode}</code> {r.assetLabel}
                    </td>
                    <td>{formatEuros(r.amountCents)}</td>
                    <td>{r.journalCode ?? "—"}</td>
                    <td>
                      <code>{r.ecritureNum ?? `#${r.entryId}`}</code>
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </div>
      ) : null}
    </div>
  );
}

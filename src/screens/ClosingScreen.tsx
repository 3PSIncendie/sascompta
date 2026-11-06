import { useEffect, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { formatEuros } from "../lib/money";
import type {
  ChecklistItem,
  CloseYearResult,
  ClosingChecklist,
  ClosingEventInfo,
  FiscalYear,
  PublicationPackMeta,
  WorkspaceInfo,
} from "../lib/types";

interface Props {
  workspace: WorkspaceInfo;
  onWorkspaceChange: (w: WorkspaceInfo) => void;
}

export function ClosingScreen({ workspace, onWorkspaceChange }: Props) {
  const [checklist, setChecklist] = useState<ClosingChecklist | null>(null);
  const [years, setYears] = useState<FiscalYear[]>([]);
  const [packYearId, setPackYearId] = useState<number>(workspace.fiscalYear.id);
  const [lastClose, setLastClose] = useState<ClosingEventInfo | null>(null);
  const [closeResult, setCloseResult] = useState<CloseYearResult | null>(null);
  const [packMeta, setPackMeta] = useState<PublicationPackMeta | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refresh() {
    setError(null);
    try {
      const [cl, ys, ev, info] = await Promise.all([
        api.closingChecklist(),
        api.listFiscalYears(),
        api.lastClosingEvent(),
        api.getWorkspaceInfo(),
      ]);
      setChecklist(cl);
      setYears(ys);
      setLastClose(ev);
      onWorkspaceChange(info);
      const closed = ys.find((y) => y.status === "closed");
      const preferred = closed?.id ?? info.fiscalYear.id;
      setPackYearId((prev) =>
        ys.some((y) => y.id === prev) ? prev : preferred,
      );
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function onCloseYear() {
    if (
      !window.confirm(
        "Clôturer l'exercice ? Les écritures de N passeront en lecture seule et les à-nouveaux seront générés sur N+1.",
      )
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      const res = await api.closeFiscalYear();
      setCloseResult(res);
      setStatus(
        `Exercice ${res.closedYear.label} clôturé → ${res.nextYear.label} ouvert (AN ${res.anEcritureNum ?? ""})`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onExportPack() {
    const y = years.find((x) => x.id === packYearId);
    const path = await save({
      defaultPath: `pack-comptes-annuels-${y?.label ?? "exercice"}.zip`,
      filters: [{ name: "ZIP", extensions: ["zip"] }],
    });
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      const meta = await api.exportPublicationZip(packYearId, String(path));
      setPackMeta(meta);
      setStatus(`Pack enregistré : ${path}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onExportFec() {
    const y = years.find((x) => x.id === packYearId) ?? workspace.fiscalYear;
    const path = await save({
      defaultPath: `FEC-brouillon-${y.label}.txt`,
      filters: [{ name: "FEC TSV", extensions: ["txt"] }],
    });
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      await api.exportFecDraft(
        {
          fromDate: y.startDate,
          toDate: y.endDate,
          journalCode: null,
          accountNumber: null,
        },
        String(path),
      );
      setStatus(`FEC brouillon exporté : ${path}`);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="panel">
      <header className="page-head">
        <div>
          <h1>Clôture &amp; publication</h1>
          <p className="muted">
            Checklist, à-nouveaux, verrouillage exercice, pack greffe (ZIP) et
            FEC brouillon (non certifié DGFiP).
          </p>
        </div>
        <div className="row-actions">
          <button type="button" className="ghost" onClick={() => void refresh()}>
            Actualiser
          </button>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="hint">{status}</p>}

      <section className="stack" style={{ gap: "1.25rem" }}>
        <div>
          <h2>Exercice courant</h2>
          <p>
            <strong>{workspace.fiscalYear.label}</strong> (
            {workspace.fiscalYear.startDate} → {workspace.fiscalYear.endDate}) —{" "}
            statut <strong>{workspace.fiscalYear.status}</strong>
            {checklist ? (
              <>
                {" "}
                · résultat {formatEuros(checklist.resultatCents)}
              </>
            ) : null}
          </p>
        </div>

        <div>
          <h2>Checklist de clôture</h2>
          {!checklist && <p className="muted">Chargement…</p>}
          {checklist && (
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>OK</th>
                    <th>Contrôle</th>
                    <th>Détail</th>
                  </tr>
                </thead>
                <tbody>
                  {checklist.items.map((it: ChecklistItem) => (
                    <tr key={it.code}>
                      <td>{it.ok ? "✓" : "✗"}</td>
                      <td>
                        <strong>{it.code}</strong> {it.label}
                      </td>
                      <td className="muted">{it.detail}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
          <div className="btn-row">
            <button
              type="button"
              className="primary"
              disabled={busy || !checklist?.allOk || workspace.fiscalYear.status === "closed"}
              onClick={() => void onCloseYear()}
            >
              Clôturer l&apos;exercice
            </button>
          </div>
          {closeResult && (
            <p className="hint">
              Clôturé {closeResult.closedYear.label} → ouvert{" "}
              {closeResult.nextYear.label} · AN{" "}
              {closeResult.anEcritureNum ?? closeResult.anEntryId} · résultat{" "}
              {formatEuros(closeResult.resultatCents)}
            </p>
          )}
          {lastClose && (
            <p className="muted small">
              Dernière clôture : {lastClose.closedAt}
              {lastClose.notes ? ` — ${lastClose.notes}` : ""}
              {lastClose.packPath ? ` · pack ${lastClose.packPath}` : ""}
            </p>
          )}
        </div>

        <div>
          <h2>Pack publication &amp; FEC</h2>
          <div className="row-actions" style={{ marginBottom: "0.75rem" }}>
            <label>
              Exercice
              <select
                value={packYearId}
                onChange={(e) => setPackYearId(Number(e.target.value))}
              >
                {years.map((y) => (
                  <option key={y.id} value={y.id}>
                    {y.label} ({y.status})
                  </option>
                ))}
              </select>
            </label>
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() => void onExportPack()}
            >
              Pack ZIP (PDF + CSV/JSON + FEC)
            </button>
            <button
              type="button"
              className="ghost"
              disabled={busy}
              onClick={() => void onExportFec()}
            >
              Export FEC brouillon
            </button>
          </div>
          {packMeta && (
            <p className="hint">
              Pack {packMeta.fiscalYearLabel} — {packMeta.organizationName} ·
              résultat {formatEuros(packMeta.resultatCents)} ·{" "}
              {packMeta.disclaimer}
            </p>
          )}
          <p className="muted small">
            Disclaimer : outil d&apos;aide au dépôt manuel greffe. Le FEC est un{" "}
            <strong>brouillon</strong> — non certifié DGFiP.
          </p>
        </div>

        <div>
          <h2>Cycle annuel (rappel)</h2>
          <ol className="muted">
            <li>Créer / ouvrir le workspace (mot de passe local).</li>
            <li>Saisir l&apos;exercice (journaux, TVA, immos).</li>
            <li>Contrôler balance / grand livre / états annuels.</li>
            <li>Clôturer → à-nouveaux N+1, N en lecture seule.</li>
            <li>Enregistrer le pack ZIP + FEC brouillon sur le disque.</li>
            <li>Déposer auprès du greffe (hors outil).</li>
          </ol>
        </div>
      </section>
    </div>
  );
}

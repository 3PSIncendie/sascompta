import { useEffect, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import { isTauriRuntime } from "../lib/updater";
import {
  phaseLabel,
  previewEinvoiceProfile,
  previewEinvoiceQueue,
  previewEreportingBatches,
  queueStatusLabel,
} from "../modules/billing/einvoice";
import type {
  EinvoicePartyProfile,
  EinvoiceQueueItem,
  EreportingBatch,
} from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

export function EInvoiceScreen() {
  const [profile, setProfile] = useState<EinvoicePartyProfile | null>(null);
  const [queue, setQueue] = useState<EinvoiceQueueItem[]>([]);
  const [batches, setBatches] = useState<EreportingBatch[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [reportKind, setReportKind] = useState("payments");
  const [periodFrom, setPeriodFrom] = useState("2026-01-01");
  const [periodTo, setPeriodTo] = useState("2026-12-31");

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      setProfile(previewEinvoiceProfile());
      setQueue(previewEinvoiceQueue());
      setBatches(previewEreportingBatches());
      return;
    }
    try {
      const [p, q, b] = await Promise.all([
        api.getEinvoiceProfile(),
        api.listEinvoiceQueue(),
        api.listEreportingBatches(),
      ]);
      setProfile(p);
      setQueue(q);
      setBatches(b);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function onImport() {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("Import aperçu — utilisez le desktop Tauri pour un fichier réel.");
      return;
    }
    const path = await open({
      multiple: false,
      filters: [{ name: "E-facture", extensions: ["xml", "pdf"] }],
    });
    if (!path || Array.isArray(path)) return;
    setBusy(true);
    setError(null);
    try {
      const item = await api.importEinvoiceFile(path);
      setStatus(item.summary ?? `Import #${item.id} OK`);
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onStubSubmit(id: number) {
    if (PREVIEW || !isTauriRuntime()) {
      setQueue((prev) =>
        prev.map((q) =>
          q.id === id
            ? {
                ...q,
                lifecycleStatus: "acked",
                attempts: q.attempts + 1,
                summary: "STUB PDP ack (aperçu)",
              }
            : q,
        ),
      );
      setStatus("Simulation PDP (aperçu) — aucun réseau.");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const item = await api.pdpStubSubmit(id);
      setStatus(item.summary ?? "Stub PDP OK");
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onEreporting() {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("E-reporting aperçu — export JSON/CSV disponible en desktop.");
      return;
    }
    const path = await save({
      defaultPath: `ereporting-${reportKind}.json`,
      filters: [
        { name: "JSON", extensions: ["json"] },
        { name: "CSV", extensions: ["csv"] },
      ],
    });
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      const batch = await api.buildEreportingBatch({
        periodFrom,
        periodTo,
        kind: reportKind,
        destPath: path,
      });
      setStatus(
        `Lot #${batch.id} exporté (${batch.entryCount} ligne(s)) — brouillon local, pas de dépôt.`,
      );
      await refresh();
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const outbound = queue.filter((q) => q.direction === "outbound");
  const inbound = queue.filter((q) => q.direction === "inbound");

  return (
    <div className="panel">
      <header className="panel-head">
        <div>
          <h1>E-facture</h1>
          <p className="muted">
            Factur-X local (CII BASIC + PDF), file d’attente et stub PDP —
            SasCompta n’est pas une PDP et n’envoie rien sur le réseau.
          </p>
        </div>
        <div className="btn-row" style={{ marginTop: 0 }}>
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void onImport()}
          >
            Importer XML
          </button>
        </div>
      </header>

      {error ? <p className="error">{error}</p> : null}
      {status ? <p className="hint">{status}</p> : null}

      {profile ? (
        <section className="einvoice-profile-card">
          <h2 className="section-title">Profil société (aperçu)</h2>
          <p className="muted small">
            Modifiez le détail (obligation, PDP stub) dans{" "}
            <strong>Ma SAS</strong>.
          </p>
          <div className="meta-grid">
            <div className="meta-item">
              <span>Phase</span>
              <strong>{phaseLabel(profile.obligationPhase)}</strong>
            </div>
            <div className="meta-item">
              <span>SIRET / routing</span>
              <strong>
                {profile.siret || "—"} · {profile.routingId || "—"}
              </strong>
            </div>
            <div className="meta-item">
              <span>PDP stub</span>
              <strong>
                {profile.pdpProvider || "non configuré"}{" "}
                {profile.pdpTechId ? `(${profile.pdpTechId})` : ""}
              </strong>
            </div>
            <div className="meta-item">
              <span>Calendrier indicatif</span>
              <strong>
                Réception {profile.obligationReceiveFrom || "—"} · Émission{" "}
                {profile.obligationEmitFrom || "—"}
              </strong>
            </div>
          </div>
        </section>
      ) : null}

      <h2 className="section-title">File outbound ({outbound.length})</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Id</th>
              <th>Facture</th>
              <th>Format</th>
              <th>Statut</th>
              <th>Résumé</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {outbound.length === 0 ? (
              <tr>
                <td colSpan={6} className="muted">
                  Aucun export — utilisez « Factur-X » sur une facture émise.
                </td>
              </tr>
            ) : (
              outbound.map((q) => (
                <tr key={q.id}>
                  <td>#{q.id}</td>
                  <td>{q.invoiceId ?? "—"}</td>
                  <td>{q.format}</td>
                  <td>
                    <span className={`pill-status status-ei-${q.lifecycleStatus}`}>
                      {queueStatusLabel(q.lifecycleStatus)}
                    </span>
                  </td>
                  <td className="small">{q.summary}</td>
                  <td>
                    {q.lifecycleStatus === "queued" ||
                    q.lifecycleStatus === "local_ready" ||
                    q.lifecycleStatus === "error" ? (
                      <button
                        type="button"
                        className="ghost"
                        disabled={busy}
                        onClick={() => void onStubSubmit(q.id)}
                      >
                        Stub PDP
                      </button>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <h2 className="section-title">File inbound ({inbound.length})</h2>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Id</th>
              <th>Format</th>
              <th>ACH</th>
              <th>Statut</th>
              <th>Résumé</th>
            </tr>
          </thead>
          <tbody>
            {inbound.length === 0 ? (
              <tr>
                <td colSpan={5} className="muted">
                  Aucun import — déposez un XML CII / UBL / Factur-X.
                </td>
              </tr>
            ) : (
              inbound.map((q) => (
                <tr key={q.id}>
                  <td>#{q.id}</td>
                  <td>{q.format}</td>
                  <td>{q.entryId ? `#${q.entryId}` : "—"}</td>
                  <td>{queueStatusLabel(q.lifecycleStatus)}</td>
                  <td className="small">{q.summary}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      <h2 className="section-title">E-reporting (brouillon)</h2>
      <div className="toolbar payment-bar">
        <label>
          Type
          <select
            value={reportKind}
            onChange={(e) => setReportKind(e.target.value)}
          >
            <option value="payments">Paiements</option>
            <option value="b2c">B2C</option>
            <option value="international">International</option>
          </select>
        </label>
        <label>
          Du
          <input
            type="date"
            value={periodFrom}
            onChange={(e) => setPeriodFrom(e.target.value)}
          />
        </label>
        <label>
          Au
          <input
            type="date"
            value={periodTo}
            onChange={(e) => setPeriodTo(e.target.value)}
          />
        </label>
        <button
          type="button"
          className="primary"
          disabled={busy}
          onClick={() => void onEreporting()}
        >
          Exporter JSON/CSV
        </button>
      </div>
      <p className="muted small">
        Export local uniquement — aucun dépôt PPF/PDP. Les dates d’obligation
        sont paramétrables et non juridiques.
      </p>
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Lot</th>
              <th>Période</th>
              <th>Type</th>
              <th>Statut</th>
              <th>Lignes</th>
            </tr>
          </thead>
          <tbody>
            {batches.length === 0 ? (
              <tr>
                <td colSpan={5} className="muted">
                  Aucun lot exporté.
                </td>
              </tr>
            ) : (
              batches.map((b) => (
                <tr key={b.id}>
                  <td>#{b.id}</td>
                  <td>
                    {b.periodFrom} → {b.periodTo}
                  </td>
                  <td>{b.kind}</td>
                  <td>{b.status}</td>
                  <td>{b.entryCount}</td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

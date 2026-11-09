import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { BillingLinesEditor } from "../components/BillingLinesEditor";
import { ThirdPartySelect } from "../components/ThirdPartySelect";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  BILLING_VAT_MODE_OPTIONS,
  buildCreateInput,
  emptyDraftLine,
  previewQuotes,
  QUOTE_STATUS_LABELS,
  quoteToDraftLines,
  summarizeQuoteLines,
  type DraftLine,
} from "../modules/billing/quotes";
import type {
  BillingVatMode,
  NavId,
  Quote,
  QuoteStatus,
  ThirdParty,
  VatCode,
} from "../lib/types";

type Mode = "list" | "edit";

const PREVIEW = typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");
const PREVIEW_EDIT =
  PREVIEW &&
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("quoteEdit") === "1";

function statusLabel(status: string): string {
  return QUOTE_STATUS_LABELS[status as QuoteStatus] ?? status;
}

export function QuotesScreen({
  onNavigate,
}: {
  onNavigate?: (id: NavId) => void;
}) {
  const [quotes, setQuotes] = useState<Quote[]>([]);
  const [customers, setCustomers] = useState<ThirdParty[]>([]);
  const [vatCodes, setVatCodes] = useState<VatCode[]>([]);
  const [mode, setMode] = useState<Mode>(PREVIEW_EDIT ? "edit" : "list");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [thirdPartyId, setThirdPartyId] = useState<number>(0);
  const [issueDate, setIssueDate] = useState("2026-03-15");
  const [validUntil, setValidUntil] = useState("2026-04-15");
  const [vatMode, setVatMode] = useState<BillingVatMode>("normale");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<DraftLine[]>([emptyDraftLine()]);

  const totals = useMemo(
    () =>
      summarizeQuoteLines(
        lines.map((l) => ({
          label: l.label,
          quantityMilli: Math.max(
            1,
            Math.round((Number(String(l.quantity).replace(",", ".")) || 0) * 1000),
          ),
          unit: l.unit,
          unitPriceCents: parseEurosToCents(l.unitPriceEuros),
          vatRateBp: l.vatRateBp,
          vatCode: l.vatCode,
          accountNumber: l.accountNumber,
        })),
        vatMode,
      ),
    [lines, vatMode],
  );

  const collectedCodes = useMemo(
    () =>
      vatCodes.filter((c) =>
        vatMode === "sans"
          ? c.direction === "exempt"
          : vatMode === "autoliquidation_chantier"
            ? c.direction === "reverse"
            : c.direction === "collected",
      ),
    [vatCodes, vatMode],
  );

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      setQuotes(previewQuotes());
      setCustomers([
        {
          id: 3,
          code: "C001",
          name: "Client Gamma",
          kind: "customer",
          accountNumber: "411",
          active: true,
          siret: "12345678200045",
          email: "contact@gamma.fr",
        },
        {
          id: 4,
          code: "C002",
          name: "Client Delta",
          kind: "customer",
          accountNumber: "411",
          active: true,
        },
      ]);
      setVatCodes([
        {
          code: "T20C",
          label: "TVA 20% collectée",
          rateBp: 2000,
          direction: "collected",
          active: true,
        },
        {
          code: "T10C",
          label: "TVA 10% collectée",
          rateBp: 1000,
          direction: "collected",
          active: true,
        },
        {
          code: "T20R",
          label: "Autoliquidation 20%",
          rateBp: 2000,
          direction: "reverse",
          active: true,
        },
        {
          code: "TEX",
          label: "Exonéré",
          rateBp: 0,
          direction: "exempt",
          active: true,
        },
      ]);
      return;
    }
    try {
      const [q, tiers, codes] = await Promise.all([
        api.listQuotes(),
        api.listThirdParties(),
        api.listVatCodes(),
      ]);
      setQuotes(q);
      setCustomers(tiers.filter((t) => t.kind === "customer" && t.active));
      setVatCodes(codes.filter((c) => c.active));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  function startCreate() {
    setEditingId(null);
    setThirdPartyId(customers[0]?.id ?? 0);
    setIssueDate(new Date().toISOString().slice(0, 10));
    setValidUntil("");
    setVatMode("normale");
    setNotes("");
    setLines([emptyDraftLine()]);
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  function startEdit(q: Quote) {
    setEditingId(q.id);
    setThirdPartyId(q.thirdPartyId);
    setIssueDate(q.issueDate);
    setValidUntil(q.validUntil ?? "");
    setVatMode((q.vatMode as BillingVatMode) || "normale");
    setNotes(q.notes ?? "");
    setLines(quoteToDraftLines(q).length ? quoteToDraftLines(q) : [emptyDraftLine()]);
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  async function onSave(e: React.FormEvent) {
    e.preventDefault();
    if (!thirdPartyId) {
      setError("Choisissez un client.");
      return;
    }
    const input = buildCreateInput({
      thirdPartyId,
      issueDate,
      validUntil,
      vatMode,
      notes,
      lines,
      parseEuros: parseEurosToCents,
    });
    if (!input.lines.length) {
      setError("Ajoutez au moins une ligne.");
      return;
    }
    if (PREVIEW || !isTauriRuntime()) {
      const fake: Quote = {
        id: editingId ?? Date.now(),
        number: editingId
          ? quotes.find((q) => q.id === editingId)?.number ?? "DEV-2026-PREVIEW"
          : `DEV-2026-${String(quotes.length + 1).padStart(4, "0")}`,
        thirdPartyId,
        thirdPartyCode:
          customers.find((c) => c.id === thirdPartyId)?.code ?? "Cxxx",
        thirdPartyName:
          customers.find((c) => c.id === thirdPartyId)?.name ?? "Client",
        status: editingId
          ? quotes.find((q) => q.id === editingId)?.status ?? "draft"
          : "draft",
        issueDate,
        validUntil: validUntil || null,
        currency: "EUR",
        htCents: totals.htCents,
        vatCents: totals.vatCents,
        ttcCents: totals.ttcCents,
        vatMode,
        notes: notes || null,
        legalMentions: input.legalMentions ?? null,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        lines: input.lines.map((l, i) => ({
          id: i + 1,
          quoteId: editingId ?? 0,
          lineNo: i + 1,
          label: l.label,
          quantityMilli: l.quantityMilli,
          unit: l.unit,
          unitPriceCents: l.unitPriceCents,
          htCents: l.htCents ?? 0,
          vatRateBp: l.vatRateBp,
          vatCode: l.vatCode ?? null,
          accountNumber: l.accountNumber,
        })),
      };
      setQuotes((prev) => {
        const others = prev.filter((q) => q.id !== fake.id);
        return [fake, ...others];
      });
      setStatus(`Devis ${fake.number} enregistré (aperçu).`);
      setMode("list");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = editingId
        ? await api.updateQuote(editingId, input)
        : await api.createQuote(input);
      setStatus(`Devis ${saved.number} enregistré.`);
      setMode("list");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function changeStatus(id: number, next: string) {
    if (PREVIEW || !isTauriRuntime()) {
      setQuotes((prev) =>
        prev.map((q) => (q.id === id ? { ...q, status: next } : q)),
      );
      setStatus(`Statut → ${statusLabel(next)} (aperçu).`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const q = await api.setQuoteStatus(id, next);
      setStatus(`Devis ${q.number} : ${statusLabel(q.status)}.`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDelete(id: number) {
    if (PREVIEW || !isTauriRuntime()) {
      setQuotes((prev) => prev.filter((q) => q.id !== id));
      return;
    }
    setBusy(true);
    try {
      await api.deleteQuote(id);
      setStatus("Devis brouillon supprimé.");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onPdf(q: Quote) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(
        `PDF devis ${q.number} — disponible dans l’application bureau (enregistrement fichier).`,
      );
      return;
    }
    const path = await save({
      defaultPath: `${q.number}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      await api.exportQuotePdf(q.id, String(path));
      setStatus(`PDF devis enregistré : ${path}`);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onConvert(q: Quote) {
    if (PREVIEW || !isTauriRuntime()) {
      setQuotes((prev) =>
        prev.map((x) => (x.id === q.id ? { ...x, status: "invoiced" } : x)),
      );
      setStatus(
        `Facture brouillon créée depuis ${q.number} (aperçu) — ouvrez Factures pour émettre.`,
      );
      onNavigate?.("factures");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const inv = await api.convertQuoteToInvoice(q.id);
      setStatus(
        `Facture brouillon #${inv.id} créée depuis ${q.number} — émettez-la dans Factures (VTE auto).`,
      );
      await refresh();
      onNavigate?.("factures");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  function onCustomerCreated(tier: ThirdParty) {
    setCustomers((prev) =>
      prev.some((c) => c.id === tier.id) ? prev : [...prev, tier],
    );
    setThirdPartyId(tier.id);
  }

  if (mode === "edit") {
    return (
      <div className="panel">
        <header className="panel-head">
          <div>
            <h1>{editingId ? "Modifier le devis" : "Nouveau devis"}</h1>
            <p className="muted">
              Lignes HT (titre + détails), régime TVA (dont chantier BTP) et
              totaux TTC.
            </p>
          </div>
          <button type="button" className="ghost" onClick={() => setMode("list")}>
            Retour liste
          </button>
        </header>

        {error ? <p className="error">{error}</p> : null}

        <form className="stack gap" onSubmit={(e) => void onSave(e)}>
          <div className="split-2">
            <label className="field">
              Client
              <ThirdPartySelect
                value={thirdPartyId || null}
                onChange={(id) => setThirdPartyId(id ?? 0)}
                tiers={customers}
                onCreated={onCustomerCreated}
                kind="customer"
                required
                emptyLabel="Choisir…"
              />
            </label>
            <label className="field">
              Régime TVA
              <select
                value={vatMode}
                onChange={(e) => {
                  const mode = e.target.value as BillingVatMode;
                  setVatMode(mode);
                  const first = vatCodes.find((c) =>
                    mode === "sans"
                      ? c.direction === "exempt"
                      : mode === "autoliquidation_chantier"
                        ? c.direction === "reverse"
                        : c.direction === "collected",
                  );
                  if (first) {
                    setLines((prev) =>
                      prev.map((l) => ({
                        ...l,
                        vatCode: first.code,
                        vatRateBp: first.rateBp,
                      })),
                    );
                  }
                }}
              >
                {BILLING_VAT_MODE_OPTIONS.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Date
              <input
                type="date"
                value={issueDate}
                onChange={(e) => setIssueDate(e.target.value)}
                required
              />
            </label>
            <label>
              Valable jusqu’au
              <input
                type="date"
                value={validUntil}
                onChange={(e) => setValidUntil(e.target.value)}
              />
            </label>
          </div>

          <p className="hint">
            {BILLING_VAT_MODE_OPTIONS.find((o) => o.id === vatMode)?.help}
          </p>

          <BillingLinesEditor
            lines={lines}
            onChange={setLines}
            vatCodes={collectedCodes}
            vatMode={vatMode}
          />

          <label>
            Notes
            <textarea
              rows={2}
              value={notes}
              onChange={(e) => setNotes(e.target.value)}
            />
          </label>

          <div className="summary-strip">
            <span>
              HT <strong>{formatEuros(totals.htCents)}</strong>
            </span>
            <span>
              TVA <strong>{formatEuros(totals.vatCents)}</strong>
            </span>
            <span>
              TTC <strong>{formatEuros(totals.ttcCents)}</strong>
            </span>
          </div>

          <div className="btn-row">
            <button type="submit" className="primary" disabled={busy}>
              Enregistrer
            </button>
          </div>
        </form>
      </div>
    );
  }

  return (
    <div className="panel">
      <header className="panel-head">
        <div>
          <h1>Devis</h1>
          <p className="muted">
            Parcours : PDF → valider → transformer en facture → encaisser →
            compta (VTE / BQ).
          </p>
        </div>
        <button type="button" className="primary" onClick={startCreate}>
          Nouveau devis
        </button>
      </header>

      <ol className="flow-steps" aria-label="Parcours devis">
        <li>
          <strong>1. PDF</strong> — imprimer / archiver
        </li>
        <li>
          <strong>2. Valider</strong> — statut accepté
        </li>
        <li>
          <strong>3. Facture</strong> — conversion sans re-saisie
        </li>
        <li>
          <strong>4. Compta</strong> — émission VTE puis encaissement BQ
        </li>
      </ol>

      {error ? <p className="error">{error}</p> : null}
      {status ? <p className="hint">{status}</p> : null}

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>N°</th>
              <th>Date</th>
              <th>Client</th>
              <th>Statut</th>
              <th className="num">HT</th>
              <th className="num">TTC</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {quotes.length === 0 ? (
              <tr>
                <td colSpan={7} className="muted">
                  Aucun devis — créez votre première proposition.
                </td>
              </tr>
            ) : (
              quotes.map((q) => (
                <tr key={q.id}>
                  <td>
                    <button
                      type="button"
                      className="linkish"
                      onClick={() => startEdit(q)}
                    >
                      {q.number}
                    </button>
                  </td>
                  <td>{q.issueDate}</td>
                  <td>
                    {q.thirdPartyCode} — {q.thirdPartyName}
                  </td>
                  <td>
                    <span className={`pill-status status-${q.status}`}>
                      {statusLabel(q.status)}
                    </span>
                  </td>
                  <td className="num">{formatEuros(q.htCents)}</td>
                  <td className="num">{formatEuros(q.ttcCents)}</td>
                  <td>
                    <div className="btn-row tight">
                      {(q.status === "draft" || q.status === "sent") && (
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => startEdit(q)}
                        >
                          Éditer
                        </button>
                      )}
                      <button
                        type="button"
                        className="secondary"
                        disabled={busy}
                        onClick={() => void onPdf(q)}
                        title="Enregistrer le devis en PDF"
                      >
                        Imprimer PDF
                      </button>
                      {q.status === "draft" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void changeStatus(q.id, "sent")}
                        >
                          Marquer envoyé
                        </button>
                      )}
                      {(q.status === "draft" || q.status === "sent") && (
                        <button
                          type="button"
                          className="primary"
                          disabled={busy}
                          onClick={() => void changeStatus(q.id, "accepted")}
                          title="Passe le devis en statut Accepté (validé)"
                        >
                          Valider
                        </button>
                      )}
                      {q.status === "accepted" && (
                        <button
                          type="button"
                          className="primary"
                          disabled={busy}
                          onClick={() => void onConvert(q)}
                          title="Crée une facture brouillon et ouvre Factures"
                        >
                          Transformer en facture
                        </button>
                      )}
                      {q.status === "invoiced" && (
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => onNavigate?.("factures")}
                        >
                          Voir factures
                        </button>
                      )}
                      {q.status === "draft" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void onDelete(q.id)}
                        >
                          Suppr.
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

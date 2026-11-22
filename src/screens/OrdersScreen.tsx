import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { BillingDocumentPreview } from "../components/BillingDocumentPreview";
import { BillingLinesEditor } from "../components/BillingLinesEditor";
import { ThirdPartySelect } from "../components/ThirdPartySelect";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  buildBillingDocumentPreview,
  draftPreviewNumber,
} from "../modules/billing/documentPreview";
import {
  BILLING_VAT_MODE_OPTIONS,
  buildCreateOrderInput,
  emptyDraftLine,
  emptyDraftLineForVatMode,
  ORDER_STATUS_LABELS,
  orderSourceHint,
  orderToDraftLines,
  previewOrders,
  summarizeQuoteLines,
  syncDraftLinesToVatMode,
  vatCodesForBillingMode,
  type DraftLine,
} from "../modules/billing/orders";
import { previewNumber } from "../modules/billing/sequences";
import { defaultBillingVatModeForOrg } from "../modules/billing/quotes";
import type {
  BillingVatMode,
  CustomerOrder,
  CustomerOrderStatus,
  NavId,
  Organization,
  ThirdParty,
  VatCode,
} from "../lib/types";

type Mode = "list" | "edit";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

function statusLabel(status: string): string {
  return ORDER_STATUS_LABELS[status as CustomerOrderStatus] ?? status;
}

export function OrdersScreen({
  organization,
}: {
  organization: Organization;
  onNavigate?: (id: NavId) => void;
}) {
  const [orders, setOrders] = useState<CustomerOrder[]>([]);
  const [customers, setCustomers] = useState<ThirdParty[]>([]);
  const [vatCodes, setVatCodes] = useState<VatCode[]>([]);
  const [mode, setMode] = useState<Mode>("list");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [thirdPartyId, setThirdPartyId] = useState<number>(0);
  const [issueDate, setIssueDate] = useState("2026-03-15");
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
    () => vatCodesForBillingMode(vatMode, vatCodes),
    [vatCodes, vatMode],
  );

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      setOrders(previewOrders());
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
      const [list, tiers, codes] = await Promise.all([
        api.listCustomerOrders(),
        api.listThirdParties(),
        api.listVatCodes(),
      ]);
      setOrders(list);
      setCustomers(tiers.filter((t) => t.kind === "customer" && t.active));
      setVatCodes(codes.filter((c) => c.active));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    if (mode !== "edit" || !vatCodes.length) return;
    setLines((prev) => {
      const next = syncDraftLinesToVatMode(prev, vatMode, vatCodes);
      const unchanged = next.every(
        (l, i) =>
          l.vatCode === prev[i]?.vatCode && l.vatRateBp === prev[i]?.vatRateBp,
      );
      return unchanged ? prev : next;
    });
  }, [vatCodes, mode, vatMode]);

  function startCreate() {
    const modeVat = defaultBillingVatModeForOrg(organization.vatRegime);
    setEditingId(null);
    setThirdPartyId(customers[0]?.id ?? 0);
    setIssueDate(new Date().toISOString().slice(0, 10));
    setVatMode(modeVat);
    setNotes("");
    setLines([emptyDraftLineForVatMode(modeVat, vatCodes)]);
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  function startEdit(o: CustomerOrder) {
    const modeVat = (o.vatMode as BillingVatMode) || "normale";
    const draft = orderToDraftLines(o);
    setEditingId(o.id);
    setThirdPartyId(o.thirdPartyId);
    setIssueDate(o.issueDate);
    setVatMode(modeVat);
    setNotes(o.notes ?? "");
    setLines(
      draft.length
        ? syncDraftLinesToVatMode(draft, modeVat, vatCodes)
        : [emptyDraftLineForVatMode(modeVat, vatCodes)],
    );
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  function onVatModeChange(next: BillingVatMode) {
    setVatMode(next);
    setLines((prev) => syncDraftLinesToVatMode(prev, next, vatCodes));
  }

  async function onSave(e: React.FormEvent) {
    e.preventDefault();
    if (!thirdPartyId) {
      setError("Choisissez un client.");
      return;
    }
    const input = buildCreateOrderInput({
      thirdPartyId,
      issueDate,
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
      const fake: CustomerOrder = {
        id: editingId ?? Date.now(),
        number: editingId
          ? (orders.find((o) => o.id === editingId)?.number ?? "C2026-03PREVIEW")
          : previewNumber(
              "CMD",
              (issueDate || "2026-03-01").slice(0, 4),
              (issueDate || "2026-03-01").slice(5, 7),
              orders.length + 1,
            ),
        thirdPartyId,
        thirdPartyCode:
          customers.find((c) => c.id === thirdPartyId)?.code ?? "Cxxx",
        thirdPartyName:
          customers.find((c) => c.id === thirdPartyId)?.name ?? "Client",
        status: editingId
          ? (orders.find((o) => o.id === editingId)?.status ?? "draft")
          : "draft",
        issueDate,
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
          orderId: editingId ?? 0,
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
      setOrders((prev) => {
        const others = prev.filter((o) => o.id !== fake.id);
        return [fake, ...others];
      });
      setStatus(`Commande ${fake.number} enregistrée (aperçu).`);
      setMode("list");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = editingId
        ? await api.updateCustomerOrder(editingId, input)
        : await api.createCustomerOrder(input);
      setStatus(`Commande ${saved.number} enregistrée.`);
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
      setOrders((prev) =>
        prev.map((o) => (o.id === id ? { ...o, status: next } : o)),
      );
      setStatus(`Statut → ${statusLabel(next)} (aperçu).`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const o = await api.setCustomerOrderStatus(id, next);
      setStatus(`Commande ${o.number} : ${statusLabel(o.status)}.`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDelete(id: number) {
    if (PREVIEW || !isTauriRuntime()) {
      setOrders((prev) => prev.filter((o) => o.id !== id));
      return;
    }
    setBusy(true);
    try {
      await api.deleteCustomerOrder(id);
      setStatus("Commande brouillon supprimée.");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onPdf(o: CustomerOrder) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(
        `PDF commande ${o.number} — disponible dans l’application bureau (enregistrement fichier).`,
      );
      return;
    }
    const path = await save({
      defaultPath: `${o.number}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      await api.exportCustomerOrderPdf(o.id, String(path));
      setStatus(`PDF commande enregistré : ${path}`);
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

  const selectedCustomer = useMemo(
    () => customers.find((c) => c.id === thirdPartyId) ?? null,
    [customers, thirdPartyId],
  );

  const existingNumber = useMemo(() => {
    if (editingId == null) return null;
    return orders.find((o) => o.id === editingId)?.number ?? null;
  }, [editingId, orders]);

  const livePreview = useMemo(
    () =>
      buildBillingDocumentPreview({
        docKind: "order",
        title: "Commande",
        number: draftPreviewNumber("CMD", issueDate, existingNumber),
        issueDate,
        secondaryDate: "",
        secondaryDateLabel: "",
        org: organization,
        client: selectedCustomer,
        vatMode,
        notes,
        lines,
      }),
    [
      existingNumber,
      issueDate,
      lines,
      notes,
      organization,
      selectedCustomer,
      vatMode,
    ],
  );

  if (mode === "edit") {
    return (
      <div className="panel">
        <header className="panel-head">
          <div>
            <h1>
              {editingId ? "Modifier la commande" : "Nouvelle commande"}
            </h1>
            <p className="muted">
              Saisie à gauche, aperçu PDF live à droite — aucune écriture
              comptable tant qu’aucune facture n’est émise.
            </p>
          </div>
          <button
            type="button"
            className="ghost"
            onClick={() => setMode("list")}
          >
            Retour liste
          </button>
        </header>

        {error ? <p className="error">{error}</p> : null}

        <div className="billing-edit-split">
          <form
            className="stack gap billing-edit-form"
            onSubmit={(e) => {
              if (e.target !== e.currentTarget) {
                e.preventDefault();
                e.stopPropagation();
                return;
              }
              void onSave(e);
            }}
          >
            <div className="split-2">
              <div className="field">
                <span className="field-caption">Client</span>
                <ThirdPartySelect
                  value={thirdPartyId || null}
                  onChange={(id) => setThirdPartyId(id ?? 0)}
                  tiers={customers}
                  onCreated={onCustomerCreated}
                  kind="customer"
                  required
                  emptyLabel="Choisir…"
                />
              </div>
              <label className="field">
                Régime TVA
                <select
                  value={vatMode}
                  onChange={(e) =>
                    onVatModeChange(e.target.value as BillingVatMode)
                  }
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

          <BillingDocumentPreview
            model={livePreview}
            label="Aperçu live de la commande"
          />
        </div>
      </div>
    );
  }

  return (
    <div className="panel">
      <header className="panel-head">
        <div>
          <h1>Commandes</h1>
          <p className="muted">
            Commandes clients : suivi commercial avant facture. Pas d’écriture
            au Grand livre sur la commande seule.
          </p>
        </div>
        <button type="button" className="primary" onClick={startCreate}>
          Nouvelle commande
        </button>
      </header>

      <ol className="flow-steps" aria-label="Parcours commande">
        <li>
          <strong>1. Saisir / importer</strong> — PDF ou saisie
        </li>
        <li>
          <strong>2. Envoyer</strong> — confirmation client
        </li>
        <li>
          <strong>3. Confirmer</strong> — commande ferme
        </li>
        <li>
          <strong>4. Facturer</strong> — dans Factures (VTE)
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
            {orders.length === 0 ? (
              <tr>
                <td colSpan={7} className="muted">
                  Aucune commande — créez-en une ou confirmez un import PDF.
                </td>
              </tr>
            ) : (
              orders.map((o) => (
                <tr key={o.id}>
                  <td>
                    <button
                      type="button"
                      className="linkish"
                      onClick={() => startEdit(o)}
                    >
                      {o.number}
                    </button>
                    {orderSourceHint(o.source) ? (
                      <span
                        className="muted"
                        title="Commande importée depuis Import docs"
                      >
                        {" "}
                        · {orderSourceHint(o.source)}
                      </span>
                    ) : null}
                  </td>
                  <td>{o.issueDate}</td>
                  <td>
                    {o.thirdPartyCode} — {o.thirdPartyName}
                  </td>
                  <td>
                    <span className={`pill-status status-${o.status}`}>
                      {statusLabel(o.status)}
                    </span>
                  </td>
                  <td className="num">{formatEuros(o.htCents)}</td>
                  <td className="num">{formatEuros(o.ttcCents)}</td>
                  <td>
                    <div className="btn-row tight">
                      {(o.status === "draft" || o.status === "sent") && (
                        <button
                          type="button"
                          className="ghost"
                          onClick={() => startEdit(o)}
                        >
                          Éditer
                        </button>
                      )}
                      <button
                        type="button"
                        className="secondary"
                        disabled={busy}
                        onClick={() => void onPdf(o)}
                        title="Enregistrer la commande en PDF"
                      >
                        Imprimer PDF
                      </button>
                      {o.status === "draft" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void changeStatus(o.id, "sent")}
                        >
                          Marquer envoyée
                        </button>
                      )}
                      {(o.status === "draft" || o.status === "sent") && (
                        <button
                          type="button"
                          className="primary"
                          disabled={busy}
                          onClick={() => void changeStatus(o.id, "confirmed")}
                        >
                          Confirmer
                        </button>
                      )}
                      {o.status !== "cancelled" && o.status !== "confirmed" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void changeStatus(o.id, "cancelled")}
                        >
                          Annuler
                        </button>
                      )}
                      {o.status === "confirmed" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void changeStatus(o.id, "cancelled")}
                        >
                          Annuler
                        </button>
                      )}
                      {o.status === "draft" && (
                        <button
                          type="button"
                          className="ghost"
                          disabled={busy}
                          onClick={() => void onDelete(o.id)}
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

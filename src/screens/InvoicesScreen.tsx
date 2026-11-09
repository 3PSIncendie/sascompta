import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { BillingLinesEditor } from "../components/BillingLinesEditor";
import { ThirdPartySelect } from "../components/ThirdPartySelect";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  BILLING_VAT_MODE_OPTIONS,
  type DraftLine,
} from "../modules/billing/quotes";
import {
  buildInvoiceCreateInput,
  defaultAccountForKind,
  emptyDraftLine,
  invoiceToDraftLines,
  kindLabel,
  previewInvoices,
  statusLabel,
  summarizeInvoiceDraft,
} from "../modules/billing/invoices";
import { PAYMENT_METHODS } from "../modules/billing/payments";
import type {
  BillingVatMode,
  Invoice,
  InvoiceKind,
  NavId,
  ThirdParty,
  VatCode,
} from "../lib/types";

type Mode = "list" | "edit";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");
const PREVIEW_EDIT =
  PREVIEW &&
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).get("invoiceEdit") === "1";

export function InvoicesScreen({
  onNavigate,
}: {
  onNavigate?: (id: NavId) => void;
}) {
  const [invoices, setInvoices] = useState<Invoice[]>([]);
  const [customers, setCustomers] = useState<ThirdParty[]>([]);
  const [vatCodes, setVatCodes] = useState<VatCode[]>([]);
  const [mode, setMode] = useState<Mode>(PREVIEW_EDIT ? "edit" : "list");
  const [editingId, setEditingId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [kind, setKind] = useState<InvoiceKind>("standard");
  const [thirdPartyId, setThirdPartyId] = useState(0);
  const [parentInvoiceId, setParentInvoiceId] = useState<number | null>(null);
  const [issueDate, setIssueDate] = useState("2026-04-01");
  const [dueDate, setDueDate] = useState("2026-04-30");
  const [serviceDate, setServiceDate] = useState("");
  const [vatMode, setVatMode] = useState<BillingVatMode>("normale");
  const [notes, setNotes] = useState("");
  const [lines, setLines] = useState<DraftLine[]>([
    emptyDraftLine(2000, "T20C"),
  ]);
  const [depositHt, setDepositHt] = useState("500.00");
  const [payInvoiceId, setPayInvoiceId] = useState<number | null>(null);
  const [payAmount, setPayAmount] = useState("");
  const [payMethod, setPayMethod] = useState("virement");
  const [payDate, setPayDate] = useState(
    () => new Date().toISOString().slice(0, 10),
  );
  const [payRef, setPayRef] = useState("");

  const totals = useMemo(
    () => summarizeInvoiceDraft(lines, vatMode, parseEurosToCents),
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
      setInvoices(previewInvoices());
      setCustomers([
        {
          id: 3,
          code: "C001",
          name: "Client Gamma",
          kind: "customer",
          accountNumber: "411",
          active: true,
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
      const [inv, tiers, codes] = await Promise.all([
        api.listInvoices(),
        api.listThirdParties(),
        api.listVatCodes(),
      ]);
      setInvoices(inv);
      setCustomers(tiers.filter((t) => t.kind === "customer" && t.active));
      setVatCodes(codes.filter((c) => c.active));
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  function startCreate(k: InvoiceKind = "standard") {
    setEditingId(null);
    setKind(k);
    setThirdPartyId(customers[0]?.id ?? 0);
    setParentInvoiceId(null);
    setIssueDate(new Date().toISOString().slice(0, 10));
    setDueDate("");
    setServiceDate("");
    setVatMode("normale");
    setNotes("");
    setLines([
      {
        ...emptyDraftLine(2000, "T20C"),
        accountNumber: defaultAccountForKind(k),
      },
    ]);
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  function startEdit(inv: Invoice) {
    if (inv.status !== "draft") {
      setError("Seuls les brouillons sont éditables.");
      return;
    }
    setEditingId(inv.id);
    setKind((inv.kind as InvoiceKind) || "standard");
    setThirdPartyId(inv.thirdPartyId);
    setParentInvoiceId(inv.parentInvoiceId);
    setIssueDate(inv.issueDate);
    setDueDate(inv.dueDate ?? "");
    setServiceDate(inv.serviceDate ?? "");
    setVatMode((inv.vatMode as BillingVatMode) || "normale");
    setNotes(inv.notes ?? "");
    setLines(
      invoiceToDraftLines(inv).length
        ? invoiceToDraftLines(inv)
        : [emptyDraftLine()],
    );
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
    const input = buildInvoiceCreateInput({
      kind,
      thirdPartyId,
      parentInvoiceId,
      issueDate,
      dueDate,
      serviceDate,
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
      const fake: Invoice = {
        id: editingId ?? Date.now(),
        number: null,
        kind,
        thirdPartyId,
        thirdPartyCode:
          customers.find((c) => c.id === thirdPartyId)?.code ?? "Cxxx",
        thirdPartyName:
          customers.find((c) => c.id === thirdPartyId)?.name ?? "Client",
        quoteId: null,
        parentInvoiceId,
        status: "draft",
        issueDate,
        dueDate: dueDate || null,
        serviceDate: serviceDate || null,
        htCents: totals.htCents,
        vatCents: totals.vatCents,
        ttcCents: totals.ttcCents,
        paidCents: 0,
        vatMode,
        vatCode: input.lines[0]?.vatCode ?? null,
        einvoiceProfile: "none",
        einvoiceStatus: "local_only",
        entryId: null,
        pdfPath: null,
        facturxPath: null,
        notes: notes || null,
        legalMentions: input.legalMentions ?? null,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
        lines: input.lines.map((l, i) => ({
          id: i + 1,
          invoiceId: editingId ?? 0,
          lineNo: i + 1,
          label: l.label,
          quantityMilli: l.quantityMilli,
          unit: l.unit,
          unitPriceCents: l.unitPriceCents,
          htCents: l.htCents ?? 0,
          vatRateBp: l.vatRateBp,
          vatCode: l.vatCode ?? null,
          accountNumber: l.accountNumber,
          analyticCode: null,
        })),
      };
      setInvoices((prev) => {
        const others = prev.filter((x) => x.id !== fake.id);
        return [fake, ...others];
      });
      setStatus("Facture brouillon enregistrée (aperçu).");
      setMode("list");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const saved = editingId
        ? await api.updateInvoice(editingId, input)
        : await api.createInvoice(input);
      setStatus(
        `${kindLabel(saved.kind)} brouillon #${saved.id} enregistrée.`,
      );
      setMode("list");
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onIssue(inv: Invoice) {
    if (PREVIEW || !isTauriRuntime()) {
      const num = `FACT-2026-${String(inv.id).padStart(4, "0")}`;
      setInvoices((prev) =>
        prev.map((x) =>
          x.id === inv.id
            ? { ...x, status: "issued", number: num, entryId: 99 }
            : x,
        ),
      );
      setStatus(`Émise ${num} + brouillon VTE (aperçu).`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const issued = await api.issueInvoice(inv.id);
      setStatus(
        `${issued.number} émise — brouillon VTE #${issued.entryId ?? "?"} créé. Validez-le pour passer en compta.`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onRebuildVte(inv: Invoice) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("Régénération du brouillon VTE (aperçu).");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const entry = await api.buildInvoiceEntryDraft(inv.id);
      setStatus(
        `Brouillon VTE #${entry.id} prêt (${entry.journalCode}). Validez pour poster en compta.`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onCreditNote(inv: Invoice) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(`Avoir brouillon créé sur ${inv.number ?? inv.id} (aperçu).`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const cn = await api.createCreditNote(inv.id);
      setStatus(`Avoir brouillon #${cn.id} créé.`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  function openPayment(inv: Invoice) {
    const due = Math.max(0, inv.ttcCents - inv.paidCents);
    setPayInvoiceId(inv.id);
    setPayAmount((due / 100).toFixed(2));
    setPayMethod("virement");
    setPayDate(new Date().toISOString().slice(0, 10));
    setPayRef("");
    setError(null);
  }

  async function onRecordPayment() {
    if (payInvoiceId == null) return;
    const amount = parseEurosToCents(payAmount);
    if (amount <= 0) {
      setError("Montant d'encaissement invalide.");
      return;
    }
    if (PREVIEW || !isTauriRuntime()) {
      setInvoices((prev) =>
        prev.map((x) => {
          if (x.id !== payInvoiceId) return x;
          const paid = x.paidCents + amount;
          return {
            ...x,
            paidCents: paid,
            status: paid >= x.ttcCents ? "paid" : "partially_paid",
          };
        }),
      );
      setStatus(
        `Encaissement ${formatEuros(amount)} — brouillon BQ + lettrage soft (aperçu).`,
      );
      setPayInvoiceId(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const p = await api.recordPayment({
        invoiceId: payInvoiceId,
        paidOn: payDate,
        amountCents: amount,
        method: payMethod,
        bankRef: payRef.trim() || null,
      });
      setStatus(
        `Encaissement #${p.id} — brouillon BQ #${p.entryId ?? "?"} · lettrage ${p.lettre ?? "—"}. Validez le BQ pour passer en compta.`,
      );
      setPayInvoiceId(null);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDeposit(inv: Invoice) {
    const ht = parseEurosToCents(depositHt);
    if (ht <= 0) {
      setError("Montant d'acompte invalide.");
      return;
    }
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(`Acompte ${formatEuros(ht)} HT créé (aperçu).`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const dep = await api.createDepositInvoice(
        inv.id,
        ht,
        2000,
        "T20C",
      );
      setStatus(`Acompte brouillon #${dep.id} (${formatEuros(dep.htCents)} HT).`);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onPdf(inv: Invoice) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("Export PDF disponible dans l’application bureau.");
      return;
    }
    const path = await save({
      defaultPath: `${inv.number ?? `facture-${inv.id}`}.pdf`,
      filters: [{ name: "PDF", extensions: ["pdf"] }],
    });
    if (!path) return;
    try {
      await api.exportInvoicePdf(inv.id, String(path));
      setStatus(`PDF exporté : ${path}`);
    } catch (err) {
      setError(String(err));
    }
  }

  async function onFacturx(inv: Invoice) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(
        "Factur-X BASIC (XML CII + PDF) — disponible en bureau ; file E-facture.",
      );
      return;
    }
    const dest = await open({
      directory: true,
      multiple: false,
      title: "Dossier d’export Factur-X",
    });
    if (!dest || Array.isArray(dest)) return;
    setBusy(true);
    setError(null);
    try {
      const q = await api.exportFacturx(inv.id, String(dest));
      setStatus(
        `Factur-X file #${q.id} — ${q.summary ?? "XML+PDF locaux (pas de dépôt réseau)"}`,
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDelete(id: number) {
    if (PREVIEW || !isTauriRuntime()) {
      setInvoices((prev) => prev.filter((x) => x.id !== id));
      return;
    }
    setBusy(true);
    try {
      await api.deleteInvoice(id);
      setStatus("Brouillon supprimé.");
      await refresh();
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

  async function onPostEntry(entryId: number, journalHint: string) {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus(
        `Validation ${journalHint} #${entryId} (aperçu) — en bureau : passage en compta.`,
      );
      onNavigate?.("ecritures");
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const current = await api.getEntry(entryId);
      if (current.status === "posted") {
        setStatus(
          `${journalHint} #${entryId} déjà en compta (${current.ecritureNum ?? "postée"}).`,
        );
        return;
      }
      const e = await api.postEntry(entryId);
      setStatus(
        `Écriture ${e.journalCode} ${e.ecritureNum ?? `#${e.id}`} validée en compta.`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  if (mode === "edit") {
    return (
      <div className="panel">
        <header className="panel-head">
          <div>
            <h1>
              {editingId
                ? `Modifier — ${kindLabel(kind)}`
                : `Nouveau — ${kindLabel(kind)}`}
            </h1>
            <p className="muted">
              Titre + détails par ligne ; l’émission réserve le numéro et
              prépare le brouillon VTE.
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
              Type
              <select
                value={kind}
                onChange={(e) => {
                  const k = e.target.value as InvoiceKind;
                  setKind(k);
                  setLines((prev) =>
                    prev.map((l) => ({
                      ...l,
                      accountNumber: defaultAccountForKind(k),
                    })),
                  );
                }}
                disabled={!!editingId}
              >
                <option value="standard">Facture</option>
                <option value="deposit">Acompte</option>
                <option value="balance">Solde</option>
                <option value="credit_note">Avoir</option>
              </select>
            </label>
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
            <label>
              Régime TVA
              <select
                value={vatMode}
                onChange={(e) => setVatMode(e.target.value as BillingVatMode)}
              >
                {BILLING_VAT_MODE_OPTIONS.map((o) => (
                  <option key={o.id} value={o.id}>
                    {o.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Facture liée (acompte / solde / avoir)
              <select
                value={parentInvoiceId ?? ""}
                onChange={(e) =>
                  setParentInvoiceId(
                    e.target.value ? Number(e.target.value) : null,
                  )
                }
              >
                <option value="">—</option>
                {invoices
                  .filter((i) => i.id !== editingId)
                  .map((i) => (
                    <option key={i.id} value={i.id}>
                      {i.number ?? `#${i.id}`} — {kindLabel(i.kind)}
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
              Échéance
              <input
                type="date"
                value={dueDate}
                onChange={(e) => setDueDate(e.target.value)}
              />
            </label>
          </div>

          <BillingLinesEditor
            lines={lines}
            onChange={setLines}
            vatCodes={collectedCodes}
            vatMode={vatMode}
            defaultAccount={defaultAccountForKind(kind)}
          />

          <label className="field">
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
              Enregistrer brouillon
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
          <h1>Factures</h1>
          <p className="muted">
            Émettre → valider VTE → encaisser → valider BQ. Création client et
            lignes titre+détail sans plafond.
          </p>
        </div>
        <div className="btn-row" style={{ marginTop: 0 }}>
          <button
            type="button"
            className="primary"
            onClick={() => startCreate("standard")}
          >
            Nouvelle facture
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => startCreate("deposit")}
          >
            Acompte
          </button>
        </div>
      </header>

      <ol className="flow-steps" aria-label="Parcours facture">
        <li>
          <strong>Émettre</strong> — numéro + PDF + brouillon VTE
        </li>
        <li>
          <strong>Valider VTE</strong> — passage en compta ventes
        </li>
        <li>
          <strong>Encaisser</strong> — brouillon BQ + lettrage 411
        </li>
        <li>
          <strong>Valider BQ</strong> — passage en compta banque
        </li>
      </ol>

      {error ? <p className="error">{error}</p> : null}
      {status ? (
        <p className="hint">
          {status}{" "}
          {onNavigate ? (
            <button
              type="button"
              className="linkish"
              onClick={() => onNavigate("ecritures")}
            >
              Ouvrir Écritures
            </button>
          ) : null}
        </p>
      ) : null}

      <div className="toolbar" style={{ marginBottom: "0.75rem" }}>
        <label>
          HT acompte rapide
          <input
            value={depositHt}
            onChange={(e) => setDepositHt(e.target.value)}
            style={{ maxWidth: "8rem" }}
          />
        </label>
        <span className="muted small">
          Puis « Acompte » sur une facture émise / brouillon marché.
        </span>
      </div>

      {payInvoiceId != null ? (
        <div className="toolbar payment-bar">
          <strong>Encaisser facture #{payInvoiceId}</strong>
          <label>
            Date
            <input
              type="date"
              value={payDate}
              onChange={(e) => setPayDate(e.target.value)}
            />
          </label>
          <label>
            Montant TTC
            <input
              value={payAmount}
              onChange={(e) => setPayAmount(e.target.value)}
              style={{ maxWidth: "8rem" }}
            />
          </label>
          <label>
            Mode
            <select
              value={payMethod}
              onChange={(e) => setPayMethod(e.target.value)}
            >
              {PAYMENT_METHODS.map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            Réf. banque
            <input
              value={payRef}
              onChange={(e) => setPayRef(e.target.value)}
              placeholder="VIR-…"
            />
          </label>
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void onRecordPayment()}
          >
            Enregistrer (BQ + lettrage)
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => setPayInvoiceId(null)}
          >
            Annuler
          </button>
        </div>
      ) : null}

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>N°</th>
              <th>Type</th>
              <th>Date</th>
              <th>Client</th>
              <th>Statut</th>
              <th className="num">TTC</th>
              <th className="num">Payé</th>
              <th>VTE</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {invoices.length === 0 ? (
              <tr>
                <td colSpan={9} className="muted">
                  Aucune facture — créez-en une ou transformez un devis accepté.
                </td>
              </tr>
            ) : (
              invoices.map((inv) => (
                <tr key={inv.id}>
                  <td>
                    {inv.number ?? (
                      <span className="muted">brouillon #{inv.id}</span>
                    )}
                  </td>
                  <td>{kindLabel(inv.kind)}</td>
                  <td>{inv.issueDate}</td>
                  <td>
                    {inv.thirdPartyCode} — {inv.thirdPartyName}
                  </td>
                  <td>
                    <span className={`pill-status status-${inv.status}`}>
                      {statusLabel(inv.status)}
                    </span>
                  </td>
                  <td className="num">{formatEuros(inv.ttcCents)}</td>
                  <td className="num">{formatEuros(inv.paidCents)}</td>
                  <td>
                    {inv.entryId ? (
                      <span className="hint">#{inv.entryId}</span>
                    ) : (
                      <span className="muted">—</span>
                    )}
                  </td>
                  <td>
                    <div className="btn-row tight">
                      {inv.status === "draft" && (
                        <>
                          <button
                            type="button"
                            className="ghost"
                            onClick={() => startEdit(inv)}
                          >
                            Éditer
                          </button>
                          <button
                            type="button"
                            className="primary"
                            disabled={busy}
                            onClick={() => void onIssue(inv)}
                          >
                            Émettre
                          </button>
                          <button
                            type="button"
                            className="ghost"
                            disabled={busy}
                            onClick={() => void onDelete(inv.id)}
                          >
                            Suppr.
                          </button>
                        </>
                      )}
                      {inv.status !== "draft" &&
                        inv.status !== "cancelled" && (
                          <>
                            {inv.entryId ? (
                              <button
                                type="button"
                                className="secondary"
                                disabled={busy}
                                onClick={() =>
                                  void onPostEntry(inv.entryId!, "VTE")
                                }
                                title="Valide le brouillon VTE en écriture postée"
                              >
                                Valider VTE
                              </button>
                            ) : (
                              <button
                                type="button"
                                className="ghost"
                                disabled={busy}
                                onClick={() => void onRebuildVte(inv)}
                              >
                                Créer VTE
                              </button>
                            )}
                            {inv.kind !== "credit_note" &&
                              inv.paidCents < inv.ttcCents && (
                                <button
                                  type="button"
                                  className="primary"
                                  disabled={busy}
                                  onClick={() => openPayment(inv)}
                                >
                                  Encaisser
                                </button>
                              )}
                            {inv.status === "paid" ||
                            inv.status === "partially_paid" ? (
                              <button
                                type="button"
                                className="ghost"
                                onClick={() => onNavigate?.("ecritures")}
                                title="Ouvre Écritures pour valider le brouillon BQ"
                              >
                                Voir BQ / compta
                              </button>
                            ) : null}
                            {inv.kind !== "credit_note" && (
                              <button
                                type="button"
                                className="ghost"
                                disabled={busy}
                                onClick={() => void onCreditNote(inv)}
                              >
                                Avoir
                              </button>
                            )}
                            {inv.kind !== "credit_note" && (
                              <button
                                type="button"
                                className="ghost"
                                disabled={busy}
                                onClick={() => void onDeposit(inv)}
                              >
                                Acompte
                              </button>
                            )}
                          </>
                        )}
                      <button
                        type="button"
                        className="secondary"
                        onClick={() => void onPdf(inv)}
                        title="Enregistrer la facture en PDF"
                      >
                        Imprimer PDF
                      </button>
                      {inv.status !== "draft" &&
                        inv.status !== "cancelled" && (
                          <button
                            type="button"
                            className="ghost"
                            disabled={busy}
                            onClick={() => void onFacturx(inv)}
                          >
                            Factur-X
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

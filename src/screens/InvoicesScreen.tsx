import { useEffect, useMemo, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { BillingDocumentPreview } from "../components/BillingDocumentPreview";
import { BillingLinesEditor } from "../components/BillingLinesEditor";
import { ThirdPartySelect } from "../components/ThirdPartySelect";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  buildBillingDocumentPreview,
  draftPreviewNumber,
  invoiceTitleForKind,
} from "../modules/billing/documentPreview";
import {
  BILLING_VAT_MODE_OPTIONS,
  defaultBillingVatModeForOrg,
  emptyDraftLine,
  emptyDraftLineForVatMode,
  syncDraftLinesToVatMode,
  vatCodesForBillingMode,
  type DraftLine,
} from "../modules/billing/quotes";
import {
  buildInvoiceCreateInput,
  defaultAccountForKind,
  invoiceSourceHint,
  invoiceToDraftLines,
  isImportedInvoice,
  kindLabel,
  previewInvoices,
  statusLabel,
  summarizeInvoiceDraft,
} from "../modules/billing/invoices";
import {
  INVOICE_NOTES_MODE_OPTIONS,
  initialInvoiceNotesMode,
  permanentInvoiceNotes,
  type InvoiceNotesMode,
} from "../modules/billing/invoiceNotes";
import { previewNumber } from "../modules/billing/sequences";
import {
  DEFAULT_FACTORING_FEE_ACCOUNT,
  factoringNetCents,
  normalizeBankRef,
  PAYMENT_METHODS,
} from "../modules/billing/payments";
import { LabelWithHelp } from "../components/FieldHelp";
import type {
  BillingVatMode,
  Invoice,
  InvoiceKind,
  NavId,
  Organization,
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
  organization,
  onOrganizationChange,
  onNavigate,
}: {
  organization: Organization;
  onOrganizationChange?: (organization: Organization) => void;
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
  const [notesMode, setNotesMode] = useState<InvoiceNotesMode>("permanent");
  const [purchaseOrderNumber, setPurchaseOrderNumber] = useState("");
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
  const [factorInvoiceId, setFactorInvoiceId] = useState<number | null>(null);
  const [factorAmount, setFactorAmount] = useState("");
  const [factorFee, setFactorFee] = useState("0.00");
  const [factorFeeAccount, setFactorFeeAccount] = useState(
    DEFAULT_FACTORING_FEE_ACCOUNT,
  );
  const [factorDate, setFactorDate] = useState(
    () => new Date().toISOString().slice(0, 10),
  );
  const [factorRef, setFactorRef] = useState("");

  const totals = useMemo(
    () => summarizeInvoiceDraft(lines, vatMode, parseEurosToCents),
    [lines, vatMode],
  );

  const collectedCodes = useMemo(
    () => vatCodesForBillingMode(vatMode, vatCodes),
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

  /** Quand le catalogue TVA arrive, aligne les codes hors régime (création précoce). */
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

  /** Mode `?invoiceEdit=1` : préremplit client + ligne démo pour l’aperçu live. */
  useEffect(() => {
    if (!PREVIEW_EDIT || customers.length === 0) return;
    if (thirdPartyId) return;
    setThirdPartyId(customers[0]!.id);
    setIssueDate("2026-04-01");
    setDueDate("2026-04-30");
    setNotes(
      permanentInvoiceNotes(organization) || "Prestation démonstration",
    );
    setNotesMode("permanent");
    const line = emptyDraftLine(2000, "T20C");
    line.label = "Prestation";
    line.description = "Intervention sur site";
    line.quantity = "1";
    line.unit = "u";
    line.unitPriceEuros = "1500,00";
    line.accountNumber = defaultAccountForKind(kind);
    setLines([line]);
  }, [customers, thirdPartyId, kind, organization]);

  function startCreate(k: InvoiceKind = "standard") {
    const mode = defaultBillingVatModeForOrg(organization.vatRegime);
    const permanent = permanentInvoiceNotes(organization);
    setEditingId(null);
    setKind(k);
    setThirdPartyId(customers[0]?.id ?? 0);
    setParentInvoiceId(null);
    setIssueDate(new Date().toISOString().slice(0, 10));
    setDueDate("");
    setServiceDate("");
    setVatMode(mode);
    setNotes(permanent);
    setNotesMode("permanent");
    setPurchaseOrderNumber("");
    setLines([
      emptyDraftLineForVatMode(mode, vatCodes, defaultAccountForKind(k)),
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
    const mode = (inv.vatMode as BillingVatMode) || "normale";
    const draft = invoiceToDraftLines(inv);
    const permanent = permanentInvoiceNotes(organization);
    setEditingId(inv.id);
    setKind((inv.kind as InvoiceKind) || "standard");
    setThirdPartyId(inv.thirdPartyId);
    setParentInvoiceId(inv.parentInvoiceId);
    setIssueDate(inv.issueDate);
    setDueDate(inv.dueDate ?? "");
    setServiceDate(inv.serviceDate ?? "");
    setVatMode(mode);
    setNotes(inv.notes ?? "");
    setNotesMode(initialInvoiceNotesMode(inv.notes, permanent));
    setPurchaseOrderNumber(inv.purchaseOrderNumber ?? "");
    setLines(
      draft.length
        ? syncDraftLinesToVatMode(draft, mode, vatCodes)
        : [
            emptyDraftLineForVatMode(
              mode,
              vatCodes,
              defaultAccountForKind((inv.kind as InvoiceKind) || "standard"),
            ),
          ],
    );
    setMode("edit");
    setStatus(null);
    setError(null);
  }

  function onVatModeChange(mode: BillingVatMode) {
    setVatMode(mode);
    setLines((prev) => syncDraftLinesToVatMode(prev, mode, vatCodes));
  }

  async function persistPermanentNotesIfNeeded() {
    if (notesMode !== "permanent") return;
    const next = notes.trim() || null;
    const current = permanentInvoiceNotes(organization) || null;
    if (next === current) return;
    if (PREVIEW || !isTauriRuntime()) {
      onOrganizationChange?.({
        ...organization,
        invoiceBillingNotes: next,
      });
      return;
    }
    const updated = await api.setInvoiceBillingNotes(next);
    onOrganizationChange?.(updated);
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
      purchaseOrderNumber,
      lines,
      parseEuros: parseEurosToCents,
    });
    if (!input.lines.length) {
      setError("Ajoutez au moins une ligne.");
      return;
    }
    if (PREVIEW || !isTauriRuntime()) {
      try {
        await persistPermanentNotesIfNeeded();
      } catch (err) {
        setError(String(err));
        return;
      }
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
        notes: notes.trim() || null,
        legalMentions: input.legalMentions ?? null,
        purchaseOrderNumber: purchaseOrderNumber.trim() || null,
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
      await persistPermanentNotesIfNeeded();
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
      const issue = inv.issueDate || "2026-04-01";
      const num = previewNumber(
        "FACT",
        issue.slice(0, 4),
        issue.slice(5, 7),
        inv.id,
      );
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
    setFactorInvoiceId(null);
    setPayInvoiceId(inv.id);
    setPayAmount((due / 100).toFixed(2));
    setPayMethod("virement");
    setPayDate(new Date().toISOString().slice(0, 10));
    setPayRef("");
    setError(null);
  }

  function openFactoring(inv: Invoice) {
    const due = Math.max(0, inv.ttcCents - inv.paidCents);
    setPayInvoiceId(null);
    setFactorInvoiceId(inv.id);
    setFactorAmount((due / 100).toFixed(2));
    setFactorFee("0.00");
    setFactorFeeAccount(DEFAULT_FACTORING_FEE_ACCOUNT);
    setFactorDate(new Date().toISOString().slice(0, 10));
    setFactorRef("");
    setError(null);
  }

  async function onRecordFactoring() {
    if (factorInvoiceId == null) return;
    const amount = parseEurosToCents(factorAmount);
    const fee = parseEurosToCents(factorFee);
    if (amount <= 0) {
      setError("Montant cédé invalide.");
      return;
    }
    if (fee < 0) {
      setError("Commission invalide.");
      return;
    }
    if (fee >= amount) {
      setError("La commission doit être strictement inférieure au montant cédé.");
      return;
    }
    const feeAccount = factorFeeAccount.trim() || DEFAULT_FACTORING_FEE_ACCOUNT;
    const net = factoringNetCents(amount, fee);
    if (PREVIEW || !isTauriRuntime()) {
      setInvoices((prev) =>
        prev.map((x) => {
          if (x.id !== factorInvoiceId) return x;
          const paid = x.paidCents + amount;
          return {
            ...x,
            paidCents: paid,
            status: paid >= x.ttcCents ? "paid" : "partially_paid",
          };
        }),
      );
      setStatus(
        `Affacturage ${formatEuros(amount)} (net ${formatEuros(net)}, commission ${formatEuros(fee)}) — OD 467/411 + BQ (aperçu).`,
      );
      setFactorInvoiceId(null);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const p = await api.recordFactoring({
        invoiceId: factorInvoiceId,
        paidOn: factorDate,
        amountCents: amount,
        feeCents: fee,
        feeAccount,
        bankRef: normalizeBankRef(factorRef),
      });
      setStatus(
        `Affacturage ${formatEuros(amount)} — OD #${p.cessionEntryId ?? "?"} (467/411) + BQ #${p.entryId ?? "?"} (512/${feeAccount}/467), lettrage ${p.lettre ?? "—"}.`,
      );
      setFactorInvoiceId(null);
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
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
        `Encaissement ${formatEuros(amount)} — BQ postée + lettrage soft (aperçu).`,
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
        bankRef: normalizeBankRef(payRef),
      });
      const refBit = p.bankRef ? ` · réf. ${p.bankRef}` : "";
      setStatus(
        `Encaissement #${p.id} — BQ postée #${p.entryId ?? "?"}${refBit} · lettrage ${p.lettre ?? "—"} (Grand livre / FEC à jour).`,
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

  const selectedCustomer = useMemo(
    () => customers.find((c) => c.id === thirdPartyId) ?? null,
    [customers, thirdPartyId],
  );

  const existingNumber = useMemo(() => {
    if (editingId == null) return null;
    return invoices.find((i) => i.id === editingId)?.number ?? null;
  }, [editingId, invoices]);

  const livePreview = useMemo(() => {
    const extraMeta: { label: string; value: string }[] = [];
    if (purchaseOrderNumber.trim()) {
      extraMeta.push({
        label: "Numéro de bon de commande",
        value: purchaseOrderNumber.trim(),
      });
    }
    if (parentInvoiceId != null) {
      extraMeta.push({ label: "Document lié", value: `#${parentInvoiceId}` });
    }
    return buildBillingDocumentPreview({
      docKind: "invoice",
      title: invoiceTitleForKind(kind),
      number: draftPreviewNumber("FACT", issueDate, existingNumber),
      issueDate,
      secondaryDate: dueDate,
      secondaryDateLabel: "Date d'échéance",
      org: organization,
      client: selectedCustomer,
      vatMode,
      notes,
      lines,
      extraMeta: extraMeta.length ? extraMeta : undefined,
    });
  }, [
    dueDate,
    existingNumber,
    issueDate,
    kind,
    lines,
    notes,
    organization,
    parentInvoiceId,
    purchaseOrderNumber,
    selectedCustomer,
    vatMode,
  ]);

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
              Saisie à gauche, aperçu PDF live à droite — le numéro définitif
              est réservé à l’émission (brouillon VTE).
            </p>
          </div>
          <button type="button" className="ghost" onClick={() => setMode("list")}>
            Retour liste
          </button>
        </header>

        {error ? <p className="error">{error}</p> : null}

        <div className="billing-edit-split">
          <form
            className="stack gap billing-edit-form"
            onSubmit={(e) => {
              // Ignore bubbled submit from a nested dialog form (webview reload risk).
              if (e.target !== e.currentTarget) {
                e.preventDefault();
                e.stopPropagation();
                return;
              }
              void onSave(e);
            }}
          >
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
              <label>
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
              <label>
                Numéro de bon de commande
                <input
                  type="text"
                  value={purchaseOrderNumber}
                  onChange={(e) => setPurchaseOrderNumber(e.target.value)}
                  placeholder="Ex. 4500418456"
                  autoComplete="off"
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

            <div className="invoice-notes-panel">
              <div className="invoice-notes-panel-head">
                <strong>Conditions de facturation</strong>
                <span className="muted small">
                  Cadre sous le total sur l’aperçu / PDF
                </span>
              </div>
              <div
                className="invoice-notes-mode"
                role="radiogroup"
                aria-label="Portée de la note"
              >
                {INVOICE_NOTES_MODE_OPTIONS.map((opt) => (
                  <label key={opt.id}>
                    <input
                      type="radio"
                      name="invoice-notes-mode"
                      value={opt.id}
                      checked={notesMode === opt.id}
                      onChange={() => setNotesMode(opt.id)}
                    />
                    {opt.label}
                  </label>
                ))}
              </div>
              <p className="muted invoice-notes-hint">
                {
                  INVOICE_NOTES_MODE_OPTIONS.find((o) => o.id === notesMode)
                    ?.hint
                }
              </p>
              <textarea
                rows={4}
                value={notes}
                onChange={(e) => setNotes(e.target.value)}
                placeholder="Ex. Paiement à 30 jours nets…"
                aria-label="Texte des conditions de facturation"
              />
            </div>

            <div className="btn-row">
              <button type="submit" className="primary" disabled={busy}>
                Enregistrer brouillon
              </button>
            </div>
          </form>

          <BillingDocumentPreview
            model={livePreview}
            label="Aperçu live de la facture"
          />
        </div>
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
          <strong>Encaisser</strong> — BQ postée (512/411) + lettrage soft
        </li>
        <li>
          <strong>Affacturage</strong> — OD 467/411 + BQ (512 / frais / 467)
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
            Réf. banque{" "}
            <span className="muted" title="Numéro de virement — pas obligatoire">
              (optionnel)
            </span>
            <input
              value={payRef}
              onChange={(e) => setPayRef(e.target.value)}
              placeholder="laisser vide si inconnu"
              aria-required="false"
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

      {factorInvoiceId != null ? (
        <div className="toolbar payment-bar">
          <LabelWithHelp topic="affacturage">
            <strong>Affacturage facture #{factorInvoiceId}</strong>
          </LabelWithHelp>
          <label>
            Date
            <input
              type="date"
              value={factorDate}
              onChange={(e) => setFactorDate(e.target.value)}
            />
          </label>
          <label>
            Montant cédé
            <input
              value={factorAmount}
              onChange={(e) => setFactorAmount(e.target.value)}
              style={{ maxWidth: "8rem" }}
            />
          </label>
          <label>
            Commission
            <input
              value={factorFee}
              onChange={(e) => setFactorFee(e.target.value)}
              style={{ maxWidth: "7rem" }}
            />
          </label>
          <label>
            Compte frais
            <input
              value={factorFeeAccount}
              onChange={(e) => setFactorFeeAccount(e.target.value)}
              style={{ maxWidth: "5rem" }}
              placeholder="622"
            />
          </label>
          <label>
            Réf. factor
            <input
              value={factorRef}
              onChange={(e) => setFactorRef(e.target.value)}
              placeholder="FAC-…"
            />
          </label>
          <span className="muted small">
            Net reçu{" "}
            {formatEuros(
              factoringNetCents(
                parseEurosToCents(factorAmount),
                parseEurosToCents(factorFee),
              ),
            )}
          </span>
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void onRecordFactoring()}
          >
            Enregistrer (467 / 411 / frais)
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => setFactorInvoiceId(null)}
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
                    {invoiceSourceHint(inv.source) ? (
                      <span className="muted" title="Facture client importée">
                        {" "}
                        · {invoiceSourceHint(inv.source)}
                      </span>
                    ) : null}
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
                            {!isImportedInvoice(inv) ? (
                              inv.entryId ? (
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
                              )
                            ) : inv.entryId ? (
                              <span
                                className="hint"
                                title="VTE déjà postée à la confirmation Import"
                              >
                                VTE postée
                              </span>
                            ) : null}
                            {inv.kind !== "credit_note" &&
                              inv.paidCents < inv.ttcCents && (
                                <>
                                  <button
                                    type="button"
                                    className="primary"
                                    disabled={busy}
                                    onClick={() => openPayment(inv)}
                                  >
                                    Encaisser
                                  </button>
                                  <button
                                    type="button"
                                    className="secondary"
                                    disabled={busy}
                                    onClick={() => openFactoring(inv)}
                                    title="Cession factor : OD 467/411 + BQ net et commission"
                                  >
                                    Affacturage
                                  </button>
                                </>
                              )}
                            {inv.status === "paid" ||
                            inv.status === "partially_paid" ? (
                              <button
                                type="button"
                                className="ghost"
                                onClick={() => onNavigate?.("ecritures")}
                                title="Ouvre Écritures pour consulter la BQ postée"
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

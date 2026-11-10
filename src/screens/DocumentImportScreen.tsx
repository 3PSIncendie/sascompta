import { useCallback, useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { LabelWithHelp } from "../components/FieldHelp";
import { ThirdPartySelect } from "../components/ThirdPartySelect";
import { api } from "../lib/api";
import {
  buildProposedLines,
  confidenceLabel,
  DOC_TYPE_REGISTRY,
  docTypeMeta,
  formFromImport,
  previewDocumentImports,
  toConfirmInput,
  type DocumentImport,
  type ReviewFormState,
} from "../lib/documentImport";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import type { NavId, ThirdParty, VatCode } from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

type Props = {
  onNavigate?: (id: NavId) => void;
};

export function DocumentImportScreen({ onNavigate }: Props) {
  const [imports, setImports] = useState<DocumentImport[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [form, setForm] = useState<ReviewFormState | null>(null);
  const [tiers, setTiers] = useState<ThirdParty[]>([]);
  const [vatCodes, setVatCodes] = useState<VatCode[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [dragOver, setDragOver] = useState(false);

  const selected = useMemo(
    () => imports.find((d) => d.id === selectedId) ?? null,
    [imports, selectedId],
  );

  const reload = useCallback(async () => {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      const list = previewDocumentImports();
      setImports(list);
      setTiers([
        {
          id: 1,
          code: "F001",
          name: "FOURNITURES DELTA SAS",
          kind: "supplier",
          accountNumber: "401",
          active: true,
          siret: "12345678900012",
          email: null,
          address: null,
          routingId: null,
        },
      ]);
      setVatCodes([
        { code: "T20D", label: "TVA 20% déductible", rateBp: 2000, direction: "deductible", active: true },
        { code: "T10D", label: "TVA 10% déductible", rateBp: 1000, direction: "deductible", active: true },
        { code: "T20R", label: "Autoliquidation 20%", rateBp: 2000, direction: "reverse", active: true },
        { code: "TEX", label: "Exonéré", rateBp: 0, direction: "exempt", active: true },
      ]);
      if (list[0]) {
        setSelectedId(list[0].id);
        setForm(formFromImport(list[0]));
      }
      return;
    }
    const [list, t, v] = await Promise.all([
      api.listDocumentImports(),
      api.listThirdParties(),
      api.listVatCodes(),
    ]);
    setImports(list);
    setTiers(t.filter((x) => x.kind === "supplier"));
    setVatCodes(v);
  }, []);

  useEffect(() => {
    void reload().catch((e) => setError(String(e)));
  }, [reload]);

  function selectDoc(doc: DocumentImport) {
    setSelectedId(doc.id);
    setForm(formFromImport(doc));
    setStatus(null);
    setError(null);
  }

  async function analyzePath(path: string) {
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu navigateur — ouvrez l’app desktop pour importer un vrai PDF.");
        return;
      }
      const doc = await api.analyzeDocumentImport(path);
      await reload();
      selectDoc(doc);
      setStatus(
        `Document analysé (#${doc.id}). Vérifiez les champs puis créez le brouillon — rien n’est validé en compta tant que vous ne confirmez pas.`,
      );
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onPickFile() {
    if (PREVIEW || !isTauriRuntime()) {
      setStatus("Aperçu — utilisez le desktop Tauri pour choisir un PDF ou une image.");
      return;
    }
    const path = await open({
      multiple: false,
      filters: [
        {
          name: "Factures / images",
          extensions: ["pdf", "png", "jpg", "jpeg", "webp", "tif", "tiff"],
        },
      ],
    });
    if (!path || Array.isArray(path)) return;
    await analyzePath(path);
  }

  function onDrop(e: React.DragEvent) {
    e.preventDefault();
    setDragOver(false);
    const file = e.dataTransfer.files?.[0];
    // Tauri webview : le chemin natif n’est pas toujours exposé via File API.
    // On guide vers le sélecteur fichier (chemin disque requis pour copie workspace).
    if (file) {
      setStatus(
        `Fichier « ${file.name} » détecté. Utilisez « Choisir un PDF / image » pour l’importer dans le workspace (chemin local requis).`,
      );
    }
    void onPickFile();
  }

  const proposed = useMemo(
    () => (form ? buildProposedLines(form) : []),
    [form],
  );

  const meta = form ? docTypeMeta(form.docType) : null;
  const canPost = meta?.postingSupported === true;

  async function onConfirm() {
    if (!form || !selected) return;
    if (!canPost) {
      setError(
        meta?.comingSoonHint ||
          "Ce type de document n’a pas encore de modèle d’écriture. Choisissez « Facture fournisseur ».",
      );
      return;
    }
    const htCents = parseEurosToCents(form.htStr);
    if (htCents <= 0) {
      setError("Indiquez un montant HT valide (ex. 100,00).");
      return;
    }
    if (!form.thirdPartyId && form.createSupplierName.trim().length < 2) {
      setError("Choisissez un fournisseur existant ou saisissez un nom pour en créer un.");
      return;
    }
    const vatParsed = parseEurosToCents(form.vatStr);
    const vatCents =
      form.vatMode === "normale" && vatParsed > 0 ? vatParsed : null;
    setBusy(true);
    setError(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus(
          "Aperçu : brouillon ACH aurait été créé après confirmation. Passez par le desktop pour persister.",
        );
        return;
      }
      const confirmed = await api.confirmDocumentImport(
        selected.id,
        toConfirmInput(form, htCents, vatCents),
      );
      setStatus(
        `Brouillon d’écriture #${confirmed.entryId} créé (ACH). Ouvrez Écritures pour relire puis valider — la validation journal reste manuelle.`,
      );
      await reload();
      setSelectedId(confirmed.id);
      setForm(formFromImport(confirmed));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function onDiscard() {
    if (!selected || selected.status !== "pending_review") return;
    setBusy(true);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setImports((prev) =>
          prev.map((d) =>
            d.id === selected.id ? { ...d, status: "discarded" } : d,
          ),
        );
        setStatus("Import écarté (aperçu).");
        return;
      }
      await api.discardDocumentImport(selected.id);
      await reload();
      setSelectedId(null);
      setForm(null);
      setStatus("Import écarté.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const pending = imports.filter((d) => d.status === "pending_review");
  const others = imports.filter((d) => d.status !== "pending_review");

  return (
    <div className="panel document-import-screen">
      <header className="panel-head">
        <div>
          <h1>Import documents</h1>
          <p>
            Déposez une facture fournisseur (PDF ou image). SasCompta lit le texte
            en local, propose le type et un brouillon ACH + TVA —{" "}
            <strong>vous confirmez avant toute écriture</strong>. Les autres types
            (tickets, relevés, frais, factures clients) réutilisent ce même écran
            dès que leur modèle sera branché.
          </p>
        </div>
        {onNavigate && (
          <button
            type="button"
            className="ghost"
            onClick={() => onNavigate("ecritures")}
          >
            Voir les écritures →
          </button>
        )}
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="ok">{status}</p>}

      <div
        className={`import-dropzone${dragOver ? " is-over" : ""}`}
        onDragOver={(e) => {
          e.preventDefault();
          setDragOver(true);
        }}
        onDragLeave={() => setDragOver(false)}
        onDrop={onDrop}
      >
        <p>
          Glissez un PDF / image ici, ou choisissez un fichier. Le fichier est
          copié à côté de votre société (dossier <code>.files/imports</code>).
        </p>
        <div className="btn-row">
          <button
            type="button"
            className="primary"
            disabled={busy}
            onClick={() => void onPickFile()}
          >
            Choisir un PDF / image
          </button>
        </div>
        <p className="muted tiny">
          Extraction locale (couche texte PDF). Pas d’API cloud payante. Image ou
          PDF scanné : complètez les montants à la revue (OCR local optionnel plus
          tard).
        </p>
      </div>

      <div className="document-import-layout">
        <aside className="document-import-list">
          <h2>En revue ({pending.length})</h2>
          {pending.length === 0 && (
            <p className="muted">Aucun document en attente.</p>
          )}
          <ul>
            {pending.map((d) => (
              <li key={d.id}>
                <button
                  type="button"
                  className={d.id === selectedId ? "is-active" : ""}
                  onClick={() => selectDoc(d)}
                >
                  <span className="doc-name">{d.sourceFileName}</span>
                  <span className="doc-meta">
                    {docTypeMeta(d.docType).label} ·{" "}
                    {confidenceLabel(d.confidence)}
                  </span>
                </button>
              </li>
            ))}
          </ul>
          {others.length > 0 && (
            <>
              <h2>Traités</h2>
              <ul>
                {others.map((d) => (
                  <li key={d.id}>
                    <button
                      type="button"
                      className={d.id === selectedId ? "is-active" : ""}
                      onClick={() => selectDoc(d)}
                    >
                      <span className="doc-name">{d.sourceFileName}</span>
                      <span className="doc-meta">
                        {d.status}
                        {d.entryId ? ` · écriture #${d.entryId}` : ""}
                      </span>
                    </button>
                  </li>
                ))}
              </ul>
            </>
          )}
        </aside>

        <section className="document-import-review">
          {!form || !selected ? (
            <p className="muted">
              Importez un document pour ouvrir l’écran de revue guidée.
            </p>
          ) : (
            <>
              <h2>Revue avant brouillon</h2>
              <p className="muted">
                Méthode : <strong>{selected.extractionMethod}</strong> · confiance{" "}
                <strong>{confidenceLabel(selected.confidence)}</strong> (
                {Math.round(selected.confidence * 100)} %) · statut{" "}
                <strong>{selected.status}</strong>
              </p>
              {selected.extraction.notes && (
                <p className="hint-banner">{selected.extraction.notes}</p>
              )}

              <div className="form-grid">
                <label>
                  <LabelWithHelp topic="importDocType">Type détecté</LabelWithHelp>
                  <select
                    value={form.docType}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({
                        ...form,
                        docType: e.target.value,
                        journalCode: docTypeMeta(e.target.value).defaultJournal,
                      })
                    }
                  >
                    {DOC_TYPE_REGISTRY.map((t) => (
                      <option key={t.id} value={t.id}>
                        {t.label}
                        {!t.postingSupported ? " (bientôt)" : ""}
                      </option>
                    ))}
                  </select>
                  <span className="field-hint">{meta?.shortHelp}</span>
                  {!canPost && meta?.comingSoonHint && (
                    <span className="field-hint warn">{meta.comingSoonHint}</span>
                  )}
                </label>

                <label>
                  <LabelWithHelp topic="piece">N° pièce</LabelWithHelp>
                  <input
                    value={form.pieceRef}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, pieceRef: e.target.value })
                    }
                  />
                </label>

                <label>
                  Date
                  <input
                    type="date"
                    value={form.entryDate}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, entryDate: e.target.value })
                    }
                  />
                </label>

                <label>
                  <LabelWithHelp topic="libelle">Libellé</LabelWithHelp>
                  <input
                    value={form.label}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, label: e.target.value })
                    }
                  />
                </label>

                <label>
                  <LabelWithHelp topic="montantHt">Montant HT (€)</LabelWithHelp>
                  <input
                    value={form.htStr}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, htStr: e.target.value })
                    }
                  />
                </label>

                <label>
                  TVA (€)
                  <input
                    value={form.vatStr}
                    disabled={
                      selected.status !== "pending_review" ||
                      form.vatMode !== "normale"
                    }
                    onChange={(e) =>
                      setForm({ ...form, vatStr: e.target.value })
                    }
                  />
                </label>

                <label>
                  <LabelWithHelp topic="regimeTvaSaisie">Régime TVA</LabelWithHelp>
                  <select
                    value={form.vatMode}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) => {
                      const vatMode = e.target.value as ReviewFormState["vatMode"];
                      setForm({
                        ...form,
                        vatMode,
                        htAccount:
                          vatMode === "autoliquidation_sous_traitance"
                            ? "604"
                            : form.htAccount || "606",
                        vatCode:
                          vatMode === "autoliquidation_sous_traitance"
                            ? "T20R"
                            : vatMode === "sans"
                              ? "TEX"
                              : "T20D",
                      });
                    }}
                  >
                    <option value="normale">TVA classique</option>
                    <option value="sans">Sans TVA</option>
                    <option value="autoliquidation_sous_traitance">
                      Autoliquidation — sous-traitance (art. 283)
                    </option>
                  </select>
                </label>

                <label>
                  <LabelWithHelp topic="codeTva">Code TVA</LabelWithHelp>
                  <select
                    value={form.vatCode}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) => {
                      const code = e.target.value;
                      const vc = vatCodes.find((c) => c.code === code);
                      setForm({
                        ...form,
                        vatCode: code,
                        rateBp: vc?.rateBp ?? form.rateBp,
                      });
                    }}
                  >
                    {vatCodes.map((c) => (
                      <option key={c.code} value={c.code}>
                        {c.code} — {c.label}
                      </option>
                    ))}
                  </select>
                </label>

                <label>
                  Compte charge
                  <input
                    value={form.htAccount}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, htAccount: e.target.value })
                    }
                  />
                </label>

                <label>
                  Journal
                  <input
                    value={form.journalCode}
                    disabled={selected.status !== "pending_review"}
                    onChange={(e) =>
                      setForm({ ...form, journalCode: e.target.value })
                    }
                  />
                </label>
              </div>

              <div className="form-grid">
                <label className="span-2">
                  <LabelWithHelp topic="tiers">Fournisseur</LabelWithHelp>
                  <ThirdPartySelect
                    tiers={tiers}
                    value={form.thirdPartyId}
                    kind="supplier"
                    disabled={selected.status !== "pending_review"}
                    onChange={(id) =>
                      setForm({
                        ...form,
                        thirdPartyId: id,
                        createSupplierName: id ? "" : form.createSupplierName,
                      })
                    }
                    onCreated={(tp) => {
                      setTiers((prev) =>
                        prev.some((t) => t.id === tp.id) ? prev : [...prev, tp],
                      );
                      setForm({
                        ...form,
                        thirdPartyId: tp.id,
                        createSupplierName: "",
                      });
                    }}
                  />
                </label>
                {!form.thirdPartyId && (
                  <>
                    <label>
                      Nouveau fournisseur — nom
                      <input
                        value={form.createSupplierName}
                        disabled={selected.status !== "pending_review"}
                        placeholder="Raison sociale"
                        onChange={(e) =>
                          setForm({
                            ...form,
                            createSupplierName: e.target.value,
                          })
                        }
                      />
                    </label>
                    <label>
                      Code (optionnel)
                      <input
                        value={form.createSupplierCode}
                        disabled={selected.status !== "pending_review"}
                        placeholder="F003"
                        onChange={(e) =>
                          setForm({
                            ...form,
                            createSupplierCode: e.target.value,
                          })
                        }
                      />
                    </label>
                    <label>
                      SIRET (optionnel)
                      <input
                        value={form.createSupplierSiret}
                        disabled={selected.status !== "pending_review"}
                        onChange={(e) =>
                          setForm({
                            ...form,
                            createSupplierSiret: e.target.value,
                          })
                        }
                      />
                    </label>
                  </>
                )}
              </div>

              {canPost && proposed.length > 0 && (
                <div className="proposed-lines">
                  <h3>Lignes proposées (ACH)</h3>
                  <p className="muted tiny">
                    Aperçu — le brouillon ne sera créé qu’après confirmation. La
                    validation (post) reste dans Écritures.
                  </p>
                  <table>
                    <thead>
                      <tr>
                        <th>Compte</th>
                        <th>Débit</th>
                        <th>Crédit</th>
                        <th>TVA</th>
                      </tr>
                    </thead>
                    <tbody>
                      {proposed.map((l, i) => (
                        <tr key={i}>
                          <td>{l.accountNumber}</td>
                          <td>
                            {l.debitCents ? formatEuros(l.debitCents) : ""}
                          </td>
                          <td>
                            {l.creditCents ? formatEuros(l.creditCents) : ""}
                          </td>
                          <td>{l.vatCode ?? ""}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}

              {selected.status === "pending_review" && (
                <div className="btn-row">
                  <button
                    type="button"
                    className="primary"
                    disabled={busy || !canPost}
                    onClick={() => void onConfirm()}
                  >
                    Confirmer → brouillon ACH
                  </button>
                  <button
                    type="button"
                    className="ghost"
                    disabled={busy}
                    onClick={() => void onDiscard()}
                  >
                    Écarter
                  </button>
                </div>
              )}

              {selected.status === "confirmed" && selected.entryId && (
                <p className="ok">
                  Lié à l’écriture brouillon #{selected.entryId}
                  {onNavigate && (
                    <>
                      {" "}
                      —{" "}
                      <button
                        type="button"
                        className="linkish"
                        onClick={() => onNavigate("ecritures")}
                      >
                        Ouvrir Écritures
                      </button>
                    </>
                  )}
                </p>
              )}

              {selected.extraction.rawTextPreview && (
                <details className="raw-extract">
                  <summary>Texte extrait (aperçu)</summary>
                  <pre>{selected.extraction.rawTextPreview}</pre>
                </details>
              )}
            </>
          )}
        </section>
      </div>
    </div>
  );
}

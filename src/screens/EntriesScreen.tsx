import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { LabelWithHelp } from "../components/FieldHelp";
import { api } from "../lib/api";
import {
  buildGuidedLines,
  DEFAULT_COUNTERPART,
  defaultVatCodeFor,
  eurosStringsFromLines,
  preferredHtAccount,
  suggestHtAccounts,
  vatCodesForKind,
  vatModesForKind,
  type GuidedKind,
  type GuidedVatMode,
} from "../lib/guidedEntry";
import { formatEuros, formatRateBp, parseEurosToCents } from "../lib/money";
import type {
  Account,
  CreateEntryInput,
  Entry,
  EntryLineInput,
  Journal,
  ThirdParty,
  VatCode,
} from "../lib/types";

const emptyLine = (): EntryLineInput => ({
  accountNumber: "",
  debitCents: 0,
  creditCents: 0,
  vatCode: null,
  thirdPartyId: null,
});

type EntryMode = "libre" | "guidee";

export function EntriesScreen() {
  const [entries, setEntries] = useState<Entry[]>([]);
  const [journals, setJournals] = useState<Journal[]>([]);
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [vatCodes, setVatCodes] = useState<VatCode[]>([]);
  const [tiers, setTiers] = useState<ThirdParty[]>([]);
  const [selected, setSelected] = useState<Entry | null>(null);
  const [editing, setEditing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<EntryMode>("guidee");

  const [journalCode, setJournalCode] = useState("OD");
  const [entryDate, setEntryDate] = useState("2026-05-01");
  const [pieceRef, setPieceRef] = useState("");
  const [label, setLabel] = useState("");
  const [lines, setLines] = useState<EntryLineInput[]>([emptyLine(), emptyLine()]);
  const [debitStr, setDebitStr] = useState<string[]>(["", ""]);
  const [creditStr, setCreditStr] = useState<string[]>(["", ""]);

  const [guidedKind, setGuidedKind] = useState<GuidedKind>("vente");
  const [guidedHtAccount, setGuidedHtAccount] = useState("706");
  const [guidedHtStr, setGuidedHtStr] = useState("100,00");
  const [guidedVatMode, setGuidedVatMode] = useState<GuidedVatMode>("normale");
  const [guidedVatCode, setGuidedVatCode] = useState("T20C");
  const [guidedCounterpart, setGuidedCounterpart] = useState(
    DEFAULT_COUNTERPART.vente,
  );
  const [guidedThirdPartyId, setGuidedThirdPartyId] = useState<number | null>(
    null,
  );
  const [guidedHint, setGuidedHint] = useState<string | null>(null);

  async function reload() {
    const [e, j, a, v, t] = await Promise.all([
      api.listEntries(),
      api.listJournals(),
      api.listAccounts(),
      api.listVatCodes(),
      api.listThirdParties(),
    ]);
    setEntries(e);
    setJournals(j);
    setAccounts(a);
    setVatCodes(v);
    setTiers(t);
  }

  useEffect(() => {
    void reload().catch((err) => setError(String(err)));
  }, []);

  const totals = useMemo(() => {
    const d = lines.reduce((s, l) => s + l.debitCents, 0);
    const c = lines.reduce((s, l) => s + l.creditCents, 0);
    return { d, c, ok: d === c && d > 0 };
  }, [lines]);

  const guidedModeOptions = useMemo(
    () => vatModesForKind(guidedKind),
    [guidedKind],
  );

  const guidedVatOptions = useMemo(
    () => vatCodesForKind(guidedKind, vatCodes, guidedVatMode),
    [guidedKind, vatCodes, guidedVatMode],
  );

  const guidedHtAccounts = useMemo(
    () => suggestHtAccounts(guidedKind, accounts),
    [guidedKind, accounts],
  );

  const selectedVat = guidedVatOptions.find((v) => v.code === guidedVatCode);
  const needsVatCode = guidedVatMode !== "sans";
  const needsVatRate =
    guidedVatMode === "normale" ||
    guidedVatMode === "autoliquidation_sous_traitance" ||
    guidedVatMode === "autoliquidation_chantier";

  const preview = useMemo(() => {
    const ht = parseEurosToCents(guidedHtStr);
    if (ht <= 0) return null;
    return buildGuidedLines({
      kind: guidedKind,
      htAccount: guidedHtAccount,
      htCents: ht,
      vatMode: guidedVatMode,
      vatCode: needsVatCode ? guidedVatCode : null,
      rateBp: needsVatRate ? (selectedVat?.rateBp ?? 0) : 0,
      counterpartAccount: guidedCounterpart,
      thirdPartyId: guidedThirdPartyId,
    });
  }, [
    guidedHtStr,
    guidedKind,
    guidedHtAccount,
    guidedVatMode,
    guidedVatCode,
    needsVatCode,
    needsVatRate,
    selectedVat?.rateBp,
    guidedCounterpart,
    guidedThirdPartyId,
  ]);

  function applyVatModeDefaults(kind: GuidedKind, mode: GuidedVatMode) {
    setGuidedVatMode(mode);
    const preferredHt = preferredHtAccount(kind, mode);
    const htList = suggestHtAccounts(kind, accounts);
    setGuidedHtAccount(
      htList.some((a) => a.number === preferredHt)
        ? preferredHt
        : (htList[0]?.number ?? ""),
    );
    const def = defaultVatCodeFor(kind, 2000, vatCodes, mode);
    if (def) setGuidedVatCode(def.code);
    else setGuidedVatCode("");
    setGuidedHint(null);
  }

  function applyKindDefaults(kind: GuidedKind) {
    setGuidedKind(kind);
    setGuidedCounterpart(DEFAULT_COUNTERPART[kind]);
    setJournalCode(kind === "vente" ? "VTE" : "ACH");
    applyVatModeDefaults(kind, "normale");
  }

  function generateFromGuided() {
    setError(null);
    const ht = parseEurosToCents(guidedHtStr);
    if (ht <= 0) {
      setError("Indiquez un montant HT positif.");
      return;
    }
    if (!guidedHtAccount) {
      setError("Choisissez le compte de charge ou de produit.");
      return;
    }
    if (!guidedCounterpart) {
      setError("Choisissez le compte de contrepartie.");
      return;
    }
    if (needsVatCode) {
      const code = guidedVatOptions.find((v) => v.code === guidedVatCode);
      if (!code) {
        setError(
          guidedVatMode.startsWith("autoliquidation")
            ? "Choisissez un code d’autoliquidation (T…R)."
            : "Choisissez un code TVA adapté (collectée ou déductible).",
        );
        return;
      }
    }
    const result = buildGuidedLines({
      kind: guidedKind,
      htAccount: guidedHtAccount,
      htCents: ht,
      vatMode: guidedVatMode,
      vatCode: needsVatCode ? guidedVatCode : null,
      rateBp: needsVatRate ? (selectedVat?.rateBp ?? 0) : 0,
      counterpartAccount: guidedCounterpart,
      thirdPartyId: guidedThirdPartyId,
    });
    setLines(result.lines);
    const strs = eurosStringsFromLines(result.lines);
    setDebitStr(strs.debitStr);
    setCreditStr(strs.creditStr);
    setMode("libre");
    let vatPart = " (sans TVA)";
    if (result.vatMode === "autoliquidation_sous_traitance" && result.vatCents > 0) {
      vatPart = ` + autoliquidation ${formatEuros(result.vatCents)} (${result.vatAccount} / ${result.vatDueAccount})`;
    } else if (result.vatMode === "autoliquidation_chantier") {
      vatPart = " (autoliquidation chantier — HT facturé)";
    } else if (result.vatCents > 0) {
      vatPart = ` + TVA ${formatEuros(result.vatCents)} (${result.vatAccount})`;
    }
    setGuidedHint(
      `Lignes générées : HT ${formatEuros(result.htCents)}${vatPart} → contrepartie ${formatEuros(result.ttcCents)}. Vous pouvez encore tout modifier ci-dessous.`,
    );
  }

  function startNew() {
    setSelected(null);
    setEditing(true);
    setJournalCode("VTE");
    setEntryDate("2026-05-01");
    setPieceRef("");
    setLabel("");
    setLines([emptyLine(), emptyLine()]);
    setDebitStr(["", ""]);
    setCreditStr(["", ""]);
    setMode("guidee");
    applyKindDefaults("vente");
    setGuidedHtStr("100,00");
    setGuidedThirdPartyId(null);
    setGuidedHint(null);
  }

  function loadEntry(e: Entry) {
    setSelected(e);
    setEditing(e.status === "draft");
    setJournalCode(e.journalCode);
    setEntryDate(e.entryDate);
    setPieceRef(e.pieceRef);
    setLabel(e.label);
    const ls = e.lines.map((l) => ({
      accountNumber: l.accountNumber,
      label: l.label,
      debitCents: l.debitCents,
      creditCents: l.creditCents,
      vatCode: l.vatCode,
      thirdPartyId: l.thirdPartyId,
      compAuxType: l.compAuxType,
      compAuxNum: l.compAuxNum,
    }));
    setLines(ls.length ? ls : [emptyLine(), emptyLine()]);
    setDebitStr(ls.map((l) => (l.debitCents ? (l.debitCents / 100).toFixed(2) : "")));
    setCreditStr(
      ls.map((l) => (l.creditCents ? (l.creditCents / 100).toFixed(2) : "")),
    );
    setMode("libre");
    setGuidedHint(null);
  }

  function updateLine(i: number, patch: Partial<EntryLineInput>) {
    setLines((prev) => prev.map((l, idx) => (idx === i ? { ...l, ...patch } : l)));
  }

  async function saveDraft() {
    setError(null);
    const input: CreateEntryInput = {
      journalCode,
      entryDate,
      pieceRef,
      label,
      lines,
    };
    try {
      const e =
        selected && selected.status === "draft"
          ? await api.updateEntry(selected.id, input)
          : await api.createEntry(input);
      await reload();
      loadEntry(e);
    } catch (err) {
      setError(String(err));
    }
  }

  async function post() {
    if (!selected) return;
    setError(null);
    try {
      const e = await api.postEntry(selected.id);
      await reload();
      loadEntry(e);
    } catch (err) {
      setError(String(err));
    }
  }

  async function reverse() {
    if (!selected) return;
    setError(null);
    try {
      const e = await api.reverseEntry(selected.id);
      await reload();
      loadEntry(e);
    } catch (err) {
      setError(String(err));
    }
  }

  async function remove() {
    if (!selected) return;
    setError(null);
    try {
      await api.deleteEntry(selected.id);
      setSelected(null);
      setEditing(false);
      await reload();
    } catch (err) {
      setError(String(err));
    }
  }

  async function attach() {
    if (!selected || selected.status !== "draft") return;
    const picked = await open({ multiple: false });
    if (!picked) return;
    const path = String(picked);
    const name = path.split(/[/\\]/).pop() ?? "piece";
    try {
      await api.addAttachment(selected.id, name, path, null);
      const e = await api.getEntry(selected.id);
      loadEntry(e);
      await reload();
    } catch (err) {
      setError(String(err));
    }
  }

  const postedCount = entries.filter((e) => e.status === "posted").length;

  return (
    <div className="panel">
      <h1>Écritures</h1>
      <p>
        Saisie en partie double — {postedCount} écriture(s) validée(s). Le total
        débit doit égaler le total crédit avant validation.
      </p>
      <aside className="help-callout">
        <strong>Pour débuter</strong>
        <p>
          Préférez la <LabelWithHelp topic="saisieGuidee">saisie guidée</LabelWithHelp>{" "}
          : une intention (montant{" "}
          <LabelWithHelp topic="montantHt">HT</LabelWithHelp>, compte,{" "}
          <LabelWithHelp topic="regimeTvaSaisie">régime TVA</LabelWithHelp>) génère
          les lignes{" "}
          <LabelWithHelp topic="compteTva">445…</LabelWithHelp> adaptées — y compris{" "}
          <LabelWithHelp topic="autoliquidationBtp">autoliquidation BTP</LabelWithHelp>{" "}
          — et la{" "}
          <LabelWithHelp topic="contrepartie">contrepartie</LabelWithHelp>. Tout reste
          modifiable ensuite.
        </p>
      </aside>

      <div className="btn-row" style={{ marginBottom: "1rem" }}>
        <button type="button" className="primary" onClick={startNew}>
          Nouvelle écriture
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      <div className="table-wrap" style={{ maxHeight: 240, marginBottom: "1rem" }}>
        <table>
          <thead>
            <tr>
              <th>Date</th>
              <th>Journal</th>
              <th>Pièce</th>
              <th>N°</th>
              <th>Libellé</th>
              <th>Statut</th>
            </tr>
          </thead>
          <tbody>
            {entries.map((e) => (
              <tr
                key={e.id}
                onClick={() => loadEntry(e)}
                style={{
                  cursor: "pointer",
                  background: selected?.id === e.id ? "#f0e9dc" : undefined,
                }}
              >
                <td>{e.entryDate}</td>
                <td>{e.journalCode}</td>
                <td>{e.pieceRef}</td>
                <td>{e.ecritureNum ?? "—"}</td>
                <td>{e.label}</td>
                <td>
                  <span className={`badge ${e.status}`}>{e.status}</span>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {(editing || selected) && (
        <div>
          <h2>{editing ? "Saisie" : "Consultation"}</h2>
          <div className="form-grid two">
            <label>
              <LabelWithHelp topic="journal">Journal</LabelWithHelp>
              <select
                value={journalCode}
                disabled={!editing}
                onChange={(e) => setJournalCode(e.target.value)}
              >
                {journals.map((j) => (
                  <option key={j.code} value={j.code}>
                    {j.code} — {j.label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              Date
              <input
                type="date"
                value={entryDate}
                disabled={!editing}
                onChange={(e) => setEntryDate(e.target.value)}
              />
            </label>
            <label>
              <LabelWithHelp topic="piece">Pièce</LabelWithHelp>
              <input
                value={pieceRef}
                disabled={!editing}
                onChange={(e) => setPieceRef(e.target.value)}
              />
            </label>
            <label>
              <LabelWithHelp topic="libelle">Libellé</LabelWithHelp>
              <input
                value={label}
                disabled={!editing}
                onChange={(e) => setLabel(e.target.value)}
              />
            </label>
          </div>

          {editing && (
            <div className="guided-panel">
              <div className="mode-toggle" role="tablist" aria-label="Mode de saisie">
                <button
                  type="button"
                  role="tab"
                  aria-selected={mode === "guidee"}
                  className={mode === "guidee" ? "active" : ""}
                  onClick={() => setMode("guidee")}
                >
                  Saisie guidée
                </button>
                <button
                  type="button"
                  role="tab"
                  aria-selected={mode === "libre"}
                  className={mode === "libre" ? "active" : ""}
                  onClick={() => setMode("libre")}
                >
                  Saisie libre
                </button>
              </div>

              {mode === "guidee" && (
                <>
                  <p className="hint">
                    Remplissez une intention ; les lignes débit/crédit sont proposées
                    automatiquement, puis éditables.
                  </p>
                  <div className="form-grid two">
                    <label>
                      Nature
                      <select
                        value={guidedKind}
                        onChange={(e) =>
                          applyKindDefaults(e.target.value as GuidedKind)
                        }
                      >
                        <option value="vente">Vente (produit)</option>
                        <option value="achat">Achat (charge)</option>
                      </select>
                    </label>
                    <label>
                      <LabelWithHelp topic="numeroCompte">
                        Compte HT
                      </LabelWithHelp>
                      <select
                        value={guidedHtAccount}
                        onChange={(e) => setGuidedHtAccount(e.target.value)}
                      >
                        <option value="">—</option>
                        {guidedHtAccounts.map((a) => (
                          <option key={a.number} value={a.number}>
                            {a.number} — {a.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label>
                      <LabelWithHelp topic="montantHt">Montant HT</LabelWithHelp>
                      <input
                        value={guidedHtStr}
                        onChange={(e) => setGuidedHtStr(e.target.value)}
                        inputMode="decimal"
                        placeholder="100,00"
                      />
                    </label>
                    <label>
                      <LabelWithHelp topic="contrepartie">
                        Contrepartie
                      </LabelWithHelp>
                      <select
                        value={guidedCounterpart}
                        onChange={(e) => setGuidedCounterpart(e.target.value)}
                      >
                        {accounts
                          .filter(
                            (a) =>
                              a.number.startsWith("401") ||
                              a.number.startsWith("411") ||
                              a.number.startsWith("512") ||
                              a.number.startsWith("530") ||
                              a.number === guidedCounterpart,
                          )
                          .map((a) => (
                            <option key={a.number} value={a.number}>
                              {a.number} — {a.label}
                            </option>
                          ))}
                      </select>
                    </label>
                    <label>
                      <LabelWithHelp topic="regimeTvaSaisie">
                        Régime TVA
                      </LabelWithHelp>
                      <select
                        value={guidedVatMode}
                        onChange={(e) =>
                          applyVatModeDefaults(
                            guidedKind,
                            e.target.value as GuidedVatMode,
                          )
                        }
                      >
                        {guidedModeOptions.map((m) => (
                          <option key={m.id} value={m.id}>
                            {m.label}
                          </option>
                        ))}
                      </select>
                    </label>
                    {needsVatRate && (
                      <label>
                        <LabelWithHelp topic="codeTva">Taux / code</LabelWithHelp>
                        <select
                          value={guidedVatCode}
                          onChange={(e) => setGuidedVatCode(e.target.value)}
                        >
                          {guidedVatOptions.map((v) => (
                            <option key={v.code} value={v.code}>
                              {v.code} — {formatRateBp(v.rateBp)} ({v.label})
                            </option>
                          ))}
                        </select>
                      </label>
                    )}
                    {(guidedVatMode === "autoliquidation_sous_traitance" ||
                      guidedVatMode === "autoliquidation_chantier") && (
                      <p className="hint" style={{ gridColumn: "1 / -1" }}>
                        <LabelWithHelp topic="autoliquidationBtp">
                          Art. 283 CGI
                        </LabelWithHelp>
                        {" — "}
                        {
                          guidedModeOptions.find((m) => m.id === guidedVatMode)
                            ?.shortHelp
                        }
                      </p>
                    )}
                    <label>
                      <LabelWithHelp topic="tiers">Tiers (optionnel)</LabelWithHelp>
                      <select
                        value={guidedThirdPartyId ?? ""}
                        onChange={(e) =>
                          setGuidedThirdPartyId(
                            e.target.value ? Number(e.target.value) : null,
                          )
                        }
                      >
                        <option value="">—</option>
                        {tiers
                          .filter((t) =>
                            guidedKind === "vente"
                              ? t.kind === "customer" || t.accountNumber.startsWith("411")
                              : t.kind === "supplier" || t.accountNumber.startsWith("401"),
                          )
                          .map((t) => (
                            <option key={t.id} value={t.id}>
                              {t.code} {t.name}
                            </option>
                          ))}
                      </select>
                    </label>
                  </div>

                  {preview && preview.htCents > 0 && (
                    <div className="summary-strip guided-preview">
                      <span>
                        HT <strong>{formatEuros(preview.htCents)}</strong>
                      </span>
                      <span>
                        TVA{" "}
                        <strong>
                          {preview.vatDueAccount && preview.vatCents
                            ? `${formatEuros(preview.vatCents)} → ${preview.vatAccount} / ${preview.vatDueAccount}`
                            : preview.vatCents
                              ? `${formatEuros(preview.vatCents)} → ${preview.vatAccount}`
                              : preview.vatMode === "autoliquidation_chantier"
                                ? "autoliquidation client"
                                : "—"}
                        </strong>
                      </span>
                      <span>
                        Contrepartie{" "}
                        <strong>{formatEuros(preview.ttcCents)}</strong>
                      </span>
                      <span>
                        {preview.lines.length} ligne
                        {preview.lines.length > 1 ? "s" : ""}
                      </span>
                    </div>
                  )}

                  <div className="btn-row">
                    <button
                      type="button"
                      className="primary"
                      onClick={generateFromGuided}
                    >
                      Générer les lignes
                    </button>
                  </div>
                </>
              )}

              {guidedHint && <p className="ok">{guidedHint}</p>}
            </div>
          )}

          <div className="line-editor">
            <div className="line-row" style={{ fontSize: "0.8rem", color: "var(--ink-muted)" }}>
              <span>
                <LabelWithHelp topic="numeroCompte">Compte</LabelWithHelp>
              </span>
              <span>
                <LabelWithHelp topic="tiers">Tiers</LabelWithHelp>
              </span>
              <span>
                <LabelWithHelp topic="codeTva">TVA</LabelWithHelp>
              </span>
              <span>
                <LabelWithHelp topic="debitCredit">Débit</LabelWithHelp>
              </span>
              <span>Crédit</span>
              <span></span>
              <span></span>
            </div>
            {lines.map((l, i) => (
              <div className="line-row" key={i}>
                <select
                  value={l.accountNumber}
                  disabled={!editing}
                  onChange={(e) =>
                    updateLine(i, { accountNumber: e.target.value })
                  }
                >
                  <option value="">—</option>
                  {accounts.map((a) => (
                    <option key={a.number} value={a.number}>
                      {a.number}
                    </option>
                  ))}
                </select>
                <select
                  value={l.thirdPartyId ?? ""}
                  disabled={!editing}
                  onChange={(e) =>
                    updateLine(i, {
                      thirdPartyId: e.target.value
                        ? Number(e.target.value)
                        : null,
                    })
                  }
                >
                  <option value="">—</option>
                  {tiers.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.code} {t.name}
                    </option>
                  ))}
                </select>
                <select
                  value={l.vatCode ?? ""}
                  disabled={!editing}
                  onChange={(e) =>
                    updateLine(i, { vatCode: e.target.value || null })
                  }
                >
                  <option value="">—</option>
                  {vatCodes.map((v) => (
                    <option key={v.code} value={v.code}>
                      {v.code}
                    </option>
                  ))}
                </select>
                <input
                  disabled={!editing}
                  value={debitStr[i] ?? ""}
                  onChange={(e) => {
                    const v = e.target.value;
                    setDebitStr((s) => s.map((x, idx) => (idx === i ? v : x)));
                    updateLine(i, {
                      debitCents: parseEurosToCents(v),
                      creditCents: 0,
                    });
                    setCreditStr((s) => s.map((x, idx) => (idx === i ? "" : x)));
                  }}
                />
                <input
                  disabled={!editing}
                  value={creditStr[i] ?? ""}
                  onChange={(e) => {
                    const v = e.target.value;
                    setCreditStr((s) => s.map((x, idx) => (idx === i ? v : x)));
                    updateLine(i, {
                      creditCents: parseEurosToCents(v),
                      debitCents: 0,
                    });
                    setDebitStr((s) => s.map((x, idx) => (idx === i ? "" : x)));
                  }}
                />
                {editing && (
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => {
                      setLines((prev) => prev.filter((_, idx) => idx !== i));
                      setDebitStr((s) => s.filter((_, idx) => idx !== i));
                      setCreditStr((s) => s.filter((_, idx) => idx !== i));
                    }}
                  >
                    ×
                  </button>
                )}
              </div>
            ))}
          </div>

          <div className="summary-strip">
            <span>
              Débit <strong>{formatEuros(totals.d)}</strong>
            </span>
            <span>
              Crédit <strong>{formatEuros(totals.c)}</strong>
            </span>
            <span>
              Écart{" "}
              <strong style={{ color: totals.ok ? "var(--ok)" : "var(--danger)" }}>
                {formatEuros(totals.d - totals.c)}
              </strong>
            </span>
          </div>

          <div className="btn-row">
            {editing && (
              <>
                <button
                  type="button"
                  className="ghost"
                  onClick={() => {
                    setLines((prev) => [...prev, emptyLine()]);
                    setDebitStr((s) => [...s, ""]);
                    setCreditStr((s) => [...s, ""]);
                  }}
                >
                  + Ligne
                </button>
                <button type="button" className="primary" onClick={() => void saveDraft()}>
                  Enregistrer brouillon
                </button>
                {selected?.status === "draft" && (
                  <>
                    <button type="button" className="secondary" onClick={() => void post()}>
                      Valider
                    </button>
                    <button type="button" className="ghost" onClick={() => void attach()}>
                      Pièce jointe
                    </button>
                    <button type="button" className="danger" onClick={() => void remove()}>
                      Supprimer
                    </button>
                  </>
                )}
              </>
            )}
            {selected?.status === "posted" && (
              <button type="button" className="secondary" onClick={() => void reverse()}>
                Contrepasser
              </button>
            )}
          </div>

          {selected && selected.attachments.length > 0 && (
            <p className="ok">
              Pièces : {selected.attachments.map((a) => a.fileName).join(", ")}
            </p>
          )}
        </div>
      )}
    </div>
  );
}

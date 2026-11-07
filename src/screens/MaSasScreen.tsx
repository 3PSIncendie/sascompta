import { useEffect, useMemo, useState } from "react";
import { DeadlineReminders } from "../components/DeadlineReminders";
import { FieldHelp, LabelWithHelp } from "../components/FieldHelp";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import type { UpdateOrganizationInput, WorkspaceInfo } from "../lib/types";

const VAT_OPTIONS = [
  { value: "reel_mensuel", label: "Réel mensuel (CA3 chaque mois)" },
  { value: "reel_trimestriel", label: "Réel trimestriel (CA3 chaque trimestre)" },
  { value: "franchise", label: "Franchise en base (pas de TVA facturée)" },
] as const;

const LEGAL_OPTIONS = [
  { value: "SAS", label: "SAS — Société par actions simplifiée" },
  { value: "SASU", label: "SASU — SAS à associé unique" },
] as const;

function centsToEuroInput(cents: number): string {
  const euros = cents / 100;
  return euros.toFixed(2).replace(".", ",");
}

function validateForm(input: {
  name: string;
  siren: string;
  siret: string;
  capital: string;
  displayName: string;
  label: string;
  startDate: string;
  endDate: string;
}): string | null {
  if (input.name.trim().length < 2) {
    return "Indiquez la raison sociale (au moins 2 caractères).";
  }
  const siren = input.siren.replace(/\s/g, "");
  if (!/^\d{9}$/.test(siren)) {
    return "Le SIREN doit contenir exactement 9 chiffres.";
  }
  const siret = input.siret.replace(/\s/g, "");
  if (siret && !/^\d{14}$/.test(siret)) {
    return "Le SIRET doit contenir exactement 14 chiffres (ou rester vide).";
  }
  if (siret && !siret.startsWith(siren)) {
    return "Le SIRET doit commencer par le SIREN saisi.";
  }
  if (input.capital.trim() && Number.isNaN(Number(input.capital.replace(/\s/g, "").replace(",", ".").replace("€", "")))) {
    return "Capital : indiquez un montant en euros (ex. 1000 ou 1 000,00).";
  }
  if (input.displayName.trim().length < 2) {
    return "Indiquez un profil local (ex. Dirigeant).";
  }
  if (!input.label.trim()) {
    return "Indiquez le libellé d’exercice (ex. 2026).";
  }
  if (!/^\d{4}-\d{2}-\d{2}$/.test(input.startDate) || !/^\d{4}-\d{2}-\d{2}$/.test(input.endDate)) {
    return "Les dates d’exercice doivent être au format AAAA-MM-JJ.";
  }
  if (input.startDate > input.endDate) {
    return "La date de début doit précéder la date de fin.";
  }
  return null;
}

function buildPayload(form: {
  name: string;
  siren: string;
  siret: string;
  legalForm: string;
  vatRegime: string;
  address: string;
  capital: string;
  displayName: string;
  label: string;
  startDate: string;
  endDate: string;
}): UpdateOrganizationInput {
  return {
    name: form.name.trim(),
    siren: form.siren.replace(/\s/g, ""),
    siret: form.siret.replace(/\s/g, "") || null,
    legalForm: form.legalForm,
    vatRegime: form.vatRegime,
    address: form.address.trim() || null,
    capitalCents: parseEurosToCents(form.capital),
    displayName: form.displayName.trim(),
    fiscalYearLabel: form.label.trim(),
    startDate: form.startDate,
    endDate: form.endDate,
  };
}

export function MaSasScreen({
  workspace,
  onWorkspaceChange,
}: {
  workspace: WorkspaceInfo;
  onWorkspaceChange: (w: WorkspaceInfo) => void;
}) {
  const { organization: org, fiscalYear: fy, profile } = workspace;
  const fyOpen = fy.status === "open";
  const isPreview = workspace.path.startsWith("/preview/");

  const [editing, setEditing] = useState(false);
  const [name, setName] = useState(org.name);
  const [siren, setSiren] = useState(org.siren);
  const [siret, setSiret] = useState(org.siret ?? "");
  const [legalForm, setLegalForm] = useState(org.legalForm || "SAS");
  const [vatRegime, setVatRegime] = useState(normalizeVat(org.vatRegime));
  const [address, setAddress] = useState(org.address ?? "");
  const [capital, setCapital] = useState(centsToEuroInput(org.capitalCents));
  const [displayName, setDisplayName] = useState(profile.displayName);
  const [label, setLabel] = useState(fy.label);
  const [startDate, setStartDate] = useState(fy.startDate);
  const [endDate, setEndDate] = useState(fy.endDate);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (editing) return;
    setName(org.name);
    setSiren(org.siren);
    setSiret(org.siret ?? "");
    setLegalForm(org.legalForm || "SAS");
    setVatRegime(normalizeVat(org.vatRegime));
    setAddress(org.address ?? "");
    setCapital(centsToEuroInput(org.capitalCents));
    setDisplayName(profile.displayName);
    setLabel(fy.label);
    setStartDate(fy.startDate);
    setEndDate(fy.endDate);
  }, [org, fy, profile, editing]);

  const liveHint = useMemo(
    () =>
      validateForm({
        name,
        siren,
        siret,
        capital,
        displayName,
        label,
        startDate,
        endDate,
      }),
    [name, siren, siret, capital, displayName, label, startDate, endDate],
  );

  function startEdit() {
    setError(null);
    setStatus(null);
    setEditing(true);
  }

  function cancelEdit() {
    setEditing(false);
    setError(null);
    setStatus(null);
  }

  async function save() {
    setError(null);
    setStatus(null);
    const v = validateForm({
      name,
      siren,
      siret,
      capital,
      displayName,
      label,
      startDate,
      endDate,
    });
    if (v) {
      setError(v);
      return;
    }
    const payload = buildPayload({
      name,
      siren,
      siret,
      legalForm,
      vatRegime,
      address,
      capital,
      displayName,
      label,
      startDate,
      endDate,
    });
    setBusy(true);
    try {
      if (isPreview) {
        onWorkspaceChange({
          ...workspace,
          organization: {
            ...org,
            name: payload.name,
            siren: payload.siren,
            siret: payload.siret ?? null,
            legalForm: payload.legalForm,
            vatRegime: payload.vatRegime,
            address: payload.address ?? null,
            capitalCents: payload.capitalCents,
          },
          fiscalYear: fyOpen
            ? {
                ...fy,
                label: payload.fiscalYearLabel,
                startDate: payload.startDate,
                endDate: payload.endDate,
              }
            : fy,
          profile: { ...profile, displayName: payload.displayName },
        });
      } else {
        const info = await api.updateOrganization(payload);
        onWorkspaceChange(info);
      }
      setEditing(false);
      setStatus("Paramètres enregistrés.");
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <div className="panel">
        <div className="page-head">
          <div>
            <h1>Ma SAS</h1>
            <p>
              Fiche société et exercice courant. Ces infos alimentent les
              exports, le pack de comptes et les rappels de déclaration.
            </p>
          </div>
          {!editing && (
            <button type="button" className="primary" onClick={startEdit}>
              Modifier les paramètres
            </button>
          )}
        </div>

        <div className="help-callout">
          <strong>Pour débuter</strong>
          <p>
            Recopiez raison sociale, SIREN/SIRET, capital et adresse depuis
            votre <strong>Kbis</strong> (ou l’extrait INSEE). Survolez ou
            cliquez sur « ? » pour une explication simple de chaque champ.
          </p>
        </div>

        {status && <div className="ok-banner">{status}</div>}
        {error && <div className="error">{error}</div>}

        {!editing ? (
          <div className="meta-grid">
            <div className="meta-item">
              <span>
                Raison sociale <FieldHelp topic="raisonSociale" compact />
              </span>
              <strong>{org.name}</strong>
            </div>
            <div className="meta-item">
              <span>
                SIREN <FieldHelp topic="siren" compact />
              </span>
              <strong>{org.siren}</strong>
            </div>
            <div className="meta-item">
              <span>
                SIRET <FieldHelp topic="siret" compact />
              </span>
              <strong>{org.siret || "—"}</strong>
            </div>
            <div className="meta-item">
              <span>
                Forme <FieldHelp topic="formeJuridique" compact />
              </span>
              <strong>{org.legalForm}</strong>
            </div>
            <div className="meta-item">
              <span>
                Régime TVA <FieldHelp topic="regimeTva" compact />
              </span>
              <strong>{vatLabel(org.vatRegime)}</strong>
            </div>
            <div className="meta-item">
              <span>
                Capital <FieldHelp topic="capital" compact />
              </span>
              <strong>{formatEuros(org.capitalCents)}</strong>
            </div>
            <div className="meta-item" style={{ gridColumn: "1 / -1" }}>
              <span>
                Adresse du siège <FieldHelp topic="adresse" compact />
              </span>
              <strong style={{ whiteSpace: "pre-wrap" }}>
                {org.address || "—"}
              </strong>
            </div>
            <div className="meta-item">
              <span>
                Exercice <FieldHelp topic="exercice" compact />
              </span>
              <strong>
                {fy.label} ({fy.startDate} → {fy.endDate})
              </strong>
            </div>
            <div className="meta-item">
              <span>Statut exercice</span>
              <strong>{fy.status}</strong>
            </div>
            <div className="meta-item">
              <span>
                Profil local <FieldHelp topic="profilLocal" compact />
              </span>
              <strong>{profile.displayName}</strong>
            </div>
            <div className="meta-item">
              <span>Fichier</span>
              <strong style={{ fontSize: "0.85rem", wordBreak: "break-all" }}>
                {workspace.path}
              </strong>
            </div>
          </div>
        ) : (
          <form
            className="stack"
            onSubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <section className="form-section">
              <h2>Société</h2>
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="raisonSociale">Raison sociale</LabelWithHelp>
                  <input
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    autoComplete="organization"
                    required
                  />
                </label>
                <label>
                  <LabelWithHelp topic="formeJuridique">Forme juridique</LabelWithHelp>
                  <select
                    value={legalForm}
                    onChange={(e) => setLegalForm(e.target.value)}
                  >
                    {LEGAL_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  <LabelWithHelp topic="siren">SIREN (9 chiffres)</LabelWithHelp>
                  <input
                    value={siren}
                    onChange={(e) => setSiren(e.target.value)}
                    inputMode="numeric"
                    placeholder="123456782"
                    required
                  />
                </label>
                <label>
                  <LabelWithHelp topic="siret">SIRET (14 chiffres, optionnel)</LabelWithHelp>
                  <input
                    value={siret}
                    onChange={(e) => setSiret(e.target.value)}
                    inputMode="numeric"
                    placeholder="12345678200002"
                  />
                </label>
                <label>
                  <LabelWithHelp topic="capital">Capital social (€)</LabelWithHelp>
                  <input
                    value={capital}
                    onChange={(e) => setCapital(e.target.value)}
                    inputMode="decimal"
                    placeholder="1000,00"
                  />
                </label>
                <label>
                  <LabelWithHelp topic="regimeTva">Régime de TVA</LabelWithHelp>
                  <select
                    value={vatRegime}
                    onChange={(e) => setVatRegime(e.target.value)}
                  >
                    {VAT_OPTIONS.map((o) => (
                      <option key={o.value} value={o.value}>
                        {o.label}
                      </option>
                    ))}
                  </select>
                </label>
                <label style={{ gridColumn: "1 / -1" }}>
                  <LabelWithHelp topic="adresse">Adresse du siège</LabelWithHelp>
                  <textarea
                    value={address}
                    onChange={(e) => setAddress(e.target.value)}
                    rows={3}
                    placeholder={"12 rue des Lilas\n75011 Paris"}
                  />
                </label>
              </div>
            </section>

            <section className="form-section">
              <h2>Exercice courant</h2>
              {!fyOpen && (
                <p className="hint">
                  Exercice « {fy.status} » : les dates sont en lecture seule.
                  Relancez une clôture / un nouvel exercice pour changer la
                  période.
                </p>
              )}
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="exercice">Libellé</LabelWithHelp>
                  <input
                    value={label}
                    onChange={(e) => setLabel(e.target.value)}
                    disabled={!fyOpen}
                    required
                  />
                </label>
                <label>
                  Statut
                  <input value={fy.status} disabled readOnly />
                </label>
                <label>
                  Date de début
                  <input
                    type="date"
                    value={startDate}
                    onChange={(e) => setStartDate(e.target.value)}
                    disabled={!fyOpen}
                    required
                  />
                </label>
                <label>
                  Date de fin
                  <input
                    type="date"
                    value={endDate}
                    onChange={(e) => setEndDate(e.target.value)}
                    disabled={!fyOpen}
                    required
                  />
                </label>
              </div>
              {fyOpen && (
                <p className="hint">
                  Modifier les dates ne réécrit pas les écritures déjà saisies :
                  vérifiez qu’elles restent dans la période.
                </p>
              )}
            </section>

            <section className="form-section">
              <h2>Profil local</h2>
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="profilLocal">Nom affiché</LabelWithHelp>
                  <input
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                    required
                  />
                </label>
                <label>
                  Fichier workspace
                  <input value={workspace.path} disabled readOnly />
                </label>
              </div>
            </section>

            {liveHint && <p className="welcome-live-hint hint">{liveHint}</p>}

            <div className="btn-row">
              <button type="submit" className="primary" disabled={busy || !!liveHint}>
                {busy ? "Enregistrement…" : "Enregistrer"}
              </button>
              <button
                type="button"
                className="ghost"
                disabled={busy}
                onClick={cancelEdit}
              >
                Annuler
              </button>
            </div>
          </form>
        )}
      </div>
      <DeadlineReminders workspace={workspace} />
    </div>
  );
}

function normalizeVat(raw: string): string {
  const r = raw.toLowerCase();
  if (r.includes("franchise")) return "franchise";
  if (r.includes("trimest")) return "reel_trimestriel";
  if (r.includes("mensuel") || r.includes("reel_mensuel") || r.includes("réel")) {
    return "reel_mensuel";
  }
  // legacy "réel normal" → mensuel
  return "reel_mensuel";
}

function vatLabel(raw: string): string {
  const v = normalizeVat(raw);
  return VAT_OPTIONS.find((o) => o.value === v)?.label ?? raw;
}

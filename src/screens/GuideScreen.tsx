import { useEffect, useMemo, useState } from "react";
import { FieldHelp } from "../components/FieldHelp";
import { DeadlineReminders } from "../components/DeadlineReminders";
import { api } from "../lib/api";
import {
  GUIDE_STEPS,
  loadGuideProgress,
  markStepDone,
  type GuideStepId,
} from "../lib/guideProgress";
import { HELP } from "../lib/glossary";
import { formatEuros } from "../lib/money";
import type { NavId, WorkspaceInfo } from "../lib/types";

type Props = {
  workspace: WorkspaceInfo;
  onNavigate: (id: NavId) => void;
};

const STEP_META: Record<
  GuideStepId,
  { title: string; blurb: string; nav?: NavId }
> = {
  societe: {
    title: "Votre société",
    blurb: "Vérifiez l’identité de la SAS et l’exercice ouvert.",
    nav: "ma-sas",
  },
  plan: {
    title: "Plan comptable",
    blurb: "Comprenez les comptes PCG déjà préchargés pour une SAS.",
    nav: "plan",
  },
  ecritures: {
    title: "Premières écritures",
    blurb: "Saisissez une opération simple en partie double.",
    nav: "ecritures",
  },
  tva: {
    title: "TVA",
    blurb: "Reliez les codes TVA au brouillon de déclaration CA3.",
    nav: "tva",
  },
  bilan: {
    title: "Bilan & résultat",
    blurb: "Lisez le bilan et le compte de résultat de l’exercice.",
    nav: "etats",
  },
};

type Checks = {
  accountCount: number;
  postedEntries: number;
  draftEntries: number;
  hasVatLines: boolean;
  bilanEcart: number | null;
};

export function GuideScreen({ workspace, onNavigate }: Props) {
  const [step, setStep] = useState<GuideStepId>("societe");
  const [progress, setProgress] = useState(() =>
    loadGuideProgress(workspace.path),
  );
  const [checks, setChecks] = useState<Checks | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  async function refreshChecks() {
    setError(null);
    const isPreview = workspace.path.startsWith("/preview/");
    if (isPreview) {
      setChecks({
        accountCount: 48,
        postedEntries: 0,
        draftEntries: 0,
        hasVatLines: false,
        bilanEcart: 0,
      });
      return;
    }
    try {
      const [accounts, entries, vat] = await Promise.all([
        api.listAccounts(),
        api.listEntries(),
        api.vatReport(
          workspace.fiscalYear.startDate,
          workspace.fiscalYear.endDate,
        ),
      ]);
      let bilanEcart: number | null = null;
      try {
        const stmts = await api.annualStatements({
          fromDate: workspace.fiscalYear.startDate,
          toDate: workspace.fiscalYear.endDate,
          journalCode: null,
          accountNumber: null,
        });
        bilanEcart = stmts.bilan.ecartCents;
      } catch {
        bilanEcart = null;
      }
      setChecks({
        accountCount: accounts.length,
        postedEntries: entries.filter((e) => e.status === "posted").length,
        draftEntries: entries.filter((e) => e.status === "draft").length,
        hasVatLines: vat.lines.some((l) => l.baseCents !== 0 || l.vatCents !== 0),
        bilanEcart,
      });
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refreshChecks();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [workspace.path]);

  const idx = GUIDE_STEPS.indexOf(step);
  const doneCount = progress.completed.length;
  const pct = Math.round((doneCount / GUIDE_STEPS.length) * 100);

  const validation = useMemo(
    () => validateStep(step, workspace, checks),
    [step, workspace, checks],
  );

  function completeCurrent() {
    if (!validation.ok) return;
    const next = markStepDone(workspace.path, step);
    setProgress(next);
    const nextIdx = idx + 1;
    if (nextIdx < GUIDE_STEPS.length) {
      setStep(GUIDE_STEPS[nextIdx]);
    }
  }

  async function createSampleSale() {
    setBusy(true);
    setError(null);
    try {
      if (workspace.path.startsWith("/preview/")) {
        setChecks({
          accountCount: 48,
          postedEntries: 1,
          draftEntries: 0,
          hasVatLines: true,
          bilanEcart: 0,
        });
        setProgress(markStepDone(workspace.path, "ecritures"));
        return;
      }
      const journals = await api.listJournals();
      const journalCode = journals.find((j) => j.code === "VTE")?.code ?? "OD";
      const date = workspace.fiscalYear.startDate;
      const entry = await api.createEntry({
        journalCode,
        entryDate: date,
        pieceRef: "DEMO-001",
        label: "Vente prestation démo TTC 120 € (HT 100 + TVA 20)",
        lines: [
          {
            accountNumber: "411",
            debitCents: 12000,
            creditCents: 0,
            vatCode: null,
          },
          {
            accountNumber: "706",
            debitCents: 0,
            creditCents: 10000,
            vatCode: "T20C",
          },
          {
            accountNumber: "44571",
            debitCents: 0,
            creditCents: 2000,
            vatCode: "T20C",
          },
        ],
      });
      await api.postEntry(entry.id);
      await refreshChecks();
      setProgress(markStepDone(workspace.path, "ecritures"));
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="guide-layout">
      <section className="panel guide-hero-panel">
        <div className="guide-hero-copy">
          <p className="eyebrow">Parcours débutant</p>
          <h1>Comptabilité SAS, pas à pas</h1>
          <p>
            Cinq étapes pour démarrer sans jargon opaque : société, plan,
            écritures, TVA, puis bilan. Chaque champ important a une aide « ? ».
          </p>
          <div className="guide-progress-bar" aria-hidden>
            <div style={{ width: `${pct}%` }} />
          </div>
          <p className="hint">
            {doneCount}/{GUIDE_STEPS.length} étapes cochées ({pct} %)
          </p>
        </div>
        <ol className="guide-steps">
          {GUIDE_STEPS.map((id, i) => {
            const meta = STEP_META[id];
            const done = progress.completed.includes(id);
            return (
              <li key={id}>
                <button
                  type="button"
                  className={[
                    "guide-step-btn",
                    step === id ? "active" : "",
                    done ? "done" : "",
                  ]
                    .filter(Boolean)
                    .join(" ")}
                  onClick={() => setStep(id)}
                >
                  <span className="guide-step-num">{done ? "✓" : i + 1}</span>
                  <span>
                    <strong>{meta.title}</strong>
                    <small>{meta.blurb}</small>
                  </span>
                </button>
              </li>
            );
          })}
        </ol>
      </section>

      <section className="panel guide-body">
        <header className="page-head">
          <div>
            <h2>
              Étape {idx + 1} — {STEP_META[step].title}
            </h2>
            <p>{STEP_META[step].blurb}</p>
          </div>
          {STEP_META[step].nav && (
            <button
              type="button"
              className="secondary"
              onClick={() => onNavigate(STEP_META[step].nav!)}
            >
              Ouvrir l’écran dédié
            </button>
          )}
        </header>

        {error && <div className="error">{error}</div>}

        {step === "societe" && (
          <SocieteStep workspace={workspace} onNavigate={onNavigate} />
        )}
        {step === "plan" && <PlanStep checks={checks} onNavigate={onNavigate} />}
        {step === "ecritures" && (
          <EcrituresStep
            checks={checks}
            busy={busy}
            onSample={() => void createSampleSale()}
            onNavigate={onNavigate}
          />
        )}
        {step === "tva" && <TvaStep workspace={workspace} checks={checks} />}
        {step === "bilan" && <BilanStep checks={checks} onNavigate={onNavigate} />}

        <div
          className={`guide-validation ${validation.ok ? "ok" : "wait"}`}
          role="status"
        >
          {validation.message}
        </div>

        <div className="btn-row">
          <button
            type="button"
            className="ghost"
            disabled={idx === 0}
            onClick={() => setStep(GUIDE_STEPS[idx - 1])}
          >
            Précédent
          </button>
          <button
            type="button"
            className="primary"
            disabled={!validation.ok}
            onClick={completeCurrent}
          >
            {idx === GUIDE_STEPS.length - 1
              ? "Terminer cette étape"
              : "Valider et continuer"}
          </button>
          {idx < GUIDE_STEPS.length - 1 && (
            <button
              type="button"
              className="ghost"
              onClick={() => setStep(GUIDE_STEPS[idx + 1])}
            >
              Passer pour l’instant
            </button>
          )}
        </div>
      </section>

      <section className="panel guide-roadmap" aria-label="Modules à venir">
        <h2>Ensuite : Commercial, E-facture, RH</h2>
        <p>
          La comptabilité reste le cœur. <strong>Commercial</strong> (devis /
          factures) et <strong>RH</strong> (salariés, congés, paie simplifiée)
          sont branchés. Relances, e-facture PDP et dépôt DSN restent dans les
          jalons suivants.
        </p>
        <ul>
          <li>Devis → factures → écritures VTE brouillon (relances bientôt)</li>
          <li>Factur-X local et file d’attente PDP (SasCompta n’est pas une PDP)</li>
          <li>
            Salariés / contrats / congés / bulletins + OD paie — export
            DSN-oriented assisté (dépôt = plus tard)
          </li>
        </ul>
      </section>

      <DeadlineReminders workspace={workspace} />
    </div>
  );
}

function SocieteStep({
  workspace,
  onNavigate,
}: {
  workspace: WorkspaceInfo;
  onNavigate: (id: NavId) => void;
}) {
  const org = workspace.organization;
  const fy = workspace.fiscalYear;
  return (
    <div className="guide-card-block">
      <p>
        Votre fichier workspace contient déjà la fiche société. Relisez ces
        informations : elles serviront aux exports et au dépôt des comptes.
        Pour corriger un champ, ouvrez <strong>Ma SAS</strong> → « Modifier les
        paramètres ».
      </p>
      <div className="meta-grid">
        <div className="meta-item">
          <span>
            <LabelHelp topic="raisonSociale" label="Raison sociale" />
          </span>
          <strong>{org.name}</strong>
        </div>
        <div className="meta-item">
          <span>
            <LabelHelp topic="siren" label="SIREN" />
          </span>
          <strong>{org.siren}</strong>
        </div>
        <div className="meta-item">
          <span>
            <LabelHelp topic="siret" label="SIRET" />
          </span>
          <strong>{org.siret || "—"}</strong>
        </div>
        <div className="meta-item">
          <span>
            <LabelHelp topic="regimeTva" label="Régime TVA" />
          </span>
          <strong>{org.vatRegime}</strong>
        </div>
        <div className="meta-item">
          <span>
            <LabelHelp topic="exercice" label="Exercice" />
          </span>
          <strong>
            {fy.label} · {fy.startDate} → {fy.endDate}
          </strong>
        </div>
      </div>
      <div className="btn-row">
        <button
          type="button"
          className="secondary"
          onClick={() => onNavigate("ma-sas")}
        >
          Ouvrir Ma SAS pour éditer
        </button>
      </div>
    </div>
  );
}

function PlanStep({
  checks,
  onNavigate,
}: {
  checks: Checks | null;
  onNavigate: (id: NavId) => void;
}) {
  return (
    <div className="guide-card-block">
      <p>
        Le <LabelHelp topic="planComptable" label="plan comptable" /> est déjà
        chargé (classes 1 à 7). Vous n’avez pas à tout créer : ouvrez l’écran
        pour parcourir banque (512), clients (411), ventes (706)…
      </p>
      <div className="summary-strip">
        <span>
          Comptes chargés{" "}
          <strong>{checks?.accountCount ?? "…"}</strong>
        </span>
      </div>
      <ul className="guide-examples">
        <li>
          <code>512</code> — Banque : l’argent sur le compte courant
        </li>
        <li>
          <code>411</code> — Clients : ce que l’on vous doit
        </li>
        <li>
          <code>401</code> — Fournisseurs : ce que vous devez
        </li>
        <li>
          <code>706</code> — Prestations de services (chiffre d’affaires)
        </li>
      </ul>
      <button type="button" className="secondary" onClick={() => onNavigate("plan")}>
        Voir le plan comptable
      </button>
    </div>
  );
}

function EcrituresStep({
  checks,
  busy,
  onSample,
  onNavigate,
}: {
  checks: Checks | null;
  busy: boolean;
  onSample: () => void;
  onNavigate: (id: NavId) => void;
}) {
  return (
    <div className="guide-card-block">
      <p>
        Une écriture raconte un fait économique en{" "}
        <LabelHelp topic="debitCredit" label="débit / crédit" />. Les deux
        colonnes doivent être égales. Exemple : vente 120 € TTC = client 120 /
        prestation 100 + TVA 20. Sur Écritures, la{" "}
        <LabelHelp topic="saisieGuidee" label="saisie guidée" /> propose aussi
        l’
        <LabelHelp topic="autoliquidationBtp" label="autoliquidation BTP" />{" "}
        (sous-traitance chantier ou travaux facturés HT).
      </p>
      <div className="summary-strip">
        <span>
          Validées <strong>{checks?.postedEntries ?? 0}</strong>
        </span>
        <span>
          Brouillons <strong>{checks?.draftEntries ?? 0}</strong>
        </span>
      </div>
      <div className="btn-row">
        <button
          type="button"
          className="primary"
          disabled={busy}
          onClick={onSample}
        >
          Créer l’exemple « vente 120 € »
        </button>
        <button type="button" className="ghost" onClick={() => onNavigate("ecritures")}>
          Saisir moi-même
        </button>
      </div>
    </div>
  );
}

function TvaStep({
  workspace,
  checks,
}: {
  workspace: WorkspaceInfo;
  checks: Checks | null;
}) {
  return (
    <div className="guide-card-block">
      <p>
        Sur chaque ligne concernée, posez un{" "}
        <LabelHelp topic="codeTva" label="code TVA" />. Le brouillon{" "}
        <LabelHelp topic="ca3" label="CA3" /> regroupe ensuite collectée et
        déductible pour la période.
      </p>
      <div className="summary-strip">
        <span>
          Régime <strong>{workspace.organization.vatRegime}</strong>
        </span>
        <span>
          Mouvements TVA détectés{" "}
          <strong>{checks?.hasVatLines ? "oui" : "pas encore"}</strong>
        </span>
      </div>
      <p className="hint">
        Astuce : après l’exemple de vente, ouvrez TVA &amp; tiers → Brouillon
        CA3 pour voir la TVA collectée.
      </p>
    </div>
  );
}

function BilanStep({
  checks,
  onNavigate,
}: {
  checks: Checks | null;
  onNavigate: (id: NavId) => void;
}) {
  return (
    <div className="guide-card-block">
      <p>
        Le <LabelHelp topic="bilan" label="bilan" /> montre le patrimoine ; le{" "}
        <LabelHelp topic="compteResultat" label="compte de résultat" /> montre
        l’activité de l’année. L’écart actif/passif doit tendre vers zéro après
        clôture.
      </p>
      <div className="summary-strip">
        <span>
          Écart bilan{" "}
          <strong>
            {checks?.bilanEcart == null
              ? "…"
              : formatEuros(checks.bilanEcart)}
          </strong>
        </span>
      </div>
      <button type="button" className="secondary" onClick={() => onNavigate("etats")}>
        Ouvrir les états annuels
      </button>
    </div>
  );
}

function LabelHelp({
  topic,
  label,
}: {
  topic: keyof typeof HELP;
  label: string;
}) {
  return (
    <span className="inline-help">
      {label} <FieldHelp topic={topic} compact />
    </span>
  );
}

function validateStep(
  step: GuideStepId,
  workspace: WorkspaceInfo,
  checks: Checks | null,
): { ok: boolean; message: string } {
  switch (step) {
    case "societe": {
      const sirenOk = /^\d{9}$/.test(workspace.organization.siren.replace(/\s/g, ""));
      const nameOk = workspace.organization.name.trim().length >= 2;
      if (!nameOk)
        return { ok: false, message: "La raison sociale semble incomplète." };
      if (!sirenOk)
        return {
          ok: false,
          message: "Le SIREN doit comporter 9 chiffres (contrôle local).",
        };
      return {
        ok: true,
        message: "Fiche société lisible — vous pouvez continuer.",
      };
    }
    case "plan":
      if (!checks)
        return { ok: false, message: "Chargement du plan comptable…" };
      if (checks.accountCount < 10)
        return {
          ok: false,
          message: "Le plan semble incomplet — rechargez le workspace.",
        };
      return {
        ok: true,
        message: `${checks.accountCount} comptes disponibles. Parcourez-les quand vous voulez.`,
      };
    case "ecritures":
      if (!checks) return { ok: false, message: "Chargement des écritures…" };
      if (checks.postedEntries + checks.draftEntries < 1)
        return {
          ok: false,
          message:
            "Créez au moins une écriture (bouton exemple ci-dessus) pour valider l’étape.",
        };
      return {
        ok: true,
        message: "Au moins une écriture est présente — bravo.",
      };
    case "tva":
      if (!checks) return { ok: false, message: "Analyse TVA…" };
      if (!checks.hasVatLines && checks.postedEntries === 0)
        return {
          ok: false,
          message:
            "Passez d’abord par une écriture avec code TVA (ex. vente démo).",
        };
      return {
        ok: true,
        message: checks.hasVatLines
          ? "Des bases TVA sont visibles dans le brouillon."
          : "Étape accessible : ouvrez TVA & tiers pour explorer les codes même sans flux.",
      };
    case "bilan":
      if (!checks) return { ok: false, message: "Calcul du bilan…" };
      return {
        ok: true,
        message:
          checks.bilanEcart === 0
            ? "Bilan équilibré (écart 0)."
            : "Bilan consultable — l’écart se résorbe avec les écritures et la clôture.",
      };
  }
}

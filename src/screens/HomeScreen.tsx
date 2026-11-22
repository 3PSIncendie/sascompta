import { useEffect, useState } from "react";
import logo3psi from "../assets/brand/3psi-logo.png";
import { api } from "../lib/api";
import {
  homeKpiFilters,
  mergeHomeKpis,
  previewHomeKpis,
  type HomeKpis,
  type HomePeriod,
} from "../lib/homeKpis";
import { formatEuros } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import type { NavId, WorkspaceInfo } from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

interface Props {
  workspace: WorkspaceInfo;
  onNavigate: (id: NavId) => void;
}

export function HomeScreen({ workspace, onNavigate }: Props) {
  const [period, setPeriod] = useState<HomePeriod>(
    () => homeKpiFilters(workspace.fiscalYear).period,
  );
  const [kpis, setKpis] = useState<HomeKpis | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    let cancelled = false;
    const { period: nextPeriod, month, fyToDate } = homeKpiFilters(
      workspace.fiscalYear,
    );
    setPeriod(nextPeriod);

    async function load() {
      setLoading(true);
      setError(null);
      if (PREVIEW || !isTauriRuntime()) {
        if (!cancelled) {
          setKpis(previewHomeKpis());
          setLoading(false);
        }
        return;
      }
      try {
        // Both packs are scoped to the current exercice id — prior years stay out.
        const [monthPack, fyPack] = await Promise.all([
          api.annualStatements(month),
          api.annualStatements(fyToDate),
        ]);
        if (!cancelled) {
          setKpis(mergeHomeKpis(monthPack, fyPack));
        }
      } catch (e) {
        if (!cancelled) {
          setKpis(null);
          setError(String(e));
        }
      } finally {
        if (!cancelled) setLoading(false);
      }
    }

    void load();
    return () => {
      cancelled = true;
    };
  }, [workspace.fiscalYear, workspace.path]);

  const org = workspace.organization;

  return (
    <div className="stack home-layout">
      <section className="panel home-hero" aria-label="Présentation société">
        <div className="home-hero-brand">
          <img
            className="home-hero-logo"
            src={logo3psi}
            alt={`Logo ${org.name}`}
            width={160}
            height={160}
          />
          <div className="home-hero-copy">
            <p className="eyebrow">Accueil</p>
            <h1 className="home-hero-title">{org.name}</h1>
            <p className="home-hero-sub">
              {org.legalForm}
              {org.siren ? ` · SIREN ${org.siren}` : ""}
              {" · "}
              Exercice {period.fyLabel}
              {workspace.fiscalYear.status === "open" ? " (ouvert)" : ""}
            </p>
          </div>
        </div>
        <div className="home-hero-actions">
          <button type="button" onClick={() => onNavigate("ma-sas")}>
            Ma SAS
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => onNavigate("guide")}
          >
            Parcours guidé
          </button>
          <button
            type="button"
            className="ghost"
            onClick={() => onNavigate("etats")}
          >
            États annuels
          </button>
        </div>
      </section>

      <section className="panel" aria-label="Chiffres clés du mois">
        <header className="page-head">
          <div>
            <h2>Chiffres clés du mois</h2>
            <p className="muted">
              Du {formatFrDate(period.monthFrom)} au{" "}
              {formatFrDate(period.monthTo)} — mois en cours de l’exercice{" "}
              {period.fyLabel}. CA / charges / résultat = compte de résultat
              (écritures validées). Trésorerie = stock bilan (banque + caisse +
              autres disponibilités) à la date du jour.
            </p>
          </div>
        </header>

        {error && <p className="error-banner">{error}</p>}
        {loading && !kpis && <p className="muted">Chargement des indicateurs…</p>}

        {kpis && (
          <div className="home-kpi-grid" role="list">
            <KpiCard
              label="Chiffre d’affaires"
              hint="Produits du mois"
              value={kpis.caCents}
              tone="brand"
            />
            <KpiCard
              label="Charges"
              hint="Charges du mois"
              value={kpis.chargesCents}
              tone="neutral"
            />
            <KpiCard
              label="Résultat"
              hint="Produits − charges"
              value={kpis.resultatCents}
              tone={kpis.resultatCents >= 0 ? "ok" : "danger"}
            />
            <KpiCard
              label="Trésorerie"
              hint="Banque + caisse"
              value={kpis.tresorerieCents}
              tone="accent"
            />
          </div>
        )}
      </section>
    </div>
  );
}

function KpiCard({
  label,
  hint,
  value,
  tone,
}: {
  label: string;
  hint: string;
  value: number;
  tone: "brand" | "accent" | "ok" | "danger" | "neutral";
}) {
  return (
    <article className={`home-kpi home-kpi--${tone}`} role="listitem">
      <p className="home-kpi-label">{label}</p>
      <p className="home-kpi-value">{formatEuros(value)}</p>
      <p className="home-kpi-hint">{hint}</p>
    </article>
  );
}

function formatFrDate(ymd: string): string {
  const [y, m, d] = ymd.split("-").map(Number);
  return new Intl.DateTimeFormat("fr-FR", {
    day: "numeric",
    month: "long",
    year: "numeric",
  }).format(new Date(y, m - 1, d));
}

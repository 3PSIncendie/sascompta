/** Calendrier local des échéances déclaratives (indicatives, SAS FR). */

export type DeadlineKind = "tva" | "cloture" | "depot" | "ag" | "einvoice";

export type DeadlineItem = {
  id: string;
  kind: DeadlineKind;
  title: string;
  dueDate: string; // YYYY-MM-DD
  detail: string;
  severity: "info" | "soon" | "overdue";
};

export type DeadlineInput = {
  today: string; // YYYY-MM-DD
  fiscalYearEnd: string;
  vatRegime: string; // mensuel | trimestriel | franchise | …
};

function parseYmd(ymd: string): Date {
  const [y, m, d] = ymd.split("-").map(Number);
  return new Date(y, m - 1, d);
}

function formatYmd(d: Date): string {
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, "0");
  const day = String(d.getDate()).padStart(2, "0");
  return `${y}-${m}-${day}`;
}

function addMonths(d: Date, n: number): Date {
  const out = new Date(d.getFullYear(), d.getMonth() + n, d.getDate());
  return out;
}

function daysBetween(a: string, b: string): number {
  const ms = parseYmd(b).getTime() - parseYmd(a).getTime();
  return Math.round(ms / 86_400_000);
}

function severityFor(today: string, due: string): DeadlineItem["severity"] {
  const delta = daysBetween(today, due);
  if (delta < 0) return "overdue";
  if (delta <= 21) return "soon";
  return "info";
}

/** 19 du mois suivant (approx. échéance CA3 mensuelle hors week-ends). */
export function nextMonthlyVatDue(today: string): string {
  const t = parseYmd(today);
  // Déclare le mois civil précédent → échéance le 19 du mois courant ;
  // si on a déjà dépassé le 19, prochain = 19 du mois suivant.
  const candidate = new Date(t.getFullYear(), t.getMonth(), 19);
  if (t.getDate() <= 19) return formatYmd(candidate);
  return formatYmd(new Date(t.getFullYear(), t.getMonth() + 1, 19));
}

/** Trimestres civils : échéance ~19 du mois suivant la fin de trimestre. */
export function nextQuarterlyVatDue(today: string): string {
  const t = parseYmd(today);
  const quarterEnds = [2, 5, 8, 11]; // mars, juin, sept, déc (0-index months)
  for (let year = t.getFullYear(); year <= t.getFullYear() + 1; year++) {
    for (const m of quarterEnds) {
      const due = new Date(year, m + 1, 19); // mois suivant la fin de trimestre
      if (due >= t) return formatYmd(due);
    }
  }
  return formatYmd(addMonths(t, 3));
}

/**
 * Construit la liste d’échéances à afficher.
 * Les dates sont indicatives (pas de calendrier fiscal officiel ni jours fériés).
 */
export function buildDeadlines(input: DeadlineInput): DeadlineItem[] {
  const { today, fiscalYearEnd, vatRegime } = input;
  const items: DeadlineItem[] = [];
  const regime = vatRegime.toLowerCase();

  if (regime.includes("franchise")) {
    items.push({
      id: "tva-franchise",
      kind: "tva",
      title: "TVA — franchise en base",
      dueDate: fiscalYearEnd,
      detail:
        "Sous franchise, vous ne facturez pas de TVA. Surveillez le seuil de chiffre d’affaires.",
      severity: "info",
    });
  } else if (regime.includes("trimest")) {
    const due = nextQuarterlyVatDue(today);
    items.push({
      id: `tva-ca3-q-${due}`,
      kind: "tva",
      title: "Déclaration TVA (CA3 trimestrielle)",
      dueDate: due,
      detail:
        "Préparez le brouillon dans TVA & tiers, puis déclarez sur impots.gouv.fr.",
      severity: severityFor(today, due),
    });
  } else {
    // défaut : mensuel / réel
    const due = nextMonthlyVatDue(today);
    items.push({
      id: `tva-ca3-m-${due}`,
      kind: "tva",
      title: "Déclaration TVA (CA3 mensuelle)",
      dueDate: due,
      detail:
        "Échéance indicative au 19. Relisez le brouillon CA3 avant envoi.",
      severity: severityFor(today, due),
    });
  }

  const fyEnd = parseYmd(fiscalYearEnd);
  const agDue = formatYmd(addMonths(fyEnd, 6)); // AG dans les 6 mois
  const depotDue = formatYmd(addMonths(fyEnd, 7)); // dépôt ~1 mois après AG max

  items.push({
    id: `cloture-${fiscalYearEnd}`,
    kind: "cloture",
    title: "Clôture de l’exercice",
    dueDate: fiscalYearEnd,
    detail:
      "Passez la checklist Clôture & FEC une fois les écritures à jour.",
    severity: severityFor(today, fiscalYearEnd),
  });

  items.push({
    id: `ag-${agDue}`,
    kind: "ag",
    title: "Assemblée d’approbation des comptes",
    dueDate: agDue,
    detail:
      "En principe dans les 6 mois suivant la clôture. Faites valider le bilan.",
    severity: severityFor(today, agDue),
  });

  items.push({
    id: `depot-${depotDue}`,
    kind: "depot",
    title: "Dépôt des comptes au greffe",
    dueDate: depotDue,
    detail:
      "Dépôt du bilan / compte de résultat / annexe (souvent via guichet unique).",
    severity: severityFor(today, depotDue),
  });

  // Calendrier e-facture indicatif (paramétrable dans Ma SAS — non juridique)
  const receiveDue = `${parseYmd(today).getFullYear()}-09-01`;
  items.push({
    id: `einvoice-receive-${receiveDue}`,
    kind: "einvoice",
    title: "E-facture — phase réception (indicatif)",
    dueDate: receiveDue,
    detail:
      "Préparez imports CII/UBL et la file inbound. Dates ajustables dans Ma SAS ; SasCompta n’est pas une PDP.",
    severity: severityFor(today, receiveDue),
  });

  return items.sort((a, b) => a.dueDate.localeCompare(b.dueDate));
}

export function urgentDeadlines(items: DeadlineItem[]): DeadlineItem[] {
  return items.filter((i) => i.severity === "soon" || i.severity === "overdue");
}

export function formatDueLabel(today: string, due: string): string {
  const d = daysBetween(today, due);
  if (d === 0) return "aujourd’hui";
  if (d === 1) return "demain";
  if (d < 0) return `en retard de ${Math.abs(d)} j`;
  return `dans ${d} j`;
}

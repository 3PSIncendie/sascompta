import { useMemo, useState } from "react";
import {
  buildDeadlines,
  formatDueLabel,
  urgentDeadlines,
  type DeadlineItem,
} from "../lib/deadlines";
import {
  dismissReminder,
  loadGuideProgress,
} from "../lib/guideProgress";
import type { WorkspaceInfo } from "../lib/types";

function todayYmd(): string {
  const d = new Date();
  return [
    d.getFullYear(),
    String(d.getMonth() + 1).padStart(2, "0"),
    String(d.getDate()).padStart(2, "0"),
  ].join("-");
}

type Props = {
  workspace: WorkspaceInfo;
  variant?: "banner" | "panel";
};

export function DeadlineReminders({ workspace, variant = "panel" }: Props) {
  const today = todayYmd();
  const [progress, setProgress] = useState(() =>
    loadGuideProgress(workspace.path),
  );

  const all = useMemo(
    () =>
      buildDeadlines({
        today,
        fiscalYearEnd: workspace.fiscalYear.endDate,
        vatRegime: workspace.organization.vatRegime,
      }),
    [today, workspace.fiscalYear.endDate, workspace.organization.vatRegime],
  );

  const visible = all.filter(
    (i) => !progress.dismissedReminders.includes(i.id),
  );
  const urgent = urgentDeadlines(visible);

  function onDismiss(id: string) {
    setProgress(dismissReminder(workspace.path, id));
  }

  if (variant === "banner") {
    if (urgent.length === 0) return null;
    return (
      <div className="deadline-banner" role="status">
        <div className="deadline-banner-copy">
          <strong>Rappel déclaration</strong>
          <span>
            {urgent[0].title} — {urgent[0].dueDate} (
            {formatDueLabel(today, urgent[0].dueDate)})
          </span>
        </div>
        <button
          type="button"
          className="ghost"
          onClick={() => onDismiss(urgent[0].id)}
        >
          Masquer
        </button>
      </div>
    );
  }

  return (
    <section className="deadline-panel">
      <header className="deadline-panel-head">
        <div>
          <h2>Échéances</h2>
          <p className="hint">
            Calendrier local indicatif (TVA, clôture, dépôt). Vérifiez toujours
            le calendrier officiel des impôts / greffe.
          </p>
        </div>
      </header>
      <ol className="deadline-list">
        {visible.map((item) => (
          <DeadlineRow
            key={item.id}
            item={item}
            today={today}
            onDismiss={() => onDismiss(item.id)}
          />
        ))}
      </ol>
      {visible.length === 0 && (
        <p className="ok">Aucune échéance active — bravo.</p>
      )}
    </section>
  );
}

function DeadlineRow({
  item,
  today,
  onDismiss,
}: {
  item: DeadlineItem;
  today: string;
  onDismiss: () => void;
}) {
  return (
    <li className={`deadline-item severity-${item.severity}`}>
      <div>
        <span className={`deadline-kind kind-${item.kind}`}>{labelKind(item.kind)}</span>
        <strong>{item.title}</strong>
        <p>{item.detail}</p>
      </div>
      <div className="deadline-meta">
        <time dateTime={item.dueDate}>{item.dueDate}</time>
        <span>{formatDueLabel(today, item.dueDate)}</span>
        <button type="button" className="ghost" onClick={onDismiss}>
          OK
        </button>
      </div>
    </li>
  );
}

function labelKind(kind: DeadlineItem["kind"]): string {
  switch (kind) {
    case "tva":
      return "TVA";
    case "cloture":
      return "Clôture";
    case "ag":
      return "AG";
    case "depot":
      return "Dépôt";
    case "einvoice":
      return "E-facture";
  }
}

/** Pastille sidebar : nombre d’échéances urgentes. */
export function DeadlineBadge({ workspace }: { workspace: WorkspaceInfo }) {
  const today = todayYmd();
  const progress = loadGuideProgress(workspace.path);
  const count = urgentDeadlines(
    buildDeadlines({
      today,
      fiscalYearEnd: workspace.fiscalYear.endDate,
      vatRegime: workspace.organization.vatRegime,
    }).filter((i) => !progress.dismissedReminders.includes(i.id)),
  ).length;
  if (count === 0) return null;
  return <span className="nav-badge">{count}</span>;
}

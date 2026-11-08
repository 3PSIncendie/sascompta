import { useEffect, useState } from "react";
import { DeadlineBadge } from "./DeadlineReminders";
import { UpdateChecker } from "./UpdateChecker";
import { PACKAGE_VERSION, resolveAppVersion } from "../lib/appVersion";
import type { NavId, WorkspaceInfo } from "../lib/types";

const NAV: { id: NavId; label: string }[] = [
  { id: "guide", label: "Parcours guidé" },
  { id: "ma-sas", label: "Ma SAS" },
  { id: "plan", label: "Plan comptable" },
  { id: "journaux", label: "Journaux" },
  { id: "ecritures", label: "Écritures" },
  { id: "grand-livre", label: "Grand livre" },
  { id: "tva", label: "TVA & tiers" },
  { id: "immos", label: "Immobilisations" },
  { id: "etats", label: "États annuels" },
  { id: "cloture", label: "Clôture & FEC" },
];

interface Props {
  workspace: WorkspaceInfo;
  nav: NavId;
  onNav: (id: NavId) => void;
  onClose: () => void;
  children: React.ReactNode;
}

export function AppShell({ workspace, nav, onNav, onClose, children }: Props) {
  const [version, setVersion] = useState(PACKAGE_VERSION);

  useEffect(() => {
    let cancelled = false;
    void resolveAppVersion().then((v) => {
      if (!cancelled) setVersion(v);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          SasCompta
          <small>{workspace.organization.name}</small>
        </div>
        <nav className="nav">
          {NAV.map((item) => (
            <button
              key={item.id}
              type="button"
              className={nav === item.id ? "active" : ""}
              onClick={() => onNav(item.id)}
            >
              <span className="nav-label">
                {item.label}
                {item.id === "guide" && <DeadlineBadge workspace={workspace} />}
              </span>
            </button>
          ))}
        </nav>
        <div className="sidebar-footer">
          <UpdateChecker variant="button" />
          <button
            type="button"
            className="ghost sidebar-close"
            onClick={onClose}
          >
            Fermer le workspace
          </button>
          <p className="sidebar-version">Version {version}</p>
        </div>
      </aside>
      <main className="main">{children}</main>
    </div>
  );
}

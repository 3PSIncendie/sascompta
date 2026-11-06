import type { NavId, WorkspaceInfo } from "../lib/types";

const NAV: { id: NavId; label: string }[] = [
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
              {item.label}
            </button>
          ))}
        </nav>
        <div style={{ marginTop: "auto" }}>
          <button type="button" className="ghost" onClick={onClose} style={{ color: "#fff", borderColor: "rgba(255,255,255,0.25)" }}>
            Fermer le workspace
          </button>
        </div>
      </aside>
      <main className="main">{children}</main>
    </div>
  );
}

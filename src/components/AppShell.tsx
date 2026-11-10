import { useEffect, useMemo, useState } from "react";
import { DeadlineBadge } from "./DeadlineReminders";
import { UpdateChecker } from "./UpdateChecker";
import { PACKAGE_VERSION, resolveAppVersion } from "../lib/appVersion";
import type { NavId, WorkspaceInfo } from "../lib/types";

type NavItem = { id: NavId; label: string; soon?: boolean };

type NavGroup = { id: string; label: string; items: NavItem[] };

const NAV_GROUPS: NavGroup[] = [
  {
    id: "societe",
    label: "Société",
    items: [
      { id: "guide", label: "Parcours guidé" },
      { id: "ma-sas", label: "Ma SAS" },
    ],
  },
  {
    id: "compta",
    label: "Comptabilité",
    items: [
      { id: "plan", label: "Plan comptable" },
      { id: "journaux", label: "Journaux" },
      { id: "ecritures", label: "Écritures" },
      { id: "import-docs", label: "Import docs" },
      { id: "grand-livre", label: "Grand livre" },
      { id: "tva", label: "TVA & tiers" },
      { id: "immos", label: "Immobilisations" },
      { id: "etats", label: "États annuels" },
      { id: "cloture", label: "Clôture & FEC" },
    ],
  },
  {
    id: "commercial",
    label: "Commercial",
    items: [
      { id: "devis", label: "Devis" },
      { id: "factures", label: "Factures" },
      { id: "relances", label: "Relances" },
      { id: "einvoice", label: "E-facture" },
    ],
  },
  {
    id: "rh",
    label: "RH",
    items: [
      { id: "salaries", label: "Salariés" },
      { id: "conges", label: "Congés" },
      { id: "paie", label: "Paie" },
    ],
  },
];

const OPEN_GROUPS_KEY = "sascompta.navOpenGroups";

function groupIdForNav(nav: NavId): string | null {
  for (const g of NAV_GROUPS) {
    if (g.items.some((i) => i.id === nav)) return g.id;
  }
  return null;
}

function defaultOpenGroups(nav: NavId): Record<string, boolean> {
  const active = groupIdForNav(nav);
  const open: Record<string, boolean> = {};
  for (const g of NAV_GROUPS) {
    open[g.id] = active ? g.id === active : g.id === "societe" || g.id === "compta";
  }
  return open;
}

function loadOpenGroups(nav: NavId): Record<string, boolean> {
  const fallback = defaultOpenGroups(nav);
  if (typeof sessionStorage === "undefined") return fallback;
  try {
    const raw = sessionStorage.getItem(OPEN_GROUPS_KEY);
    if (!raw) return fallback;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    const open: Record<string, boolean> = {};
    for (const g of NAV_GROUPS) {
      const value = parsed[g.id];
      open[g.id] = typeof value === "boolean" ? value : fallback[g.id];
    }
    return open;
  } catch {
    return fallback;
  }
}

function persistOpenGroups(open: Record<string, boolean>) {
  if (typeof sessionStorage === "undefined") return;
  try {
    sessionStorage.setItem(OPEN_GROUPS_KEY, JSON.stringify(open));
  } catch {
    /* ignore quota / private mode */
  }
}

interface Props {
  workspace: WorkspaceInfo;
  nav: NavId;
  onNav: (id: NavId) => void;
  onClose: () => void;
  children: React.ReactNode;
}

export function AppShell({ workspace, nav, onNav, onClose, children }: Props) {
  const [version, setVersion] = useState(PACKAGE_VERSION);
  const [openGroups, setOpenGroups] = useState<Record<string, boolean>>(() =>
    loadOpenGroups(nav),
  );

  useEffect(() => {
    let cancelled = false;
    void resolveAppVersion().then((v) => {
      if (!cancelled) setVersion(v);
    });
    return () => {
      cancelled = true;
    };
  }, []);

  // Keep the active screen's group expanded (multi-open elsewhere stays as-is).
  useEffect(() => {
    const gid = groupIdForNav(nav);
    if (!gid) return;
    setOpenGroups((prev) => {
      if (prev[gid]) return prev;
      const next = { ...prev, [gid]: true };
      persistOpenGroups(next);
      return next;
    });
  }, [nav]);

  const activeGroupId = useMemo(() => groupIdForNav(nav), [nav]);

  function toggleGroup(id: string) {
    setOpenGroups((prev) => {
      const next = { ...prev, [id]: !prev[id] };
      persistOpenGroups(next);
      return next;
    });
  }

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          SasCompta
          <small>{workspace.organization.name}</small>
        </div>
        <nav className="nav" aria-label="Navigation principale">
          {NAV_GROUPS.map((group) => {
            const expanded = !!openGroups[group.id];
            const panelId = `nav-panel-${group.id}`;
            const btnId = `nav-toggle-${group.id}`;
            return (
              <div
                key={group.id}
                className={`nav-group${expanded ? " is-open" : ""}${
                  activeGroupId === group.id ? " has-active" : ""
                }`}
              >
                <button
                  type="button"
                  id={btnId}
                  className="nav-group-toggle"
                  aria-expanded={expanded}
                  aria-controls={panelId}
                  onClick={() => toggleGroup(group.id)}
                >
                  <span className="nav-group-label">{group.label}</span>
                  <span className="nav-group-chevron" aria-hidden>
                    {expanded ? "▾" : "▸"}
                  </span>
                </button>
                <div
                  id={panelId}
                  role="region"
                  aria-labelledby={btnId}
                  className="nav-group-items"
                  hidden={!expanded}
                >
                  {group.items.map((item) => (
                    <button
                      key={item.id}
                      type="button"
                      className={nav === item.id ? "active" : ""}
                      onClick={() => onNav(item.id)}
                    >
                      <span className="nav-active-indicator" aria-hidden />
                      <span className="nav-label">
                        {item.label}
                        {item.id === "guide" && (
                          <DeadlineBadge workspace={workspace} />
                        )}
                        {item.soon && (
                          <span className="nav-soon" title="Bientôt disponible">
                            bientôt
                          </span>
                        )}
                      </span>
                    </button>
                  ))}
                </div>
              </div>
            );
          })}
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

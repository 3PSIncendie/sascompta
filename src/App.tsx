import { useState } from "react";
import { AppShell } from "./components/AppShell";
import { DeadlineReminders } from "./components/DeadlineReminders";
import { UpdateChecker } from "./components/UpdateChecker";
import { WelcomeScreen } from "./screens/WelcomeScreen";
import { GuideScreen } from "./screens/GuideScreen";
import { MaSasScreen } from "./screens/MaSasScreen";
import { ChartScreen } from "./screens/ChartScreen";
import { JournalsScreen } from "./screens/JournalsScreen";
import { EntriesScreen } from "./screens/EntriesScreen";
import { LedgerScreen } from "./screens/LedgerScreen";
import { TaxScreen } from "./screens/TaxScreen";
import { AssetsScreen } from "./screens/AssetsScreen";
import { StatementsScreen } from "./screens/StatementsScreen";
import { ClosingScreen } from "./screens/ClosingScreen";
import { api } from "./lib/api";
import { loadGuideProgress } from "./lib/guideProgress";
import type { NavId, WorkspaceInfo } from "./lib/types";

/** Aperçu UI navigateur (`?preview=guide`) — sans backend Tauri. */
function previewWorkspace(): WorkspaceInfo | null {
  if (typeof window === "undefined") return null;
  if (!new URLSearchParams(window.location.search).has("preview")) return null;
  return {
    path: "/preview/demo.sascompta",
    organization: {
      id: 1,
      name: "Atelier Lumière SAS",
      siren: "123456782",
      legalForm: "SAS",
      vatRegime: "reel_mensuel",
      address: null,
      capitalCents: 1_000_00,
    },
    fiscalYear: {
      id: 1,
      label: "2026",
      startDate: "2026-01-01",
      endDate: "2026-12-31",
      status: "open",
    },
    profile: { id: 1, displayName: "Dirigeant" },
  };
}

export default function App() {
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(
    () => previewWorkspace(),
  );
  const [nav, setNav] = useState<NavId>("guide");

  async function handleClose() {
    if (previewWorkspace()) {
      setWorkspace(null);
      setNav("guide");
      return;
    }
    try {
      await api.closeWorkspace();
    } catch {
      /* ignore */
    }
    setWorkspace(null);
    setNav("guide");
  }

  function handleOpened(info: WorkspaceInfo) {
    setWorkspace(info);
    const progress = loadGuideProgress(info.path);
    setNav(progress.completed.length >= 5 ? "ma-sas" : "guide");
  }

  if (!workspace) {
    return (
      <div className="app-shell welcome">
        <UpdateChecker checkOnStartup variant="banner" />
        <WelcomeScreen onOpened={handleOpened} />
      </div>
    );
  }

  return (
    <AppShell
      workspace={workspace}
      nav={nav}
      onNav={setNav}
      onClose={() => void handleClose()}
    >
      <UpdateChecker checkOnStartup variant="banner" />
      {nav !== "guide" && <DeadlineReminders workspace={workspace} variant="banner" />}
      {nav === "guide" && (
        <GuideScreen workspace={workspace} onNavigate={setNav} />
      )}
      {nav === "ma-sas" && <MaSasScreen workspace={workspace} />}
      {nav === "plan" && <ChartScreen />}
      {nav === "journaux" && <JournalsScreen />}
      {nav === "ecritures" && <EntriesScreen />}
      {nav === "grand-livre" && <LedgerScreen />}
      {nav === "tva" && <TaxScreen />}
      {nav === "immos" && <AssetsScreen />}
      {nav === "etats" && <StatementsScreen />}
      {nav === "cloture" && workspace && (
        <ClosingScreen
          workspace={workspace}
          onWorkspaceChange={setWorkspace}
        />
      )}
    </AppShell>
  );
}

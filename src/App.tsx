import { useState } from "react";
import { AppShell } from "./components/AppShell";
import { WelcomeScreen } from "./screens/WelcomeScreen";
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
import type { NavId, WorkspaceInfo } from "./lib/types";

export default function App() {
  const [workspace, setWorkspace] = useState<WorkspaceInfo | null>(null);
  const [nav, setNav] = useState<NavId>("ma-sas");

  async function handleClose() {
    try {
      await api.closeWorkspace();
    } catch {
      /* ignore */
    }
    setWorkspace(null);
    setNav("ma-sas");
  }

  if (!workspace) {
    return (
      <div className="app-shell welcome">
        <WelcomeScreen onOpened={setWorkspace} />
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

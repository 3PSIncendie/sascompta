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
import { ComingSoonScreen } from "./screens/ComingSoonScreen";
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
      siret: "12345678200002",
      legalForm: "SAS",
      vatRegime: "reel_mensuel",
      address: "12 rue des Lilas\n75011 Paris",
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
      {nav === "ma-sas" && (
        <MaSasScreen
          workspace={workspace}
          onWorkspaceChange={setWorkspace}
        />
      )}
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
      {nav === "devis" && (
        <ComingSoonScreen
          group="Commercial"
          title="Devis"
          blurb="Chiffrez vos propositions clients, puis transformez-les en facture sans re-saisie."
        />
      )}
      {nav === "factures" && (
        <ComingSoonScreen
          group="Commercial"
          title="Factures"
          blurb="Factures, acomptes et avoirs avec écritures VTE automatiques en brouillon."
        />
      )}
      {nav === "relances" && (
        <ComingSoonScreen
          group="Commercial"
          title="Relances"
          blurb="File de relances basée sur l’échéance et le statut de paiement."
        />
      )}
      {nav === "einvoice" && (
        <ComingSoonScreen
          group="Commercial"
          title="E-facture"
          blurb="Factur-X local, file d’attente PDP et imports structurés — sans prétendre être une PDP."
        />
      )}
      {nav === "salaries" && (
        <ComingSoonScreen
          group="RH"
          title="Salariés"
          blurb="Dossiers salariés et contrats (CDI, CDD, apprentissage) pour une petite SAS."
        />
      )}
      {nav === "conges" && (
        <ComingSoonScreen
          group="RH"
          title="Congés"
          blurb="Pose, solde et validation simple des congés."
        />
      )}
      {nav === "paie" && (
        <ComingSoonScreen
          group="RH"
          title="Paie"
          blurb="Bulletins simplifiés, charges et écritures OD en brouillon, export DSN-oriented."
        />
      )}
    </AppShell>
  );
}

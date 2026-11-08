import { useMemo, useState } from "react";
import { save, open } from "@tauri-apps/plugin-dialog";
import { FieldHelp, LabelWithHelp } from "../components/FieldHelp";
import { api } from "../lib/api";
import type { WorkspaceInfo } from "../lib/types";

interface Props {
  onOpened: (info: WorkspaceInfo) => void;
}

function validateCreate(input: {
  orgName: string;
  siren: string;
  year: string;
  displayName: string;
  password: string;
  password2: string;
}): string | null {
  if (input.orgName.trim().length < 2) {
    return "Indiquez la raison sociale (au moins 2 caractères).";
  }
  const siren = input.siren.replace(/\s/g, "");
  if (!/^\d{9}$/.test(siren)) {
    return "Le SIREN doit contenir exactement 9 chiffres.";
  }
  if (!/^\d{4}$/.test(input.year)) {
    return "L’exercice doit être une année à 4 chiffres (ex. 2026).";
  }
  if (input.displayName.trim().length < 2) {
    return "Indiquez un profil local (ex. Dirigeant).";
  }
  if (input.password.length < 8) {
    return "Choisissez un mot de passe d’au moins 8 caractères.";
  }
  if (input.password !== input.password2) {
    return "Les mots de passe ne correspondent pas.";
  }
  return null;
}

export function WelcomeScreen({ onOpened }: Props) {
  const [mode, setMode] = useState<"idle" | "create" | "open">("idle");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [orgName, setOrgName] = useState("");
  const [siren, setSiren] = useState("");
  const [year, setYear] = useState(String(new Date().getFullYear()));
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");
  const [openPath, setOpenPath] = useState("");
  const [openPassword, setOpenPassword] = useState("");

  const liveHint = useMemo(
    () =>
      validateCreate({
        orgName,
        siren,
        year,
        displayName,
        password,
        password2,
      }),
    [orgName, siren, year, displayName, password, password2],
  );

  async function handleCreate() {
    setError(null);
    const v = validateCreate({
      orgName,
      siren,
      year,
      displayName,
      password,
      password2,
    });
    if (v) {
      setError(v);
      return;
    }
    setBusy(true);
    try {
      const path = await save({
        title: "Enregistrer le workspace",
        defaultPath: `${orgName.replace(/\s+/g, "-").toLowerCase()}.sascompta`,
        filters: [{ name: "SasCompta", extensions: ["sascompta"] }],
      });
      if (!path) {
        setBusy(false);
        return;
      }
      const info = await api.createWorkspace({
        path: String(path),
        password,
        orgName: orgName.trim(),
        siren: siren.replace(/\s/g, ""),
        fiscalYearLabel: year,
        startDate: `${year}-01-01`,
        endDate: `${year}-12-31`,
        displayName: displayName.trim(),
      });
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleOpen() {
    setError(null);
    setBusy(true);
    try {
      let path = openPath;
      if (!path) {
        const picked = await open({
          title: "Ouvrir un workspace",
          multiple: false,
          filters: [{ name: "SasCompta", extensions: ["sascompta"] }],
        });
        if (!picked) {
          setBusy(false);
          return;
        }
        path = String(picked);
        setOpenPath(path);
      }
      const info = await api.openWorkspace(path, openPassword);
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleDemo() {
    setError(null);
    setBusy(true);
    try {
      const path = "/tmp/sascompta-demo/demo.sascompta";
      const info = await api.createDemoWorkspace(path);
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="welcome-stage">
      <div className="welcome-atmosphere" aria-hidden />
      <div className="welcome-viewport">
        <header className="welcome-brand-block">
          <h1 className="welcome-brand">SasCompta</h1>
          <p className="welcome-headline">
            La compta SAS, claire et locale.
          </p>
          <p className="welcome-support">
            Un fichier sur votre machine, un parcours guidé, zéro cloud
            imposé.
          </p>
          <div className="welcome-cta">
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => {
                setError(null);
                setMode("create");
              }}
            >
              Créer une société
            </button>
            <button
              type="button"
              className="secondary"
              disabled={busy}
              onClick={() => {
                setError(null);
                setMode("open");
              }}
            >
              Ouvrir
            </button>
            <button
              type="button"
              className="ghost welcome-demo"
              disabled={busy}
              onClick={() => void handleDemo()}
            >
              Workspace démo
            </button>
          </div>
        </header>
      </div>

      {mode !== "idle" && (
        <section
          className="welcome-form-panel"
          aria-label={mode === "create" ? "Créer une société" : "Ouvrir"}
        >
          <div className="tabs">
            <button
              type="button"
              className={mode === "create" ? "active" : ""}
              onClick={() => setMode("create")}
            >
              Créer une société
            </button>
            <button
              type="button"
              className={mode === "open" ? "active" : ""}
              onClick={() => setMode("open")}
            >
              Ouvrir
            </button>
            <button
              type="button"
              className="ghost"
              onClick={() => {
                setMode("idle");
                setError(null);
              }}
            >
              Fermer
            </button>
          </div>

          {mode === "create" ? (
            <div className="form-grid">
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="raisonSociale">Raison sociale</LabelWithHelp>
                  <input
                    value={orgName}
                    placeholder="Ex. Atelier Lumière SAS"
                    onChange={(e) => setOrgName(e.target.value)}
                  />
                </label>
                <label>
                  <LabelWithHelp topic="siren">SIREN</LabelWithHelp>
                  <input
                    value={siren}
                    placeholder="9 chiffres"
                    inputMode="numeric"
                    onChange={(e) => setSiren(e.target.value)}
                  />
                </label>
              </div>
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="exercice">Exercice</LabelWithHelp>
                  <input
                    value={year}
                    placeholder="2026"
                    onChange={(e) => setYear(e.target.value)}
                  />
                </label>
                <label>
                  <LabelWithHelp topic="profilLocal">Profil local</LabelWithHelp>
                  <input
                    value={displayName}
                    placeholder="Dirigeant"
                    onChange={(e) => setDisplayName(e.target.value)}
                  />
                </label>
              </div>
              <div className="form-grid two">
                <label>
                  <LabelWithHelp topic="motDePasse">Mot de passe</LabelWithHelp>
                  <input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </label>
                <label>
                  <span className="label-with-help">
                    Confirmation
                    <FieldHelp
                      topic={{
                        term: "Confirmation",
                        plain:
                          "Retapez le même mot de passe pour éviter une faute de frappe.",
                      }}
                      compact
                    />
                  </span>
                  <input
                    type="password"
                    value={password2}
                    onChange={(e) => setPassword2(e.target.value)}
                  />
                </label>
              </div>
              {liveHint && !error && (
                <p className="hint welcome-live-hint">{liveHint}</p>
              )}
              <div className="btn-row">
                <button
                  type="button"
                  className="primary"
                  disabled={busy || Boolean(liveHint)}
                  onClick={() => void handleCreate()}
                >
                  Créer le workspace
                </button>
              </div>
            </div>
          ) : (
            <div className="form-grid">
              <label>
                Fichier .sascompta
                <div className="btn-row" style={{ marginTop: 0 }}>
                  <input
                    style={{ flex: 1 }}
                    value={openPath}
                    onChange={(e) => setOpenPath(e.target.value)}
                    placeholder="Chemin du workspace"
                  />
                  <button
                    type="button"
                    className="ghost"
                    onClick={() => void handleOpen()}
                  >
                    Parcourir…
                  </button>
                </div>
              </label>
              <label>
                <LabelWithHelp topic="motDePasse">Mot de passe</LabelWithHelp>
                <input
                  type="password"
                  value={openPassword}
                  onChange={(e) => setOpenPassword(e.target.value)}
                />
              </label>
              <div className="btn-row">
                <button
                  type="button"
                  className="primary"
                  disabled={busy || !openPassword}
                  onClick={() => void handleOpen()}
                >
                  Ouvrir
                </button>
              </div>
            </div>
          )}

          {error && <div className="error">{error}</div>}
        </section>
      )}
    </div>
  );
}

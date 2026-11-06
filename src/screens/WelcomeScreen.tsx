import { useState } from "react";
import { save, open } from "@tauri-apps/plugin-dialog";
import { api } from "../lib/api";
import type { WorkspaceInfo } from "../lib/types";

interface Props {
  onOpened: (info: WorkspaceInfo) => void;
}

export function WelcomeScreen({ onOpened }: Props) {
  const [mode, setMode] = useState<"create" | "open">("create");
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [orgName, setOrgName] = useState("Demo SAS");
  const [siren, setSiren] = useState("123456782");
  const [year, setYear] = useState("2026");
  const [displayName, setDisplayName] = useState("Dirigeant");
  const [password, setPassword] = useState("SecretPass1");
  const [password2, setPassword2] = useState("SecretPass1");
  const [openPath, setOpenPath] = useState("");
  const [openPassword, setOpenPassword] = useState("");

  async function handleCreate() {
    setError(null);
    if (password !== password2) {
      setError("Les mots de passe ne correspondent pas.");
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
        orgName,
        siren,
        fiscalYearLabel: year,
        startDate: `${year}-01-01`,
        endDate: `${year}-12-31`,
        displayName,
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
      // Chemin fixe local (pas de dialogue) — comme les jalons J0–J3
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
    <div className="welcome-wrap">
      <div className="welcome-card">
        <section className="welcome-hero">
          <h1>SasCompta</h1>
          <p>
            Comptabilité générale pour une SAS française — workspace local protégé
            par mot de passe.
          </p>
        </section>
        <section className="welcome-forms">
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
          </div>

          {mode === "create" ? (
            <div className="form-grid">
              <div className="form-grid two">
                <label>
                  Raison sociale
                  <input value={orgName} onChange={(e) => setOrgName(e.target.value)} />
                </label>
                <label>
                  SIREN
                  <input value={siren} onChange={(e) => setSiren(e.target.value)} />
                </label>
              </div>
              <div className="form-grid two">
                <label>
                  Exercice
                  <input value={year} onChange={(e) => setYear(e.target.value)} />
                </label>
                <label>
                  Profil local
                  <input
                    value={displayName}
                    onChange={(e) => setDisplayName(e.target.value)}
                  />
                </label>
              </div>
              <div className="form-grid two">
                <label>
                  Mot de passe
                  <input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                  />
                </label>
                <label>
                  Confirmation
                  <input
                    type="password"
                    value={password2}
                    onChange={(e) => setPassword2(e.target.value)}
                  />
                </label>
              </div>
              <div className="btn-row">
                <button
                  type="button"
                  className="primary"
                  disabled={busy}
                  onClick={() => void handleCreate()}
                >
                  Créer le workspace
                </button>
                <button
                  type="button"
                  className="secondary"
                  disabled={busy}
                  onClick={() => void handleDemo()}
                >
                  Workspace démo
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
                Mot de passe
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
      </div>
    </div>
  );
}

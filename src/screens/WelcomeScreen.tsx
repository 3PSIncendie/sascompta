import { useEffect, useMemo, useState } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { FieldHelp, LabelWithHelp } from "../components/FieldHelp";
import { api } from "../lib/api";
import type { KnownWorkspace, WorkspaceInfo } from "../lib/types";

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
  const [known, setKnown] = useState<KnownWorkspace[]>([]);
  const [knownLoaded, setKnownLoaded] = useState(false);

  const [orgName, setOrgName] = useState("");
  const [siren, setSiren] = useState("");
  const [year, setYear] = useState(String(new Date().getFullYear()));
  const [displayName, setDisplayName] = useState("");
  const [password, setPassword] = useState("");
  const [password2, setPassword2] = useState("");

  const [selectedPath, setSelectedPath] = useState("");
  const [openPassword, setOpenPassword] = useState("");
  const [importHint, setImportHint] = useState<string | null>(null);

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

  async function refreshKnown(preferPath?: string) {
    try {
      const list = await api.listKnownWorkspaces();
      setKnown(list);
      if (preferPath && list.some((w) => w.path === preferPath)) {
        setSelectedPath(preferPath);
      } else if (list.length === 0) {
        if (!preferPath) setSelectedPath("");
      } else if (!list.some((w) => w.path === selectedPath)) {
        setSelectedPath(list[0].path);
      }
      return list;
    } catch {
      // Aperçu navigateur (`?previewLogin`) — sans backend Tauri.
      const previewLogin =
        typeof window !== "undefined" &&
        new URLSearchParams(window.location.search).has("previewLogin");
      if (previewLogin) {
        const mock: KnownWorkspace[] = [
          {
            id: "preview-1",
            name: "Atelier Lumière SAS",
            siren: "123456782",
            path: "/preview/atelier-lumiere.sascompta",
            lastOpenedAt: new Date().toISOString(),
          },
          {
            id: "preview-2",
            name: "Nord Énergie SAS",
            siren: "987654321",
            path: "/preview/nord-energie.sascompta",
            lastOpenedAt: new Date().toISOString(),
          },
        ];
        setKnown(mock);
        setSelectedPath(mock[0].path);
        return mock;
      }
      setKnown([]);
      return [] as KnownWorkspace[];
    } finally {
      setKnownLoaded(true);
    }
  }

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      const list = await refreshKnown();
      if (cancelled) return;
      if (list.length > 0) {
        setMode("open");
      }
    })();
    return () => {
      cancelled = true;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps -- bootstrap once
  }, []);

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
      const info = await api.createWorkspace({
        password,
        orgName: orgName.trim(),
        siren: siren.replace(/\s/g, ""),
        fiscalYearLabel: year,
        startDate: `${year}-01-01`,
        endDate: `${year}-12-31`,
        displayName: displayName.trim(),
      });
      await refreshKnown(info.path);
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleOpenSelected() {
    setError(null);
    setImportHint(null);
    if (!selectedPath) {
      setError("Choisissez une société ou importez un fichier.");
      return;
    }
    if (!openPassword) {
      setError("Indiquez le mot de passe.");
      return;
    }
    setBusy(true);
    try {
      const info = await api.openWorkspace(selectedPath, openPassword);
      await refreshKnown(info.path);
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  async function handleImportFile() {
    setError(null);
    setImportHint(null);
    setBusy(true);
    try {
      const picked = await open({
        title: "Importer un fichier .sascompta",
        multiple: false,
        filters: [{ name: "SasCompta", extensions: ["sascompta"] }],
      });
      if (!picked) {
        setBusy(false);
        return;
      }
      const path = String(picked);
      setSelectedPath(path);
      setImportHint(
        "Fichier sélectionné — entrez le mot de passe pour l’ouvrir (il sera mémorisé).",
      );
      setMode("open");
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
      const info = await api.createDemoWorkspace();
      await refreshKnown(info.path);
      onOpened(info);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }

  const selectedIsImported =
    Boolean(selectedPath) && !known.some((w) => w.path === selectedPath);

  return (
    <div className={`welcome-stage${mode !== "idle" ? " has-form" : ""}`}>
      <div className="welcome-atmosphere" aria-hidden />
      <div className="welcome-viewport">
        <header className="welcome-brand-block">
          <h1 className="welcome-brand">SasCompta</h1>
          <p className="welcome-headline">
            La compta SAS, claire et locale.
          </p>
          <p className="welcome-support">
            Choisissez votre société, entrez le mot de passe — vos fichiers
            restent sur cette machine.
          </p>
          <div className="welcome-cta">
            <button
              type="button"
              className="primary"
              disabled={busy}
              onClick={() => {
                setError(null);
                setImportHint(null);
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
                setImportHint(null);
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
                setImportHint(null);
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
              <p className="hint welcome-live-hint">
                Le fichier sera enregistré automatiquement sur cet ordinateur.
              </p>
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
                  Créer la société
                </button>
              </div>
            </div>
          ) : (
            <div className="form-grid">
              {knownLoaded && known.length === 0 && !selectedIsImported ? (
                <p className="hint welcome-live-hint">
                  Aucune société mémorisée pour l’instant. Créez-en une, lancez
                  la démo, ou importez un fichier existant.
                </p>
              ) : (
                <label>
                  Société
                  <select
                    value={selectedPath}
                    onChange={(e) => {
                      setSelectedPath(e.target.value);
                      setImportHint(null);
                    }}
                  >
                    {selectedIsImported && (
                      <option value={selectedPath}>
                        Fichier importé…
                      </option>
                    )}
                    {known.map((w) => (
                      <option key={w.id} value={w.path}>
                        {w.name}
                        {w.siren ? ` · ${w.siren}` : ""}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <label>
                <LabelWithHelp topic="motDePasse">Mot de passe</LabelWithHelp>
                <input
                  type="password"
                  value={openPassword}
                  onChange={(e) => setOpenPassword(e.target.value)}
                  autoComplete="current-password"
                />
              </label>
              {importHint && (
                <p className="hint welcome-live-hint">{importHint}</p>
              )}
              <div className="btn-row welcome-open-actions">
                <button
                  type="button"
                  className="primary"
                  disabled={busy || !selectedPath || !openPassword}
                  onClick={() => void handleOpenSelected()}
                >
                  Ouvrir
                </button>
                <button
                  type="button"
                  className="ghost"
                  disabled={busy}
                  onClick={() => void handleImportFile()}
                >
                  Importer un fichier…
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

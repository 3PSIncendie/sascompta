import { formatEuros } from "../lib/money";
import type { WorkspaceInfo } from "../lib/types";

export function MaSasScreen({ workspace }: { workspace: WorkspaceInfo }) {
  const { organization: org, fiscalYear: fy, profile } = workspace;
  return (
    <div className="panel">
      <h1>Ma SAS</h1>
      <p>Paramètres de la société et de l’exercice courant.</p>
      <div className="meta-grid">
        <div className="meta-item">
          <span>Raison sociale</span>
          <strong>{org.name}</strong>
        </div>
        <div className="meta-item">
          <span>SIREN</span>
          <strong>{org.siren}</strong>
        </div>
        <div className="meta-item">
          <span>Forme</span>
          <strong>{org.legalForm}</strong>
        </div>
        <div className="meta-item">
          <span>Régime TVA</span>
          <strong>{org.vatRegime}</strong>
        </div>
        <div className="meta-item">
          <span>Capital</span>
          <strong>{formatEuros(org.capitalCents)}</strong>
        </div>
        <div className="meta-item">
          <span>Exercice</span>
          <strong>
            {fy.label} ({fy.startDate} → {fy.endDate})
          </strong>
        </div>
        <div className="meta-item">
          <span>Statut exercice</span>
          <strong>{fy.status}</strong>
        </div>
        <div className="meta-item">
          <span>Profil local</span>
          <strong>{profile.displayName}</strong>
        </div>
        <div className="meta-item">
          <span>Fichier</span>
          <strong style={{ fontSize: "0.85rem", wordBreak: "break-all" }}>
            {workspace.path}
          </strong>
        </div>
      </div>
    </div>
  );
}

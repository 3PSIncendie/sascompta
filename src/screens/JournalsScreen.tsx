import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Journal } from "../lib/types";

export function JournalsScreen() {
  const [journals, setJournals] = useState<Journal[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api
      .listJournals()
      .then(setJournals)
      .catch((e) => setError(String(e)));
  }, []);

  return (
    <div className="panel">
      <h1>Journaux</h1>
      <p>Journaux standards : Achats, Ventes, Banque, OD, À-nouveaux.</p>
      {error && <div className="error">{error}</div>}
      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>Code</th>
              <th>Libellé</th>
              <th>Type</th>
            </tr>
          </thead>
          <tbody>
            {journals.map((j) => (
              <tr key={j.id}>
                <td>
                  <strong>{j.code}</strong>
                </td>
                <td>{j.label}</td>
                <td>{j.journalType}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

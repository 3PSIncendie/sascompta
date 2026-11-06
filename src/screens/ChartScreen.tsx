import { useEffect, useState } from "react";
import { api } from "../lib/api";
import type { Account } from "../lib/types";

export function ChartScreen() {
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [number, setNumber] = useState("");
  const [label, setLabel] = useState("");
  const [accountType, setAccountType] = useState("expense");
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState("");

  async function reload() {
    setAccounts(await api.listAccounts());
  }

  useEffect(() => {
    void reload().catch((e) => setError(String(e)));
  }, []);

  async function addAccount() {
    setError(null);
    try {
      await api.createAccount(number, label, accountType);
      setNumber("");
      setLabel("");
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  async function remove(id: number) {
    setError(null);
    try {
      await api.deleteAccount(id);
      await reload();
    } catch (e) {
      setError(String(e));
    }
  }

  const visible = accounts.filter(
    (a) =>
      !filter ||
      a.number.includes(filter) ||
      a.label.toLowerCase().includes(filter.toLowerCase()),
  );

  return (
    <div className="panel">
      <h1>Plan comptable</h1>
      <p>PCG SAS classes 1–7 — comptes standards et personnalisés.</p>

      <div className="toolbar">
        <label>
          Filtrer
          <input value={filter} onChange={(e) => setFilter(e.target.value)} />
        </label>
        <label>
          N°
          <input value={number} onChange={(e) => setNumber(e.target.value)} />
        </label>
        <label>
          Libellé
          <input value={label} onChange={(e) => setLabel(e.target.value)} />
        </label>
        <label>
          Type
          <select
            value={accountType}
            onChange={(e) => setAccountType(e.target.value)}
          >
            <option value="asset">Actif</option>
            <option value="liability">Passif</option>
            <option value="equity">Capitaux</option>
            <option value="expense">Charge</option>
            <option value="revenue">Produit</option>
          </select>
        </label>
        <button type="button" className="primary" onClick={() => void addAccount()}>
          Ajouter
        </button>
      </div>

      {error && <div className="error">{error}</div>}

      <div className="table-wrap">
        <table>
          <thead>
            <tr>
              <th>N°</th>
              <th>Libellé</th>
              <th>Classe</th>
              <th>Type</th>
              <th>Aux.</th>
              <th></th>
            </tr>
          </thead>
          <tbody>
            {visible.map((a) => (
              <tr key={a.id}>
                <td>{a.number}</td>
                <td>{a.label}</td>
                <td>{a.class}</td>
                <td>{a.accountType}</td>
                <td>{a.isAuxiliary ? "oui" : ""}</td>
                <td>
                  {a.isCustom && (
                    <button
                      type="button"
                      className="danger"
                      onClick={() => void remove(a.id)}
                    >
                      Suppr.
                    </button>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </div>
  );
}

import { useEffect, useMemo, useState } from "react";
import { api } from "../lib/api";
import { formatEuros, parseEurosToCents } from "../lib/money";
import { isTauriRuntime } from "../lib/updater";
import {
  CONTRACT_KIND_LABELS,
  CONTRACT_STATUS_LABELS,
  formatHoursBp,
  parseHoursToBp,
} from "../modules/hr/format";
import { previewContracts, previewEmployees } from "../modules/hr/preview";
import type {
  CreateContractInput,
  CreateEmployeeInput,
  Employee,
  EmploymentContract,
} from "../lib/types";

const PREVIEW =
  typeof window !== "undefined" &&
  new URLSearchParams(window.location.search).has("preview");

type Tab = "salaries" | "contrats";

const emptyEmployee = (): CreateEmployeeInput => ({
  code: "",
  firstName: "",
  lastName: "",
  birthDate: "",
  address: "",
  email: "",
  ibanRef: "",
  active: true,
});

export function EmployeesScreen() {
  const [tab, setTab] = useState<Tab>("salaries");
  const [employees, setEmployees] = useState<Employee[]>([]);
  const [contracts, setContracts] = useState<EmploymentContract[]>([]);
  const [selectedId, setSelectedId] = useState<number | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [form, setForm] = useState(emptyEmployee());
  const [editingId, setEditingId] = useState<number | null>(null);

  const [contractForm, setContractForm] = useState({
    employeeId: 0,
    kind: "cdi",
    startDate: "2026-01-01",
    endDate: "",
    jobTitle: "",
    weeklyHours: "35",
    grossEuros: "2800.00",
    conventionRef: "",
    status: "active",
  });

  const selected = useMemo(
    () => employees.find((e) => e.id === selectedId) ?? null,
    [employees, selectedId],
  );

  const contractsForSelected = useMemo(
    () =>
      selectedId == null
        ? contracts
        : contracts.filter((c) => c.employeeId === selectedId),
    [contracts, selectedId],
  );

  async function refresh() {
    setError(null);
    if (PREVIEW || !isTauriRuntime()) {
      const emps = previewEmployees();
      setEmployees(emps);
      setContracts(previewContracts());
      if (selectedId == null && emps[0]) setSelectedId(emps[0].id);
      return;
    }
    try {
      const [emps, cts] = await Promise.all([
        api.listEmployees(),
        api.listContracts(null),
      ]);
      setEmployees(emps);
      setContracts(cts);
      if (selectedId == null && emps[0]) setSelectedId(emps[0].id);
    } catch (e) {
      setError(String(e));
    }
  }

  useEffect(() => {
    void refresh();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function loadEmployee(emp: Employee) {
    setEditingId(emp.id);
    setSelectedId(emp.id);
    setForm({
      code: emp.code,
      firstName: emp.firstName,
      lastName: emp.lastName,
      birthDate: emp.birthDate ?? "",
      address: emp.address ?? "",
      email: emp.email ?? "",
      ibanRef: emp.ibanRef ?? "",
      active: emp.active,
    });
    setContractForm((f) => ({ ...f, employeeId: emp.id }));
  }

  function resetForm() {
    setEditingId(null);
    setForm(emptyEmployee());
  }

  async function onSaveEmployee(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : enregistrement simulé.");
        return;
      }
      const input: CreateEmployeeInput = {
        ...form,
        birthDate: form.birthDate || null,
        address: form.address || null,
        email: form.email || null,
        ibanRef: form.ibanRef || null,
      };
      const saved = editingId
        ? await api.updateEmployee(editingId, input)
        : await api.createEmployee(input);
      setStatus(
        editingId
          ? `Salarié ${saved.code} mis à jour`
          : `Salarié ${saved.code} créé`,
      );
      setSelectedId(saved.id);
      resetForm();
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onDeactivate(emp: Employee) {
    if (PREVIEW || !isTauriRuntime()) return;
    setBusy(true);
    try {
      await api.updateEmployee(emp.id, {
        code: emp.code,
        firstName: emp.firstName,
        lastName: emp.lastName,
        birthDate: emp.birthDate,
        address: emp.address,
        email: emp.email,
        ibanRef: emp.ibanRef,
        active: !emp.active,
      });
      setStatus(
        emp.active
          ? `${emp.code} désactivé`
          : `${emp.code} réactivé`,
      );
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  async function onSaveContract(e: React.FormEvent) {
    e.preventDefault();
    setBusy(true);
    setError(null);
    setStatus(null);
    try {
      if (PREVIEW || !isTauriRuntime()) {
        setStatus("Aperçu : contrat simulé.");
        return;
      }
      const employeeId = contractForm.employeeId || selectedId;
      if (!employeeId) {
        setError("Choisissez un salarié");
        return;
      }
      const input: CreateContractInput = {
        employeeId,
        kind: contractForm.kind,
        startDate: contractForm.startDate,
        endDate: contractForm.endDate || null,
        jobTitle: contractForm.jobTitle.trim(),
        weeklyHoursBp: parseHoursToBp(contractForm.weeklyHours) || 3500,
        grossMonthlyCents: parseEurosToCents(contractForm.grossEuros),
        conventionRef: contractForm.conventionRef || null,
        status: contractForm.status,
      };
      const c = await api.createContract(input);
      setStatus(`Contrat ${CONTRACT_KIND_LABELS[c.kind] ?? c.kind} créé`);
      setContractForm({
        ...contractForm,
        jobTitle: "",
        endDate: "",
        conventionRef: "",
      });
      await refresh();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="stack">
      <header className="page-head">
        <div>
          <p className="eyebrow">RH</p>
          <h1>Salariés</h1>
          <p>
            Dossiers salariés et contrats (CDI, CDD, apprentissage) — mono-SAS,
            données locales.
          </p>
        </div>
      </header>

      {error && <p className="error">{error}</p>}
      {status && <p className="ok">{status}</p>}

      <div className="tabs" role="tablist">
        <button
          type="button"
          className={tab === "salaries" ? "active" : ""}
          onClick={() => setTab("salaries")}
        >
          Salariés
        </button>
        <button
          type="button"
          className={tab === "contrats" ? "active" : ""}
          onClick={() => setTab("contrats")}
        >
          Contrats
        </button>
      </div>

      {tab === "salaries" && (
        <div className="grid-2">
          <section className="panel">
            <div className="toolbar">
              <h2>Liste</h2>
              <button type="button" className="ghost" onClick={resetForm}>
                Nouveau
              </button>
            </div>
            <table>
              <thead>
                <tr>
                  <th>Code</th>
                  <th>Nom</th>
                  <th>Statut</th>
                </tr>
              </thead>
              <tbody>
                {employees.map((emp) => (
                  <tr
                    key={emp.id}
                    className={selectedId === emp.id ? "selected" : ""}
                    onClick={() => loadEmployee(emp)}
                  >
                    <td>{emp.code}</td>
                    <td>
                      {emp.firstName} {emp.lastName}
                    </td>
                    <td>{emp.active ? "Actif" : "Inactif"}</td>
                  </tr>
                ))}
                {employees.length === 0 && (
                  <tr>
                    <td colSpan={3}>Aucun salarié pour l’instant.</td>
                  </tr>
                )}
              </tbody>
            </table>
          </section>

          <section className="panel">
            <h2>{editingId ? "Modifier" : "Nouveau salarié"}</h2>
            <form className="stack" onSubmit={(e) => void onSaveEmployee(e)}>
              <div className="grid-2">
                <label className="field">
                  Code
                  <input
                    value={form.code}
                    onChange={(e) =>
                      setForm({ ...form, code: e.target.value })
                    }
                    required
                  />
                </label>
                <label className="field">
                  Naissance
                  <input
                    type="date"
                    value={form.birthDate ?? ""}
                    onChange={(e) =>
                      setForm({ ...form, birthDate: e.target.value })
                    }
                  />
                </label>
                <label className="field">
                  Prénom
                  <input
                    value={form.firstName}
                    onChange={(e) =>
                      setForm({ ...form, firstName: e.target.value })
                    }
                    required
                  />
                </label>
                <label className="field">
                  Nom
                  <input
                    value={form.lastName}
                    onChange={(e) =>
                      setForm({ ...form, lastName: e.target.value })
                    }
                    required
                  />
                </label>
              </div>
              <label className="field">
                E-mail
                <input
                  type="email"
                  value={form.email ?? ""}
                  onChange={(e) => setForm({ ...form, email: e.target.value })}
                />
              </label>
              <label className="field">
                Adresse
                <textarea
                  rows={2}
                  value={form.address ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, address: e.target.value })
                  }
                />
              </label>
              <label className="field">
                Réf. IBAN (masquée)
                <input
                  value={form.ibanRef ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, ibanRef: e.target.value })
                  }
                />
              </label>
              {editingId != null && (
                <label className="field checkbox">
                  <input
                    type="checkbox"
                    checked={!!form.active}
                    onChange={(e) =>
                      setForm({ ...form, active: e.target.checked })
                    }
                  />
                  Actif
                </label>
              )}
              <div className="toolbar">
                <button type="submit" disabled={busy}>
                  {editingId ? "Enregistrer" : "Créer"}
                </button>
                {selected && (
                  <button
                    type="button"
                    className="ghost"
                    disabled={busy}
                    onClick={() => void onDeactivate(selected)}
                  >
                    {selected.active ? "Désactiver" : "Réactiver"}
                  </button>
                )}
              </div>
            </form>
          </section>
        </div>
      )}

      {tab === "contrats" && (
        <div className="grid-2">
          <section className="panel">
            <h2>
              Contrats
              {selected
                ? ` — ${selected.firstName} ${selected.lastName}`
                : ""}
            </h2>
            <table>
              <thead>
                <tr>
                  <th>Salarié</th>
                  <th>Type</th>
                  <th>Poste</th>
                  <th>Brut</th>
                  <th>Statut</th>
                </tr>
              </thead>
              <tbody>
                {contractsForSelected.map((c) => (
                  <tr key={c.id}>
                    <td>
                      {c.employeeCode}
                      <br />
                      <small>{c.employeeName}</small>
                    </td>
                    <td>{CONTRACT_KIND_LABELS[c.kind] ?? c.kind}</td>
                    <td>
                      {c.jobTitle}
                      <br />
                      <small>
                        {c.startDate}
                        {c.endDate ? ` → ${c.endDate}` : ""} ·{" "}
                        {formatHoursBp(c.weeklyHoursBp)}
                      </small>
                    </td>
                    <td>{formatEuros(c.grossMonthlyCents)}</td>
                    <td>{CONTRACT_STATUS_LABELS[c.status] ?? c.status}</td>
                  </tr>
                ))}
                {contractsForSelected.length === 0 && (
                  <tr>
                    <td colSpan={5}>Aucun contrat.</td>
                  </tr>
                )}
              </tbody>
            </table>
          </section>

          <section className="panel">
            <h2>Nouveau contrat</h2>
            <form className="stack" onSubmit={(e) => void onSaveContract(e)}>
              <label className="field">
                Salarié
                <select
                  value={contractForm.employeeId || selectedId || 0}
                  onChange={(e) =>
                    setContractForm({
                      ...contractForm,
                      employeeId: Number(e.target.value),
                    })
                  }
                  required
                >
                  <option value={0}>— choisir —</option>
                  {employees.map((emp) => (
                    <option key={emp.id} value={emp.id}>
                      {emp.code} — {emp.firstName} {emp.lastName}
                    </option>
                  ))}
                </select>
              </label>
              <div className="grid-2">
                <label className="field">
                  Type
                  <select
                    value={contractForm.kind}
                    onChange={(e) =>
                      setContractForm({ ...contractForm, kind: e.target.value })
                    }
                  >
                    <option value="cdi">CDI</option>
                    <option value="cdd">CDD</option>
                    <option value="apprentice">Apprentissage</option>
                  </select>
                </label>
                <label className="field">
                  Statut
                  <select
                    value={contractForm.status}
                    onChange={(e) =>
                      setContractForm({
                        ...contractForm,
                        status: e.target.value,
                      })
                    }
                  >
                    <option value="active">Actif</option>
                    <option value="draft">Brouillon</option>
                    <option value="ended">Terminé</option>
                  </select>
                </label>
                <label className="field">
                  Début
                  <input
                    type="date"
                    value={contractForm.startDate}
                    onChange={(e) =>
                      setContractForm({
                        ...contractForm,
                        startDate: e.target.value,
                      })
                    }
                    required
                  />
                </label>
                <label className="field">
                  Fin
                  <input
                    type="date"
                    value={contractForm.endDate}
                    onChange={(e) =>
                      setContractForm({
                        ...contractForm,
                        endDate: e.target.value,
                      })
                    }
                  />
                </label>
              </div>
              <label className="field">
                Intitulé
                <input
                  value={contractForm.jobTitle}
                  onChange={(e) =>
                    setContractForm({
                      ...contractForm,
                      jobTitle: e.target.value,
                    })
                  }
                  required
                />
              </label>
              <div className="grid-2">
                <label className="field">
                  Heures / semaine
                  <input
                    value={contractForm.weeklyHours}
                    onChange={(e) =>
                      setContractForm({
                        ...contractForm,
                        weeklyHours: e.target.value,
                      })
                    }
                  />
                </label>
                <label className="field">
                  Brut mensuel (€)
                  <input
                    value={contractForm.grossEuros}
                    onChange={(e) =>
                      setContractForm({
                        ...contractForm,
                        grossEuros: e.target.value,
                      })
                    }
                  />
                </label>
              </div>
              <label className="field">
                Convention
                <input
                  value={contractForm.conventionRef}
                  onChange={(e) =>
                    setContractForm({
                      ...contractForm,
                      conventionRef: e.target.value,
                    })
                  }
                />
              </label>
              <button type="submit" disabled={busy}>
                Créer le contrat
              </button>
            </form>
          </section>
        </div>
      )}
    </div>
  );
}

import { useEffect, useState, type FormEvent } from "react";
import { api } from "../lib/api";
import { isTauriRuntime } from "../lib/updater";
import type { ThirdParty } from "../lib/types";
import {
  createActionLabel,
  kindLabel,
  modalTitle,
  suggestThirdPartyCode,
  toCreateThirdPartyInput,
  validateCreateThirdParty,
  type ThirdPartyKind,
} from "../modules/tax/thirdParty";

export type NewThirdPartyModalProps = {
  open: boolean;
  onClose: () => void;
  onCreated: (tier: ThirdParty) => void;
  existing: ThirdParty[];
  /** Default / locked kind when opening. */
  defaultKind?: ThirdPartyKind;
  /** If true, kind cannot be changed (devis / facture clients). */
  lockKind?: boolean;
  /** Allow both kinds in the form (TVA & tiers, lignes d’écritures). */
  allowAnyKind?: boolean;
};

type FormState = {
  code: string;
  name: string;
  kind: ThirdPartyKind;
  siret: string;
  email: string;
  address: string;
  routingId: string;
};

function emptyForm(
  existing: ThirdParty[],
  kind: ThirdPartyKind,
): FormState {
  return {
    code: suggestThirdPartyCode(existing, kind),
    name: "",
    kind,
    siret: "",
    email: "",
    address: "",
    routingId: "",
  };
}

export function NewThirdPartyModal({
  open,
  onClose,
  onCreated,
  existing,
  defaultKind = "customer",
  lockKind = false,
  allowAnyKind = false,
}: NewThirdPartyModalProps) {
  const [form, setForm] = useState<FormState>(() =>
    emptyForm(existing, defaultKind),
  );
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    if (!open) return;
    setError(null);
    setBusy(false);
    setForm(emptyForm(existing, defaultKind));
    // Reset only when the dialog opens — not on every parent list refresh.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open]);

  if (!open) return null;

  const titleKind: ThirdPartyKind | "any" = allowAnyKind
    ? "any"
    : lockKind
      ? defaultKind
      : "any";
  const title = modalTitle(titleKind);

  async function onSubmit(e: FormEvent) {
    e.preventDefault();
    setError(null);
    const validation = validateCreateThirdParty(form);
    if (validation) {
      setError(validation);
      return;
    }
    const input = toCreateThirdPartyInput(form);
    setBusy(true);
    try {
      let created: ThirdParty;
      if (isTauriRuntime()) {
        created = await api.createThirdParty(input);
      } else {
        // Browser / ?preview=1 — keep UX without Tauri backend.
        const maxId = existing.reduce((m, t) => Math.max(m, t.id), 0);
        created = {
          id: maxId + 1,
          code: input.code,
          name: input.name,
          kind: input.kind,
          accountNumber: input.kind === "customer" ? "411" : "401",
          active: true,
          siret: input.siret ?? null,
          email: input.email ?? null,
          address: input.address ?? null,
          routingId: input.routingId ?? null,
        };
      }
      onCreated(created);
      onClose();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      className="modal-backdrop"
      role="presentation"
      onClick={(ev) => {
        if (ev.target === ev.currentTarget && !busy) onClose();
      }}
    >
      <div
        className="modal-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-third-party-title"
      >
        <header className="modal-head">
          <h2 id="new-third-party-title">{title}</h2>
          <button
            type="button"
            className="ghost"
            onClick={onClose}
            disabled={busy}
            aria-label="Fermer"
          >
            Fermer
          </button>
        </header>
        <p className="muted small">
          Fiche minimale pour la facturation SAS : raison sociale, SIREN/SIRET,
          adresse et contact.
        </p>
        {error ? <p className="error">{error}</p> : null}
        <form className="stack stack-tight" onSubmit={(e) => void onSubmit(e)}>
          <div className="split-2">
            <label className="field">
              Code
              <input
                value={form.code}
                onChange={(e) => setForm({ ...form, code: e.target.value })}
                required
                autoFocus
              />
            </label>
            {(allowAnyKind || !lockKind) && (
              <label className="field">
                Type
                <select
                  value={form.kind}
                  onChange={(e) => {
                    const kind = e.target.value as ThirdPartyKind;
                    setForm({
                      ...form,
                      kind,
                      code: suggestThirdPartyCode(existing, kind),
                    });
                  }}
                >
                  <option value="customer">{kindLabel("customer")}</option>
                  <option value="supplier">{kindLabel("supplier")}</option>
                </select>
              </label>
            )}
          </div>
          <label className="field">
            Raison sociale / nom
            <input
              value={form.name}
              onChange={(e) => setForm({ ...form, name: e.target.value })}
              required
              placeholder="ex. Gamma SAS"
            />
          </label>
          <label className="field">
            SIREN ou SIRET
            <input
              value={form.siret}
              onChange={(e) => setForm({ ...form, siret: e.target.value })}
              inputMode="numeric"
              placeholder="9 ou 14 chiffres"
            />
          </label>
          <label className="field">
            Adresse
            <textarea
              value={form.address}
              onChange={(e) => setForm({ ...form, address: e.target.value })}
              rows={2}
              placeholder="Rue, code postal, ville"
            />
          </label>
          <div className="split-2">
            <label className="field">
              E-mail
              <input
                type="email"
                value={form.email}
                onChange={(e) => setForm({ ...form, email: e.target.value })}
                placeholder="contact@exemple.fr"
              />
            </label>
            <label className="field">
              Identifiant de routage
              <input
                value={form.routingId}
                onChange={(e) =>
                  setForm({ ...form, routingId: e.target.value })
                }
                placeholder="Souvent le SIRET"
              />
            </label>
          </div>
          <div className="btn-row">
            <button type="submit" className="primary" disabled={busy}>
              {busy ? "Création…" : createActionLabel(form.kind)}
            </button>
            <button
              type="button"
              className="ghost"
              onClick={onClose}
              disabled={busy}
            >
              Annuler
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

import { useEffect, useState, type FormEvent, type MouseEvent } from "react";
import { createPortal } from "react-dom";
import { api } from "../lib/api";
import { isTauriRuntime } from "../lib/updater";
import type { ThirdParty } from "../lib/types";
import {
  kindLabel,
  modalTitle,
  submitActionLabel,
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

function formatCreateError(err: unknown): string {
  const raw = err instanceof Error ? err.message : String(err);
  const text = raw.replace(/^Error:\s*/i, "").trim() || "Création impossible.";
  if (/unique|constraint|déjà|already|duplicate/i.test(text)) {
    return "Ce code tiers existe déjà. Choisissez un autre code.";
  }
  if (/workspace|aucun workspace/i.test(text)) {
    return "Workspace indisponible. Rouvrez la société, puis réessayez.";
  }
  return text;
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

  useEffect(() => {
    if (!open) return;
    function onKey(ev: KeyboardEvent) {
      if (ev.key === "Escape" && !busy) {
        ev.preventDefault();
        ev.stopPropagation();
        onClose();
      }
    }
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [open, busy, onClose]);

  if (!open) return null;
  if (typeof document === "undefined") return null;

  const titleKind: ThirdPartyKind | "any" = allowAnyKind
    ? "any"
    : lockKind
      ? defaultKind
      : "any";
  const title = modalTitle(titleKind);

  async function onSubmit(e: FormEvent) {
    // Nested under devis/facture <form> before portal — always kill default
    // submit (webview reload → welcome) and stop bubbling to parent save.
    e.preventDefault();
    e.stopPropagation();
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
      setError(formatCreateError(err));
    } finally {
      setBusy(false);
    }
  }

  function stopCardBubble(ev: MouseEvent) {
    ev.stopPropagation();
  }

  return createPortal(
    <div
      className="modal-backdrop"
      role="presentation"
      onClick={(ev) => {
        if (ev.target === ev.currentTarget && !busy) onClose();
      }}
      onMouseDown={(ev) => {
        // Avoid activating a parent <label> (select) under the overlay.
        ev.stopPropagation();
      }}
    >
      <div
        className="modal-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="new-third-party-title"
        onClick={stopCardBubble}
        onMouseDown={stopCardBubble}
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
        {error ? (
          <p className="error" role="alert">
            {error}
          </p>
        ) : null}
        <form
          className="stack stack-tight"
          noValidate
          onSubmit={(e) => void onSubmit(e)}
        >
          <div className="split-2">
            <label className="field">
              Code
              <input
                value={form.code}
                onChange={(e) => setForm({ ...form, code: e.target.value })}
                required
                autoFocus
                autoComplete="off"
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
              autoComplete="organization"
            />
          </label>
          <label className="field">
            SIREN ou SIRET
            <input
              value={form.siret}
              onChange={(e) => setForm({ ...form, siret: e.target.value })}
              inputMode="numeric"
              placeholder="9 ou 14 chiffres"
              autoComplete="off"
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
                autoComplete="email"
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
                autoComplete="off"
              />
            </label>
          </div>
          <div className="btn-row">
            <button type="submit" className="primary" disabled={busy}>
              {busy ? "Création…" : submitActionLabel(form.kind)}
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
    </div>,
    document.body,
  );
}

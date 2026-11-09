import { useState } from "react";
import type { ThirdParty } from "../lib/types";
import {
  createActionLabel,
  type ThirdPartyKind,
} from "../modules/tax/thirdParty";
import { NewThirdPartyModal } from "./NewThirdPartyModal";

export type ThirdPartySelectProps = {
  /** Current selected id ; 0 / null / "" = empty. */
  value: number | null | undefined;
  onChange: (id: number | null) => void;
  /** Full tiers list (used for suggestions + after-create merge). */
  tiers: ThirdParty[];
  /** Called after create so the parent refreshes its list (and optionally selects). */
  onCreated: (tier: ThirdParty) => void;
  /** Filter options; default customer for billing. */
  kind?: ThirdPartyKind | "any";
  required?: boolean;
  allowEmpty?: boolean;
  disabled?: boolean;
  emptyLabel?: string;
  /** Hide the Nouveau button (rare). */
  hideCreate?: boolean;
  className?: string;
  /** Compact row layout for line editors. */
  compact?: boolean;
};

function filterTiers(
  tiers: ThirdParty[],
  kind: ThirdPartyKind | "any",
): ThirdParty[] {
  const active = tiers.filter((t) => t.active);
  if (kind === "any") return active;
  return active.filter(
    (t) =>
      t.kind === kind ||
      (kind === "customer"
        ? t.accountNumber.startsWith("411")
        : t.accountNumber.startsWith("401")),
  );
}

export function ThirdPartySelect({
  value,
  onChange,
  tiers,
  onCreated,
  kind = "customer",
  required = false,
  allowEmpty = !required,
  disabled = false,
  emptyLabel = "Choisir…",
  hideCreate = false,
  className,
  compact = false,
}: ThirdPartySelectProps) {
  const [open, setOpen] = useState(false);
  const options = filterTiers(tiers, kind);
  const createKind: ThirdPartyKind =
    kind === "any" ? "customer" : kind;
  const lockKind = kind !== "any";
  const buttonLabel = createActionLabel(kind);

  function handleCreated(tier: ThirdParty) {
    onCreated(tier);
    onChange(tier.id);
  }

  return (
    <>
      <div
        className={
          compact
            ? `third-party-select compact ${className ?? ""}`
            : `third-party-select ${className ?? ""}`
        }
      >
        <select
          value={value || ""}
          disabled={disabled}
          required={required}
          onChange={(e) =>
            onChange(e.target.value ? Number(e.target.value) : null)
          }
        >
          {(allowEmpty || !value) && (
            <option value="" disabled={required && !allowEmpty}>
              {allowEmpty ? "—" : emptyLabel}
            </option>
          )}
          {options.map((t) => (
            <option key={t.id} value={t.id}>
              {t.code} — {t.name}
            </option>
          ))}
        </select>
        {!hideCreate && !disabled && (
          <button
            type="button"
            className={compact ? "linkish" : "ghost"}
            onClick={() => setOpen(true)}
          >
            {compact ? "+" : buttonLabel}
          </button>
        )}
      </div>
      <NewThirdPartyModal
        open={open}
        onClose={() => setOpen(false)}
        onCreated={handleCreated}
        existing={tiers}
        defaultKind={createKind}
        lockKind={lockKind}
        allowAnyKind={kind === "any"}
      />
    </>
  );
}

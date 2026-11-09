import type { CreateThirdPartyInput, ThirdParty } from "../../lib/types";

export type ThirdPartyKind = "customer" | "supplier";

export function kindLabel(kind: ThirdPartyKind): string {
  return kind === "customer" ? "Client" : "Fournisseur";
}

export function createActionLabel(kind: ThirdPartyKind | "any"): string {
  if (kind === "customer") return "Nouveau client";
  if (kind === "supplier") return "Nouveau fournisseur";
  return "Nouveau tiers";
}

export function modalTitle(kind: ThirdPartyKind | "any"): string {
  if (kind === "customer") return "Nouveau client";
  if (kind === "supplier") return "Nouveau fournisseur";
  return "Nouveau tiers";
}

/** Suggest next C001 / F001 style code from existing tiers. */
export function suggestThirdPartyCode(
  existing: Pick<ThirdParty, "code" | "kind">[],
  kind: ThirdPartyKind,
): string {
  const prefix = kind === "customer" ? "C" : "F";
  let max = 0;
  const re = new RegExp(`^${prefix}(\\d+)$`, "i");
  for (const t of existing) {
    const m = t.code.trim().match(re);
    if (m) {
      max = Math.max(max, Number(m[1]));
    }
  }
  return `${prefix}${String(max + 1).padStart(3, "0")}`;
}

export function digitsOnly(value: string): string {
  return value.replace(/\D/g, "");
}

export function validateCreateThirdParty(input: {
  code: string;
  name: string;
  kind: string;
  siret: string;
  email: string;
}): string | null {
  if (!input.code.trim()) {
    return "Indiquez un code tiers (ex. C003).";
  }
  if (input.name.trim().length < 2) {
    return "Indiquez la raison sociale ou le nom (au moins 2 caractères).";
  }
  if (input.kind !== "customer" && input.kind !== "supplier") {
    return "Type de tiers invalide.";
  }
  const id = digitsOnly(input.siret);
  if (id && id.length !== 9 && id.length !== 14) {
    return "SIREN (9 chiffres) ou SIRET (14 chiffres), sinon laissez vide.";
  }
  const email = input.email.trim();
  if (email && !/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email)) {
    return "Adresse e-mail invalide.";
  }
  return null;
}

export function toCreateThirdPartyInput(form: {
  code: string;
  name: string;
  kind: ThirdPartyKind;
  siret: string;
  email: string;
  address: string;
  routingId: string;
}): CreateThirdPartyInput {
  const siret = digitsOnly(form.siret) || null;
  const email = form.email.trim() || null;
  const address = form.address.trim() || null;
  const routingId = form.routingId.trim() || null;
  return {
    code: form.code.trim(),
    name: form.name.trim(),
    kind: form.kind,
    siret,
    email,
    address,
    routingId,
  };
}

# SasCompta

Comptabilité générale desktop pour une **SAS française** (mono-entité).

Stack : **Tauri 2** + **React** + **TypeScript** + **Vite** + **SQLite** (fichier `.sascompta`), mot de passe local Argon2.

## Prérequis

- Node 20+
- Rust **1.83** (`edition = "2021"`)

## Lancer

```bash
npm install
npm run tauri dev
```

Workspace démo : mot de passe `SecretPass1`, Demo SAS, SIREN `123456782`, exercice 2026.

## Checks

```bash
npm run check   # lint + typecheck + vitest + cargo test
```

## CLI

```bash
# Créer un workspace démo
cargo run --manifest-path src-tauri/Cargo.toml --bin seed_demo_workspace -- /tmp/demo.sascompta

# Exporter TVA CA3 + balance auxiliaire (alias : export_demo_tax)
cargo run --manifest-path src-tauri/Cargo.toml --bin demo_export_tax -- /tmp/demo.sascompta ./out
```

## Jalons livrés (J0–J4)

- J0 workspace + Ma SAS
- J1 PCG + journaux ACH/VTE/BQ/OD/AN
- J2 écritures draft/post/reverse/attachments
- J3 grand livre + balance + CSV/PDF
- J4 TVA + tiers + CA3 + balance auxiliaire

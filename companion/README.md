# SasCompta — app compagnon (Wi‑Fi local)

Petite app **Expo / React Native** pour envoyer photos et PDF vers SasCompta desktop sur le **même réseau Wi‑Fi**. Aucun cloud obligatoire. Inclus dans le produit (pas d’add-on payant).

## Prérequis

- Node 20+
- Téléphone Android (MVP) ou iOS, **même Wi‑Fi** que le PC
- SasCompta desktop avec un workspace ouvert → **Import docs** ou **Ma SAS** → *Activer le pont Wi‑Fi*

## Installation MVP

### Option A — Expo Go (le plus rapide)

```bash
cd companion
npm install
npx expo start
```

1. Installez **Expo Go** depuis le Play Store (Android) ou l’App Store (iOS).
2. Scannez le QR Expo affiché dans le terminal.
3. Dans l’app compagnon : scannez le **QR SasCompta** du PC (ou saisissez IP + code à 6 chiffres).

> iOS / Expo Go : le transfert HTTP LAN fonctionne sur le réseau local ; si le scan QR Expo échoue, utilisez le mode tunnel uniquement pour charger l’app, puis revenez en LAN pour le pont SasCompta.

### Option B — APK Android (sideload)

```bash
cd companion
npm install
npx expo install
# Build local (besoin Android SDK) ou EAS :
# npx eas-cli build -p android --profile preview
```

Installez l’APK via `adb install …` ou fichier partagé. Pas de compte store requis pour un sideload interne.

## Parcours

1. PC : activer le pont → noter code / QR (valide ~5 min).
2. Téléphone : appairer (QR ou IP + code) → jeton de session local.
3. Choisir un **type** (indice), photo / galerie / PDF → envoi.
4. PC : le document apparaît dans **Import docs → En revue** ; confirmation humaine avant écriture.

## Protocole (`/v1`)

| Méthode | Route | Auth |
|---------|-------|------|
| `GET` | `/v1/info` | non |
| `POST` | `/v1/pair` body `{ "code": "123456" }` | code court |
| `POST` | `/v1/upload` raw body + `Authorization: Bearer …` + `X-Doc-Type` + `X-File-Name` | jeton |

Port par défaut : **17832**. Fichier max ~15 Mo.

## Hors périmètre MVP

- Publication stores iOS/Android soignée
- Relais Internet / cloud
- OCR embarqué sur le téléphone

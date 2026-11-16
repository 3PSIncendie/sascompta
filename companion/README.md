# SasCompta — app compagnon (Wi‑Fi local)

Petite app **Expo / React Native** pour envoyer photos et PDF vers SasCompta desktop sur le **même réseau Wi‑Fi**. Aucun cloud obligatoire. Inclus dans le produit (pas d’add-on payant).

**Android MVP : Expo Go** — pas besoin d’APK SasCompta. Le QR du pont desktop ouvre une page « Ouvrir Expo Go » (Intent `host.exp.exponent`), pas une installation native manquante.

## Prérequis

- Node 20+
- Téléphone **Android** (MVP) avec **Expo Go** (Play Store), **même Wi‑Fi** que le PC
- SasCompta desktop avec un workspace ouvert → **Import docs** ou **Ma SAS** → *Activer le pont Wi‑Fi*

## Installation MVP (Android + Expo Go)

```bash
cd companion
npm install
npx expo start
```

1. Installez **Expo Go** : [Play Store](https://play.google.com/store/apps/details?id=host.exp.exponent).
2. Scannez le **QR Expo** du terminal (`exp://…`) avec Expo Go — cela charge le projet compagnon.
3. Sur le PC : activez le pont Wi‑Fi.
4. Dans l’app compagnon : **Scanner le QR** du PC (ou saisissez IP + code à 6 chiffres).

### Si vous scannez le QR du PC avec la caméra Android

Le QR encode une URL HTTP du pont (`http://IP:17832/launch?…`), **pas** `sascompta://`.
Le navigateur affiche une page avec :

- bouton **Ouvrir Expo Go** (Intent Android → package `host.exp.exponent`)
- lien Play Store si Expo Go n’est pas installé
- le code d’appairage à saisir dans l’app

> Ne cherchez pas une APK « SasCompta Compagnon » : le parcours supporté est Expo Go.

### Option avancée — APK Android (sideload)

Réservée aux builds internes ; **non requise** pour le MVP.

```bash
cd companion
npm install
# Build local (Android SDK) ou EAS :
# npx eas-cli build -p android --profile preview
```

Installez via `adb install …` ou fichier partagé. Le schéma `sascompta://` est déclaré dans `app.json` pour ces builds ; le QR desktop reste en HTTP `/launch` pour rester compatible Expo Go.

## Parcours

1. PC : activer le pont → noter code / QR (valide ~5 min).
2. Téléphone : Expo Go → projet compagnon → appairer (QR ou IP + code).
3. Choisir un **type** (indice), photo / galerie / PDF → envoi.
4. PC : le document apparaît dans **Import docs → En revue** ; confirmation humaine avant écriture.

## Protocole (`/v1`)

| Méthode | Route | Auth |
|---------|-------|------|
| `GET` | `/v1/info` | non |
| `POST` | `/v1/pair` body `{ "code": "123456" }` | code court |
| `POST` | `/v1/upload` raw body + `Authorization: Bearer …` + `X-Doc-Type` + `X-File-Name` | jeton |
| `GET` | `/launch` (et `/`) | page HTML « Ouvrir Expo Go » |

Port par défaut : **17832**. Fichier max ~15 Mo.

### QR d’appairage

Contenu (HTTP, compatible caméra Android + parse in-app) :

```text
http://192.168.1.42:17832/launch?v=1&code=482913
```

L’app accepte aussi l’ancien URI `sascompta://pair?host=&port=&code=`, les Intent Android avec `S.browser_fallback_url=…`, et le JSON legacy.

HTTP clair sur le LAN : Android `usesCleartextTraffic`, iOS `NSAllowsLocalNetworking` (voir `app.json`).

## Hors périmètre MVP

- Publication stores iOS/Android soignée
- Relais Internet / cloud
- OCR embarqué sur le téléphone

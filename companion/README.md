# SasCompta — app compagnon (Wi‑Fi local)

Petite app **Expo / React Native** *et* page web servie par le pont desktop pour envoyer photos et PDF vers SasCompta sur le **même réseau Wi‑Fi**. Aucun cloud obligatoire.

**Android MVP recommandé : caméra → Chrome** sur `http://IP:17832/launch` — pas besoin d’APK ni d’Expo Go pour envoyer une pièce. Expo Go reste disponible pour développer / utiliser l’app React Native.

## Prérequis

- Téléphone **Android** (MVP) et PC sur le **même Wi‑Fi** (pas d’isolation « invité »)
- SasCompta desktop avec un workspace ouvert → **Import docs** ou **Ma SAS** → *Activer le pont Wi‑Fi*
- Pare-feu Windows : autoriser **SasCompta** / TCP **17832** (entrant)

## Parcours Android (QR / URL)

1. PC : activer le pont → noter le QR et l’**adresse LAN** affichée.
2. Téléphone : scanner le QR avec la **caméra système** (ou ouvrir l’URL `http://IP:17832/launch?v=1&code=…` dans Chrome).
3. La page charge → appairage auto → choisir photo / PDF → **Envoyer au PC**.
4. PC : document dans **Import docs → En revue**.

### Si la page ne s’ouvre pas

| Cause probable | Correctif |
|----------------|-----------|
| Mauvaise IP dans le QR (VPN, Hyper‑V, Docker, `127.0.0.1`) | Sur le PC, choisir l’IP **Wi‑Fi** (`192.168.…`) dans le panneau compagnon → rescanner |
| Pare-feu bloque le port | Autoriser TCP 17832 entrant pour SasCompta |
| Pas le même Wi‑Fi / Wi‑Fi invité | Même SSID, désactiver l’isolation client |
| Chrome « site inaccessible » | Vérifier l’IP ; le pont écoute sur `0.0.0.0` (toutes interfaces) en HTTP clair |

Le QR encode **`http://{host}:{port}/launch?v=1&code=…`** (pas `sascompta://`, pas `exp://`).

## Option — Expo Go / app React Native

```bash
cd companion
npm install
npx expo start
```

1. Installez **Expo Go** (Play Store).
2. Scannez le **QR Expo** du terminal (`exp://…`) pour charger le projet.
3. Dans l’app : **Scanner le QR** du pont desktop (pas le QR Metro).

`usesCleartextTraffic` + ATS local sont activés dans `app.json` pour HTTP LAN.

### Option avancée — APK Android (sideload)

Réservée aux builds internes ; **non requise** pour le MVP. Schéma `sascompta://` déclaré pour ces builds ; le QR desktop reste en HTTP `/launch`.

## Protocole (`/v1`)

| Méthode | Route | Auth |
|---------|-------|------|
| `GET` | `/v1/info` | non |
| `POST` | `/v1/pair` body `{ "code": "123456" }` | code court |
| `POST` | `/v1/upload` raw body + `Authorization: Bearer …` + `X-Doc-Type` + `X-File-Name` | jeton |
| `GET` | `/launch` (et `/`) | page HTML envoi Chrome (+ Expo optionnel) |

Port par défaut : **17832**. Écoute : **`0.0.0.0`**. Fichier max ~15 Mo.

### QR d’appairage

```text
http://192.168.1.42:17832/launch?v=1&code=482913
```

L’app React Native accepte aussi l’ancien URI `sascompta://pair?…`, les Intent Android avec `S.browser_fallback_url=…`, et le JSON legacy.

## Hors périmètre MVP

- Publication stores iOS/Android soignée
- Relais Internet / cloud
- OCR embarqué sur le téléphone

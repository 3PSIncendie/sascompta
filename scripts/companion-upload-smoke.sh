#!/usr/bin/env bash
# Smoke HTTP du pont compagnon (PC déjà démarré : Import docs → Activer le pont).
# Usage :
#   CODE=482913 HOST=192.168.1.42 ./scripts/companion-upload-smoke.sh [fichier]
set -euo pipefail
HOST="${HOST:-127.0.0.1}"
PORT="${PORT:-17832}"
CODE="${CODE:?Indiquez CODE=xxxxxx (6 chiffres affichés sur le PC)}"
FILE="${1:-}"

BASE="http://${HOST}:${PORT}"
echo "== GET $BASE/v1/info"
curl -sS "$BASE/v1/info" | head -c 400
echo

echo "== POST /v1/pair"
TOKEN=$(curl -sS -X POST "$BASE/v1/pair" \
  -H 'Content-Type: application/json' \
  -d "{\"code\":\"${CODE}\"}" | python3 -c 'import sys,json; print(json.load(sys.stdin)["token"])')
echo "token ok (${#TOKEN} chars)"

TMP="${FILE:-}"
CLEANUP=0
if [[ -z "$TMP" ]]; then
  TMP="$(mktemp /tmp/sascompta-smoke-XXXX.pdf)"
  CLEANUP=1
  printf '%%PDF-1.4 smoke ticket 12,50\n' > "$TMP"
fi

echo "== POST /v1/upload ($TMP)"
curl -sS -X POST "$BASE/v1/upload" \
  -H "Authorization: Bearer ${TOKEN}" \
  -H "Content-Type: application/pdf" \
  -H "X-Doc-Type: ticket" \
  -H "X-File-Name: $(basename "$TMP")" \
  --data-binary @"$TMP"
echo

if [[ "$CLEANUP" -eq 1 ]]; then
  rm -f "$TMP"
fi
echo "OK — vérifiez Import docs → En revue sur le PC."

#!/usr/bin/env bash
# Run the Callisto frontend (Vite dev server) for local development.
#
# Usage:
#   scripts/dev-fe.sh
#
# Notes:
#   - Reads fe/callisto/.env for VITE_CALLISTO_BACKEND, VITE_NODE_SERVER, and
#     VITE_GOOGLE_OAUTH_CLIENT_ID. No manual exports needed.
#   - The OAuth client allows http://localhost:50001 as a redirect URI, so the
#     dev server must run on port 50001.
#   - Forwards extra arguments to `npm start`.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/fe/callisto"

if [[ ! -d node_modules ]]; then
  echo "[dev-fe] node_modules missing; running npm install --legacy-peer-deps."
  npm install --legacy-peer-deps
fi

if [[ ! -f .env ]]; then
  echo "[dev-fe] WARNING: fe/callisto/.env is missing." >&2
  echo "[dev-fe] Copy .env.example and fill in VITE_GOOGLE_OAUTH_CLIENT_ID." >&2
fi

# Tee output so Claude Code can read recent logs. Override with DEV_FE_LOG=...
LOG_FILE="${DEV_FE_LOG:-/tmp/callisto-fe.log}"
echo "[dev-fe] Starting Vite on port 50001 (logs -> $LOG_FILE)."
npm start -- --port 50001 "$@" 2>&1 | tee "$LOG_FILE"

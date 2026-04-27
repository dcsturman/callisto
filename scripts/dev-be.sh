#!/usr/bin/env bash
# Run the Callisto backend natively via cargo for local development.
#
# Usage:
#   scripts/dev-be.sh           # local scenario files (callisto/scenarios)
#   scripts/dev-be.sh --gcs     # read scenarios from gs://callisto-scenarios
#
# Notes:
#   - Builds with --features no_tls_upgrade so the frontend can talk ws:// directly.
#     (fe/callisto/.env points VITE_CALLISTO_BACKEND at http://localhost:30000.)
#   - --gcs requires `gcloud auth application-default login` to have been run.
#   - Real Google OAuth is used either way; --test mode on the backend doesn't help
#     end-to-end testing because the frontend always does the OAuth flow.

set -eo pipefail

USE_GCS=0
EXTRA_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --gcs) USE_GCS=1 ;;
    --) shift; EXTRA_ARGS+=("$@"); break ;;
    *)  EXTRA_ARGS+=("$arg") ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT/callisto"

CARGO_ARGS=(--features no_tls_upgrade)
# Match the docker/frontend port. The cargo default is 8443; the frontend's
# .env points at localhost:30000.
RUN_ARGS=(--port 30000)

if [[ $USE_GCS -eq 1 ]]; then
  ADC_PATH="${GOOGLE_APPLICATION_CREDENTIALS:-$HOME/.config/gcloud/application_default_credentials.json}"
  if [[ ! -f "$ADC_PATH" ]]; then
    echo "GCS mode requested but no application-default credentials found at $ADC_PATH." >&2
    echo "Run: gcloud auth application-default login" >&2
    exit 1
  fi
  export GOOGLE_APPLICATION_CREDENTIALS="$ADC_PATH"
  RUN_ARGS+=(--scenario-dir gs://callisto-scenarios)
  echo "[dev-be] Using GCS bucket gs://callisto-scenarios for scenarios."
else
  echo "[dev-be] Using local scenario files in callisto/scenarios."
fi

# RUST_LOG defaults to something useful; user can override.
export RUST_LOG="${RUST_LOG:-debug,gomez=warn,h2=warn,hyper=warn,reqwest=warn,rustls=warn}"

# Sentry: DSN baked in so dev runs report errors automatically. Tagged as
# "development" so they're filterable from canary/prod in the Sentry UI.
# Set SENTRY_DSN= (empty) to disable for a session.
export SENTRY_DSN="${SENTRY_DSN-https://2bd6b4d950ccb59ce589533dcf1b253a@o4511288095080448.ingest.us.sentry.io/4511288117755904}"
export SENTRY_ENVIRONMENT="${SENTRY_ENVIRONMENT:-development}"

# Tee output to a known location so Claude Code can read recent logs without
# attaching to the running terminal. Override with DEV_BE_LOG=...
LOG_FILE="${DEV_BE_LOG:-/tmp/callisto-be.log}"
echo "[dev-be] cargo run ${CARGO_ARGS[*]} -- ${RUN_ARGS[*]} ${EXTRA_ARGS[*]} (logs -> $LOG_FILE)"
cargo run "${CARGO_ARGS[@]}" -- "${RUN_ARGS[@]}" "${EXTRA_ARGS[@]}" 2>&1 | tee "$LOG_FILE"

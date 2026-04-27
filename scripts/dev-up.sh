#!/usr/bin/env bash
# Bring up the Callisto backend in Docker (option 2 from the README).
#
# Usage:
#   scripts/dev-up.sh             # local scenario files via compose.override.yaml
#   scripts/dev-up.sh --gcs       # read scenarios from gs://callisto-scenarios
#   scripts/dev-up.sh --build     # force a rebuild
#   scripts/dev-up.sh --gcs --build
#
# After this is up, run scripts/dev-fe.sh in another terminal.

set -eo pipefail

USE_GCS=0
DO_BUILD=0
EXTRA_ARGS=()
for arg in "$@"; do
  case "$arg" in
    --gcs)   USE_GCS=1 ;;
    --build) DO_BUILD=1 ;;
    *)       EXTRA_ARGS+=("$arg") ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

# compose.yaml mounts ~/.config/gcloud/application_default_credentials.json as the
# gcs_credentials secret. Make sure it's there before we try GCS mode.
if [[ $USE_GCS -eq 1 ]]; then
  ADC="$HOME/.config/gcloud/application_default_credentials.json"
  if [[ ! -f "$ADC" ]]; then
    echo "GCS mode requested but $ADC is missing." >&2
    echo "Run: gcloud auth application-default login" >&2
    exit 1
  fi
fi

COMPOSE_FILES=(-f compose.yaml)
# compose.override.yaml is auto-loaded if present, but we name it explicitly so
# the layering is obvious when we add compose.gcs.yaml on top.
if [[ -f compose.override.yaml ]]; then
  COMPOSE_FILES+=(-f compose.override.yaml)
fi
if [[ $USE_GCS -eq 1 ]]; then
  COMPOSE_FILES+=(-f compose.gcs.yaml)
fi

if [[ $DO_BUILD -eq 1 ]]; then
  echo "[dev-up] docker compose ${COMPOSE_FILES[*]} build be"
  docker compose "${COMPOSE_FILES[@]}" build be
fi

echo "[dev-up] docker compose ${COMPOSE_FILES[*]} up be ${EXTRA_ARGS[*]}"
exec docker compose "${COMPOSE_FILES[@]}" up be "${EXTRA_ARGS[@]}"

#!/usr/bin/env bash
# Upload all local scenarios in callisto/scenarios/ to a Callisto scenario bucket.
#
# Environments (-e / --env, default: canary):
#   canary  gs://callisto-canary-scenarios  (project callisto-canary)
#   prod    gs://callisto-scenarios         (project callisto-1731280702227)
#   both    canary first, then prod, each reported separately
# Prod is never the default — you have to ask for it by name.
#
# Usage:
#   scripts/upload-scenarios.sh              # interactive, uploads to canary
#   scripts/upload-scenarios.sh --env prod   # upload to production
#   scripts/upload-scenarios.sh --env both   # upload to canary, then prod
#   scripts/upload-scenarios.sh --dry-run    # show what would be uploaded
#   scripts/upload-scenarios.sh --yes        # skip the confirmation prompt
#
# The bucket name can be overridden, which wins over --env and collapses the
# upload to that one destination:
#   DEST_BUCKET=gs://my-bucket scripts/upload-scenarios.sh
#
# Requires gcloud / gsutil auth (e.g. `gcloud auth login` or service account).
# Existing GCS objects with the same name are overwritten; objects only in GCS
# are NOT deleted (use this for "push my edits up", not for full sync).

set -eo pipefail

DRY_RUN=0
SKIP_CONFIRM=0
TARGET_ENV=canary
# A while/shift loop rather than `for arg in "$@"` because --env takes a value.
while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run) DRY_RUN=1 ;;
    --yes|-y)  SKIP_CONFIRM=1 ;;
    --env=*)   TARGET_ENV="${1#--env=}" ;;
    -e|--env)
      if [[ $# -lt 2 ]]; then
        echo "Missing value for $1 (expected prod, canary or both)" >&2
        exit 2
      fi
      TARGET_ENV="$2"
      shift
      ;;
    -h|--help)
      sed -n '2,23p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 2
      ;;
  esac
  shift
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/callisto/scenarios"

PROD_BUCKET="gs://callisto-scenarios"
CANARY_BUCKET="gs://callisto-canary-scenarios"

case "$TARGET_ENV" in
  canary) DEST_BUCKETS=("$CANARY_BUCKET") ;;
  prod)   DEST_BUCKETS=("$PROD_BUCKET") ;;
  both)   DEST_BUCKETS=("$CANARY_BUCKET" "$PROD_BUCKET") ;;
  *)
    echo "Unknown environment: $TARGET_ENV (expected prod, canary or both)" >&2
    exit 2
    ;;
esac

# An explicit DEST_BUCKET is the escape hatch for one-off buckets; it replaces
# whatever --env selected.
if [[ -n "${DEST_BUCKET:-}" ]]; then
  DEST_BUCKETS=("$DEST_BUCKET")
  TARGET_ENV="DEST_BUCKET override"
fi

# Loud, human-readable label per destination so nobody pushes to prod by accident.
ENV_LABELS=()
for bucket in "${DEST_BUCKETS[@]}"; do
  case "$bucket" in
    "$PROD_BUCKET")   ENV_LABELS+=("PROD") ;;
    "$CANARY_BUCKET") ENV_LABELS+=("canary") ;;
    *)                ENV_LABELS+=("custom") ;;
  esac
done

if [[ ! -d "$SRC_DIR" ]]; then
  echo "Scenario directory not found: $SRC_DIR" >&2
  exit 1
fi

# Build the file list. nullglob protects against the literal "*.json"
# sneaking through if there are no matches. Direct array assignment from a
# glob works on macOS bash 3.2 (mapfile is bash 4+).
shopt -s nullglob
LOCAL_FILES=("$SRC_DIR"/*.json)
shopt -u nullglob

if [[ ${#LOCAL_FILES[@]} -eq 0 ]]; then
  echo "No *.json files found in $SRC_DIR." >&2
  exit 1
fi

# Sanity-check JSON before pushing — a malformed file would land in the bucket
# and the running server would skip it with a parse-error log on next reload.
for f in "${LOCAL_FILES[@]}"; do
  if ! jq empty "$f" >/dev/null 2>&1; then
    echo "Refusing to upload $f: not valid JSON." >&2
    exit 1
  fi
done

# "canary (gs://…) and PROD (gs://…)" — reused in the prompt so the confirmation
# names every bucket that is about to be written.
DEST_SUMMARY=""
for ((i = 0; i < ${#DEST_BUCKETS[@]}; i++)); do
  [[ -n "$DEST_SUMMARY" ]] && DEST_SUMMARY+=" and "
  DEST_SUMMARY+="${ENV_LABELS[$i]} (${DEST_BUCKETS[$i]})"
done

echo "Local source:  $SRC_DIR"
echo "Environment:   $TARGET_ENV"
echo "Destination:   $DEST_SUMMARY"
echo
echo "Files to upload (${#LOCAL_FILES[@]}):"
for f in "${LOCAL_FILES[@]}"; do
  printf '  %s\n' "$(basename "$f")"
done
echo

# Show which files already exist in each destination (they will be overwritten)
# so the user knows what's about to change.
for ((i = 0; i < ${#DEST_BUCKETS[@]}; i++)); do
  echo "Currently in ${ENV_LABELS[$i]} — ${DEST_BUCKETS[$i]}:"
  if ! gsutil ls "${DEST_BUCKETS[$i]}/" 2>&1 | sed 's|^|  |'; then
    echo "  (could not list bucket; gsutil auth issue?)" >&2
  fi
  echo
done

if [[ $DRY_RUN -eq 1 ]]; then
  echo "[dry-run] Skipping upload."
  exit 0
fi

if [[ $SKIP_CONFIRM -ne 1 ]]; then
  read -r -p "Proceed with upload to $DEST_SUMMARY? [y/N] " reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

# `-m` parallelizes the uploads. `-c` (cache control) and `-r` (recursive)
# aren't needed for flat *.json copy. Trailing slash on dest is essential so
# gsutil treats it as a directory rather than renaming files. Each destination
# is reported on its own so a partial failure under `--env both` is visible.
EXIT_CODE=0
for ((i = 0; i < ${#DEST_BUCKETS[@]}; i++)); do
  echo "Uploading to ${ENV_LABELS[$i]} — ${DEST_BUCKETS[$i]} …"
  if gsutil -m cp "${LOCAL_FILES[@]}" "${DEST_BUCKETS[$i]}/"; then
    echo
    echo "  ${ENV_LABELS[$i]}: uploaded ${#LOCAL_FILES[@]} file(s). ${DEST_BUCKETS[$i]} now contains:"
    gsutil ls "${DEST_BUCKETS[$i]}/" | sed 's|^|    |'
  else
    echo "  ${ENV_LABELS[$i]}: FAILED to upload to ${DEST_BUCKETS[$i]}" >&2
    EXIT_CODE=1
  fi
  echo
done

exit $EXIT_CODE

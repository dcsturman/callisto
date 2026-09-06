#!/usr/bin/env bash
# Upload one or more ship-design JSON files to a Callisto ship-templates bucket.
#
# Environments (-e / --env, default: canary):
#   canary  gs://callisto-canary-ship-templates  (project callisto-canary)
#   prod    gs://callisto-ship-templates         (project callisto-1731280702227)
#   both    canary first, then prod, each reported separately
# Prod is never the default — you have to ask for it by name.
#
# Usage:
#   scripts/upload-designs.sh                            # every *.json in callisto/ship_templates/ -> canary
#   scripts/upload-designs.sh foo.json bar.json          # upload just those (paths can be absolute, repo-relative, or bare names that resolve in callisto/ship_templates/)
#   scripts/upload-designs.sh --env prod foo.json        # upload to production
#   scripts/upload-designs.sh --env both                 # upload to canary, then prod
#   scripts/upload-designs.sh --dry-run new_design.json  # show what would be uploaded, don't actually copy
#   scripts/upload-designs.sh --yes new_design.json      # skip the confirmation prompt
#
# The bucket name can be overridden, which wins over --env and collapses the
# upload to that one destination:
#   DEST_BUCKET=gs://my-bucket scripts/upload-designs.sh new_design.json
#
# Requires gcloud / gsutil auth (e.g. `gcloud auth login` or a service account).
# Existing GCS objects with the same name are overwritten. Objects only in GCS
# are NOT deleted — use this to push edits up, not as a full mirror.
#
# After upload the running server picks up the new file on its next 5s
# fingerprint poll (no restart needed).

set -eo pipefail

DRY_RUN=0
SKIP_CONFIRM=0
TARGET_ENV=canary
POSITIONAL=()
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
      sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "Unknown flag: $1" >&2
      exit 2
      ;;
    *)
      POSITIONAL+=("$1")
      ;;
  esac
  shift
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/callisto/ship_templates"

PROD_BUCKET="gs://callisto-ship-templates"
CANARY_BUCKET="gs://callisto-canary-ship-templates"

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
  echo "Ship-templates directory not found: $SRC_DIR" >&2
  exit 1
fi

# Resolve each positional arg to an actual file. Lookup order:
#   1. exact path as given (absolute or relative to cwd)
#   2. inside $SRC_DIR
# This lets you pass either `scripts/upload-designs.sh callisto/ship_templates/foo.json`
# or `scripts/upload-designs.sh foo.json`.
resolve_file() {
  local arg="$1"
  if [[ -f "$arg" ]]; then
    printf '%s' "$arg"
    return 0
  fi
  if [[ -f "$SRC_DIR/$arg" ]]; then
    printf '%s' "$SRC_DIR/$arg"
    return 0
  fi
  return 1
}

if [[ ${#POSITIONAL[@]} -gt 0 ]]; then
  LOCAL_FILES=()
  for arg in "${POSITIONAL[@]}"; do
    if resolved="$(resolve_file "$arg")"; then
      LOCAL_FILES+=("$resolved")
    else
      echo "File not found: $arg (looked in cwd and $SRC_DIR)" >&2
      exit 1
    fi
  done
else
  shopt -s nullglob
  LOCAL_FILES=("$SRC_DIR"/*.json)
  shopt -u nullglob
  if [[ ${#LOCAL_FILES[@]} -eq 0 ]]; then
    echo "No *.json files found in $SRC_DIR." >&2
    exit 1
  fi
fi

# Sanity-check JSON before pushing — a malformed file would land in the bucket
# and the running server would skip it with a parse-error log on next reload.
# Better to catch it here.
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

echo "Source dir:    $SRC_DIR"
echo "Environment:   $TARGET_ENV"
echo "Destination:   $DEST_SUMMARY"
echo
echo "Files to upload (${#LOCAL_FILES[@]}):"
for f in "${LOCAL_FILES[@]}"; do
  printf '  %s\n' "$(basename "$f")"
done
echo

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

# `-m` parallelizes the uploads. Trailing slash on dest is essential so
# gsutil treats it as a directory rather than renaming files. Each destination
# is reported on its own so a partial failure under `--env both` is visible.
EXIT_CODE=0
for ((i = 0; i < ${#DEST_BUCKETS[@]}; i++)); do
  echo "Uploading to ${ENV_LABELS[$i]} — ${DEST_BUCKETS[$i]} …"
  if gsutil -m cp "${LOCAL_FILES[@]}" "${DEST_BUCKETS[$i]}/"; then
    echo "  ${ENV_LABELS[$i]}: uploaded ${#LOCAL_FILES[@]} file(s) to ${DEST_BUCKETS[$i]}"
  else
    echo "  ${ENV_LABELS[$i]}: FAILED to upload to ${DEST_BUCKETS[$i]}" >&2
    EXIT_CODE=1
  fi
  echo
done

echo "Done. Server will pick up changes on its next 5s reload poll."
exit $EXIT_CODE

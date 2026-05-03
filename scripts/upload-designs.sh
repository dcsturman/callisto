#!/usr/bin/env bash
# Upload one or more ship-design JSON files to gs://callisto-ship-templates/.
#
# Usage:
#   scripts/upload-designs.sh                            # upload every *.json in callisto/ship_templates/
#   scripts/upload-designs.sh foo.json bar.json          # upload just those (paths can be absolute, repo-relative, or bare names that resolve in callisto/ship_templates/)
#   scripts/upload-designs.sh --dry-run new_design.json  # show what would be uploaded, don't actually copy
#   scripts/upload-designs.sh --yes new_design.json      # skip the confirmation prompt
#
# The bucket name can be overridden:
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
POSITIONAL=()
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --yes|-y)  SKIP_CONFIRM=1 ;;
    -h|--help)
      sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    -*)
      echo "Unknown flag: $arg" >&2
      exit 2
      ;;
    *)
      POSITIONAL+=("$arg")
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/callisto/ship_templates"
DEST_BUCKET="${DEST_BUCKET:-gs://callisto-ship-templates}"

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

echo "Source dir:    $SRC_DIR"
echo "Destination:   $DEST_BUCKET"
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
  read -r -p "Proceed with upload? [y/N] " reply
  case "$reply" in
    y|Y|yes|YES) ;;
    *) echo "Aborted."; exit 1 ;;
  esac
fi

echo "Uploading…"
# `-m` parallelizes the uploads. Trailing slash on dest is essential so
# gsutil treats it as a directory rather than renaming files.
gsutil -m cp "${LOCAL_FILES[@]}" "$DEST_BUCKET/"
echo
echo "Done. Server will pick up changes on its next 5s reload poll."

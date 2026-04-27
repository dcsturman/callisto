#!/usr/bin/env bash
# Upload all local scenarios in callisto/scenarios/ to gs://callisto-scenarios/.
#
# Usage:
#   scripts/upload-scenarios.sh             # interactive (prompts to confirm)
#   scripts/upload-scenarios.sh --dry-run   # show what would be uploaded
#   scripts/upload-scenarios.sh --yes       # skip the confirmation prompt
#
# Requires gcloud / gsutil auth (e.g. `gcloud auth login` or service account).
# Existing GCS objects with the same name are overwritten; objects only in GCS
# are NOT deleted (use this for "push my edits up", not for full sync).

set -eo pipefail

DRY_RUN=0
SKIP_CONFIRM=0
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=1 ;;
    --yes|-y)  SKIP_CONFIRM=1 ;;
    -h|--help)
      sed -n '2,11p' "$0" | sed 's/^# \{0,1\}//'
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      exit 2
      ;;
  esac
done

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SRC_DIR="$REPO_ROOT/callisto/scenarios"
DEST_BUCKET="gs://callisto-scenarios"

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

echo "Local source:  $SRC_DIR"
echo "Destination:   $DEST_BUCKET"
echo
echo "Files to upload (${#LOCAL_FILES[@]}):"
for f in "${LOCAL_FILES[@]}"; do
  printf '  %s\n' "$(basename "$f")"
done
echo

# Show which files already exist in GCS (will be overwritten) so the user
# knows what's about to change.
echo "Currently in $DEST_BUCKET:"
if ! gsutil ls "$DEST_BUCKET/" 2>&1 | sed 's|^|  |'; then
  echo "  (could not list bucket; gsutil auth issue?)" >&2
fi
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

# `-m` parallelizes the uploads. `-c` (cache control) and `-r` (recursive)
# aren't needed for flat *.json copy. Trailing slash on dest is essential so
# gsutil treats it as a directory rather than renaming files.
echo "Uploading…"
gsutil -m cp "${LOCAL_FILES[@]}" "$DEST_BUCKET/"
echo
echo "Done. Bucket now contains:"
gsutil ls "$DEST_BUCKET/" | sed 's|^|  |'

#!/usr/bin/env bash
# migrate-users-file.sh — one-shot promotion of a Callisto authorized-users
# file from the legacy `Vec<String>` shape to the V1 object shape. Idempotent:
# on an already-V1 file the rewrite is a no-op (same content).
#
# Use this once before deploying Callisto >= 1.0 against a users file that
# was created under 0.x. The server refuses to start on a legacy-shaped file.
#
# Path source (in priority order):
#   --users-file <path>     (preferred)
#   $USERS_FILE
#   first positional arg
#
# Examples:
#   ./migrate-users-file.sh gs://callisto-be-user-profiles/authorized_users.json
#   ./migrate-users-file.sh --users-file ./config/authorized_users.json
#   USERS_FILE=gs://my-bucket/authorized_users.json ./migrate-users-file.sh
#
# Concurrency: on `gs://` paths the upload uses an `if-generation-match`
# precondition so a server (or another writer) racing on the same file
# can't be silently clobbered. On precondition failure the script exits
# non-zero and asks the operator to retry.

set -euo pipefail

USERS_FILE_ARG="${USERS_FILE:-}"

usage() {
  cat >&2 <<EOF
Usage:
  $0 [--users-file <path>] [<path>]

Path can also come from \$USERS_FILE.
Path must start with 'gs://' or be an absolute local path.
EOF
  exit 64
}

while [ "$#" -gt 0 ]; do
  case "$1" in
    --users-file)
      shift
      [ "$#" -gt 0 ] || usage
      USERS_FILE_ARG="$1"
      shift
      ;;
    -h|--help)
      usage
      ;;
    *)
      USERS_FILE_ARG="$1"
      shift
      ;;
  esac
done

if [ -z "$USERS_FILE_ARG" ]; then
  echo "Users file path required (--users-file, \$USERS_FILE, or positional arg)" >&2
  exit 64
fi

case "$USERS_FILE_ARG" in
  */*) ;;
  *)
    echo "Refusing path '$USERS_FILE_ARG' — must contain '/' (use './name.json' for cwd-relative)" >&2
    exit 64
    ;;
esac

# Normalize either a v1 file or a legacy Vec<String> down to a v1 object,
# lowercasing every email along the way.
read -r -d '' JQ_NORMALIZE <<'EOF' || true
def normalize:
  if type == "array" then
    {
      version: 1,
      users: [
        .[] | { email: (. | ascii_downcase), status: "active", registered_at: 0 }
      ]
    }
  elif type == "object" and (.users // null) != null then
    .version = 1
    | .users |= map(.email |= ascii_downcase)
  else
    error("Unrecognized users file shape")
  end;
normalize
EOF

if [[ "$USERS_FILE_ARG" == gs://* ]]; then
  tmp_in="$(mktemp)"
  tmp_out="$(mktemp)"
  trap 'rm -f "$tmp_in" "$tmp_out"' EXIT

  if ! gcloud storage cp "$USERS_FILE_ARG" "$tmp_in" >/dev/null 2>&1; then
    echo "Could not read $USERS_FILE_ARG (does the object exist? are you authenticated?)" >&2
    exit 1
  fi

  generation="$(gcloud storage objects describe "$USERS_FILE_ARG" --format='value(generation)' 2>/dev/null || true)"
  if [ -z "$generation" ]; then
    echo "Could not read object generation for $USERS_FILE_ARG" >&2
    exit 1
  fi

  jq "$JQ_NORMALIZE" < "$tmp_in" > "$tmp_out"

  if cmp -s "$tmp_in" "$tmp_out"; then
    echo "Already V1; no rewrite needed."
    exit 0
  fi

  if gcloud storage cp "$tmp_out" "$USERS_FILE_ARG" --if-generation-match="$generation" >/dev/null 2>&1; then
    echo "OK — $USERS_FILE_ARG migrated to V1 shape (generation was $generation)."
  else
    echo "Upload failed (likely a generation-precondition mismatch from a concurrent writer)." >&2
    echo "Re-run the script to retry." >&2
    exit 1
  fi
else
  tmp_out="$(mktemp "${USERS_FILE_ARG}.XXXXXX")"
  trap 'rm -f "$tmp_out"' EXIT
  if [ ! -f "$USERS_FILE_ARG" ]; then
    echo "File not found: $USERS_FILE_ARG" >&2
    exit 1
  fi
  jq "$JQ_NORMALIZE" < "$USERS_FILE_ARG" > "$tmp_out"
  if cmp -s "$USERS_FILE_ARG" "$tmp_out"; then
    echo "Already V1; no rewrite needed."
    exit 0
  fi
  mv "$tmp_out" "$USERS_FILE_ARG"
  trap - EXIT
  echo "OK — $USERS_FILE_ARG migrated to V1 shape."
fi

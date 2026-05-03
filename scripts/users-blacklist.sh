#!/usr/bin/env bash
# users-blacklist.sh — manage the Callisto authorized-users file (active/
# blacklisted state) from the command line. Supports both local paths and
# `gs://` GCS objects; on GCS we use generation preconditions to avoid
# clobbering concurrent writes from the running server.
#
# Subcommands:
#   add <email>      — add or flip an email to status=blacklisted (idempotent)
#   remove <email>   — flip a blacklisted email back to status=active
#                      (no-op if active; refuses if absent)
#   list [--all|--active|--blacklisted]  — print the current roster
#
# To migrate a legacy `Vec<String>` users file to the V1 shape, use the
# sibling `migrate-users-file.sh` script (one-shot operation, separate
# concern from blacklist management).
#
# Path source (in priority order):
#   --users-file <path>
#   $USERS_FILE
#
# Examples:
#   ./users-blacklist.sh --users-file ./config/authorized_users.json add eve@example.com
#   USERS_FILE=gs://callisto-be-user-profiles/authorized_users.json \
#     ./users-blacklist.sh list --blacklisted

set -euo pipefail

# ---------- arg parsing ----------
USERS_FILE_ARG="${USERS_FILE:-}"
SUBCOMMAND=""
EMAIL=""
LIST_FILTER="all"

usage() {
  cat >&2 <<EOF
Usage:
  $0 [--users-file <path>] add <email>
  $0 [--users-file <path>] remove <email>
  $0 [--users-file <path>] list [--all|--active|--blacklisted]

Path can also come from \$USERS_FILE.
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
    add|remove|list)
      SUBCOMMAND="$1"
      shift
      break
      ;;
    -h|--help)
      usage
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage
      ;;
  esac
done

if [ -z "$SUBCOMMAND" ]; then
  usage
fi

case "$SUBCOMMAND" in
  add|remove)
    [ "$#" -ge 1 ] || { echo "Missing <email>" >&2; usage; }
    EMAIL="$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')"
    shift || true
    ;;
  list)
    if [ "$#" -ge 1 ]; then
      case "$1" in
        --all) LIST_FILTER="all"; shift ;;
        --active) LIST_FILTER="active"; shift ;;
        --blacklisted) LIST_FILTER="blacklisted"; shift ;;
        *) echo "Unknown list filter: $1" >&2; usage ;;
      esac
    fi
    ;;
esac

if [ -z "$USERS_FILE_ARG" ]; then
  echo "Users file path required (--users-file or USERS_FILE)" >&2
  exit 64
fi

# Path safety: require at least one '/' (catches bare-name typos but allows
# relative paths like './config/authorized_users.json' as well as gs:// URIs
# and absolute local paths).
case "$USERS_FILE_ARG" in
  */*) ;;
  *)
    echo "Refusing path '$USERS_FILE_ARG' — must contain '/' (use './name.json' for cwd-relative)" >&2
    exit 64
    ;;
esac

# ---------- jq programs ----------
# Single-quoted heredoc: variables resolve at jq time, not bash time.
# Normalizes either a v1 file or a legacy Vec<String> down to a v1 object,
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
EOF

read -r -d '' JQ_ADD_BLACKLIST <<'EOF' || true
normalize
| (.users | map(.email == $email) | any) as $present
| if $present then
    .users |= map(
      if .email == $email then
        .status = "blacklisted"
        | .blacklisted_at = ($now | tonumber)
      else . end
    )
  else
    .users += [{
      email: $email,
      status: "blacklisted",
      registered_at: 0,
      blacklisted_at: ($now | tonumber)
    }]
  end
EOF

read -r -d '' JQ_REMOVE_BLACKLIST <<'EOF' || true
normalize
| (.users | map(.email == $email) | any) as $present
| if ($present | not) then
    error("Email not present: " + $email)
  else
    .users |= map(
      if .email == $email then
        .status = "active" | del(.blacklisted_at)
      else . end
    )
  end
EOF

read -r -d '' JQ_LIST <<'EOF' || true
normalize
| .users
| if $filter == "active" then map(select(.status == "active"))
  elif $filter == "blacklisted" then map(select(.status == "blacklisted"))
  else .
  end
| .[] | "\(.status)\t\(.email)"
EOF

NOW="$(date -u +%s)"

# ---------- I/O helpers (local vs gs://) ----------
read_file_local() {
  local path="$1"
  if [ -f "$path" ]; then
    cat "$path"
  else
    echo '[]'
  fi
}

write_file_local() {
  local path="$1"
  local contents="$2"
  local tmp
  tmp="$(mktemp "${path}.XXXXXX")"
  printf '%s' "$contents" > "$tmp"
  mv "$tmp" "$path"
}

read_file_gcs() {
  local uri="$1"
  local tmp
  tmp="$(mktemp)"
  if gcloud storage cp "$uri" "$tmp" >/dev/null 2>&1; then
    cat "$tmp"
    rm -f "$tmp"
  else
    rm -f "$tmp"
    echo '[]'
  fi
}

get_generation_gcs() {
  local uri="$1"
  gcloud storage objects describe "$uri" --format='value(generation)' 2>/dev/null || true
}

# Upload with generation precondition. Empty $2 ⇒ object must not exist
# (--if-generation-match=0 in GCS REST conventions).
write_file_gcs() {
  local uri="$1"
  local generation="$2"
  local contents="$3"
  local tmp
  tmp="$(mktemp)"
  printf '%s' "$contents" > "$tmp"
  local precondition
  if [ -z "$generation" ]; then
    precondition="--if-generation-match=0"
  else
    precondition="--if-generation-match=$generation"
  fi
  if gcloud storage cp "$tmp" "$uri" "$precondition" >/dev/null 2>&1; then
    rm -f "$tmp"
    return 0
  else
    rm -f "$tmp"
    return 1
  fi
}

# ---------- subcommand implementations ----------
do_list() {
  local body
  if [[ "$USERS_FILE_ARG" == gs://* ]]; then
    body="$(read_file_gcs "$USERS_FILE_ARG")"
  else
    body="$(read_file_local "$USERS_FILE_ARG")"
  fi
  printf '%s' "$body" \
    | jq -r --arg filter "$LIST_FILTER" "$JQ_NORMALIZE $JQ_LIST"
}

do_mutate() {
  local jq_program="$1"
  local max_attempts=3

  if [[ "$USERS_FILE_ARG" == gs://* ]]; then
    local attempt=1
    while [ "$attempt" -le "$max_attempts" ]; do
      local body
      body="$(read_file_gcs "$USERS_FILE_ARG")"
      local generation
      generation="$(get_generation_gcs "$USERS_FILE_ARG")"

      local new_body
      new_body="$(printf '%s' "$body" \
        | jq --arg email "$EMAIL" --arg now "$NOW" \
            "$JQ_NORMALIZE $jq_program")"

      if write_file_gcs "$USERS_FILE_ARG" "$generation" "$new_body"; then
        echo "OK"
        return 0
      fi
      echo "Precondition failed on attempt $attempt; retrying..." >&2
      attempt=$((attempt + 1))
      sleep 1
    done
    echo "Failed to write after $max_attempts attempts (precondition kept failing)" >&2
    return 1
  else
    local body
    body="$(read_file_local "$USERS_FILE_ARG")"
    local new_body
    new_body="$(printf '%s' "$body" \
      | jq --arg email "$EMAIL" --arg now "$NOW" \
          "$JQ_NORMALIZE $jq_program")"
    write_file_local "$USERS_FILE_ARG" "$new_body"
    echo "OK"
  fi
}

case "$SUBCOMMAND" in
  add)
    do_mutate "$JQ_ADD_BLACKLIST"
    ;;
  remove)
    do_mutate "$JQ_REMOVE_BLACKLIST"
    ;;
  list)
    do_list
    ;;
esac

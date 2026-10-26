#!/usr/bin/env bash
# Who is on a Callisto server right now, and in which game -- check before a
# deploy, since a new backend revision drops every live game.
#
# Environments (-e / --env, default: prod):
#   prod    project callisto-1731280702227
#   canary  project callisto-canary
#
# Usage:
#   scripts/whos-on.sh               # production
#   scripts/whos-on.sh --env canary
#
# It replays the backend's own logs since the server last started: sign-ins,
# joining and leaving games, and disconnects. Logs reach Cloud Logging within
# a few seconds, so someone who connected that recently may not show yet.
# The AI service shows up too, as its service account, crewing its side.
set -eo pipefail

TARGET_ENV=prod
while [[ $# -gt 0 ]]; do
  case "$1" in
    --env=*) TARGET_ENV="${1#--env=}" ;;
    -e|--env) TARGET_ENV="$2"; shift ;;
    -h|--help) sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

case "$TARGET_ENV" in
  prod)   PROJECT=callisto-1731280702227 ;;
  canary) PROJECT=callisto-canary ;;
  *) echo "Unknown environment: $TARGET_ENV (expected prod or canary)" >&2; exit 2 ;;
esac

# When the server last started: the latest Cloud Run instance start.
STARTED=$(gcloud logging read \
  'resource.type="cloud_run_revision" AND resource.labels.service_name="callisto-be" AND textPayload:"Starting new instance"' \
  --project "$PROJECT" --freshness=30d --limit 1 --format='value(timestamp)')
if [[ -z "$STARTED" ]]; then
  echo "Could not find when the $TARGET_ENV server last started (no logs in 30 days?)." >&2
  exit 1
fi

gcloud logging read \
  "resource.type=\"cloud_run_revision\" AND resource.labels.service_name=\"callisto-be\" AND timestamp>=\"$STARTED\" AND (jsonPayload.target=\"SCENARIO\" OR jsonPayload.target=\"LOGIN_ATTEMPT\" OR jsonPayload.target=\"LOGOUT\")" \
  --project "$PROJECT" --order=asc --limit 100000 --format=json |
  python3 -c '
import json, sys, collections
started, env = sys.argv[1], sys.argv[2]
connected = collections.Counter()   # email -> open connections
game = {}                           # email -> the game they are in
for entry in json.load(sys.stdin):
    p = entry.get("jsonPayload", {})
    email, target, action = p.get("email"), p.get("target"), p.get("action", "")
    if target == "LOGIN_ATTEMPT" and p.get("result") in ("Success", "Service"):
        connected[email] += 1
    elif target == "LOGOUT" and email:
        connected[email] = max(0, connected[email] - 1)
        if connected[email] == 0:
            connected.pop(email, None)
            game.pop(email, None)
    elif target == "SCENARIO" and action == "join":
        game[email] = p.get("scenario")
    elif target == "SCENARIO" and action == "exit":
        game.pop(email, None)
print(env + ": server up since " + started[:19].replace("T", " ") + " UTC")
people = sorted(e for e in connected if not e.startswith("callisto-ai@"))
if not people:
    print("Nobody is connected.")
else:
    print(f"{len(people)} connected:")
    for email in people:
        where = game.get(email)
        print(f"  {email}" + (f"  -- in game {where}" if where else "  -- in the lobby"))
ai = sorted({game[e] for e in connected if e.startswith("callisto-ai@") and e in game})
if ai:
    print("AI crewing: " + ", ".join(ai))
' "$STARTED" "$TARGET_ENV"

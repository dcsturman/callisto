#!/usr/bin/env bash
# Runs ON a RunPod pod, started by cloud_train.sh: train, then remove this pod.
#
#   pod_supervise.sh <deadline, seconds since 1970> <run name> <side_ppo.py settings...>
#
# Training stops at the deadline whether it has finished or not. Once it has
# stopped (it leaves done-<run name>), the pod waits for the Mac to copy the
# results home (the Mac leaves fetched-<run name>), or for a grace period, and then removes itself with
# RunPod's own tool and the pod's own key. Removing deletes the pod and its
# disk, which ends all billing; nothing needs wiping. Everything it does is
# logged to supervise.log.
set -u
DEADLINE=$1; NAME=$2; shift 2
GRACE_SECONDS=${GRACE_SECONDS:-2700}
cd /root/callisto/rl-env
log() { echo "$(date -u +%FT%TZ) $*" >> supervise.log; }

# The pod's id and its own key, which RunPod gives every pod.
export $(tr "\0" "\n" < /proc/1/environ | grep -E "^RUNPOD_(API_KEY|POD_ID)=" | xargs)
remove() {
  log "removing pod $RUNPOD_POD_ID: $1"
  until runpodctl remove pod "$RUNPOD_POD_ID" >> supervise.log 2>&1; do sleep 30; done
}

log "training $NAME until $(date -u -d @"$DEADLINE" +%FT%TZ)"
.venv/bin/python scripts/side_ppo.py --name "$NAME" "$@" > "train-$NAME.log" 2>&1 &
TRAIN=$!
while kill -0 "$TRAIN" 2>/dev/null; do
  if [ "$(date +%s)" -ge "$DEADLINE" ]; then
    log "deadline reached: stopping training"
    kill -INT "$TRAIN"; sleep 120; kill -9 "$TRAIN" 2>/dev/null
    break
  fi
  sleep 30
done
wait "$TRAIN" 2>/dev/null; log "training stopped, exit $?"
date +%s > "done-$NAME"

GRACE=$(( $(date +%s) + GRACE_SECONDS ))
until [ -f "fetched-$NAME" ] || [ "$(date +%s)" -ge "$GRACE" ]; do sleep 20; done
remove "$([ -f "fetched-$NAME" ] && echo 'the results are home' || echo 'the grace period ran out')"

#!/bin/zsh
# Run a training on a RunPod pod, unattended, and bring the results home.
#
#   cloud_train.sh <host> <ssh port> <run name> <hours> <side_ppo.py settings...>
#
# The pod must already exist (created with SSH on port 22 and the account's SSH
# key). This script sets it up, copies the checkpoints in UPLOAD over, waits
# for W&B to be signed in on the pod (unless WANDB=0), then starts training
# under pod_supervise.sh with a deadline of <hours> from now. Every ten minutes
# it copies the run's checkpoints, charts and log home. When training stops it
# makes a last copy and tells the pod (the file fetched-<run name>), which then removes itself; if this Mac
# is asleep or offline, the pod removes itself anyway after a grace period.
#
#   UPLOAD   space-separated paths under rl-env/ to copy to the pod first
#            (default: the side-5 and side-20-1 final networks)
#   WANDB    1 (default) to wait for `wandb login` on the pod; 0 to train without W&B
#   KEY      the SSH key to use (default ~/.ssh/id_ed25519, from the keychain)
#   EVERY    seconds between copies home (default 600)
set -u
H=$1; P=$2; NAME=$3; HOURS=$4; shift 4
UPLOAD=${UPLOAD:-checkpoints/side-5/final.pt checkpoints/side-20-1/final.pt}
WANDB=${WANDB:-1}
KEY=${KEY:-$HOME/.ssh/id_ed25519}
EVERY=${EVERY:-600}
HERE=${0:A:h}/..
LOG=$HERE/logs/cloud-$NAME.log
C=$HOME/.ssh/cloud-train-%C
SSH=(ssh -o StrictHostKeyChecking=accept-new -o IdentitiesOnly=yes -o IdentityAgent=none -o UseKeychain=yes -i $KEY
     -o BatchMode=yes -o ConnectTimeout=20 -o ServerAliveInterval=30 -o ControlMaster=auto -o ControlPath=$C -o ControlPersist=4h
     -p $P root@${H})
RSYNC=(rsync -az -e "ssh -o IdentitiesOnly=yes -o IdentityAgent=none -o UseKeychain=yes -i $KEY -o BatchMode=yes -o ControlPath=$C -p $P")
mkdir -p $HERE/logs
say() { echo "$(date '+%H:%M') $*" | tee -a $LOG; }
cd $HERE

say "setting up $H:$P for $NAME"
"${SSH[@]}" true < /dev/null || { say "cannot reach the pod"; exit 1; }
scp -q -o ControlPath=$C -P $P scripts/cloud_setup.sh scripts/pod_supervise.sh "root@${H}:"
"${SSH[@]}" 'bash cloud_setup.sh > setup.log 2>&1; echo $? > setup.done' < /dev/null
if [ "$("${SSH[@]}" cat setup.done < /dev/null)" != 0 ]; then
  say "setup failed; the end of its log:"; "${SSH[@]}" 'tail -20 setup.log' < /dev/null | tee -a $LOG; exit 1
fi
"${SSH[@]}" 'cp pod_supervise.sh callisto/rl-env/scripts/' < /dev/null
for item in ${=UPLOAD}; do
  "${SSH[@]}" "mkdir -p callisto/rl-env/$(dirname $item)" < /dev/null
  "${RSYNC[@]}" "$item" "root@${H}:callisto/rl-env/$item"
done
say "set up; copied ${=UPLOAD}"

if [ "$WANDB" = 1 ]; then
  say "waiting for W&B: in a terminal, run"
  say "  ssh -i $KEY -p $P root@$H"
  say "  cd callisto/rl-env && .venv/bin/wandb login --host https://api.forge.coreweave.com"
  until "${SSH[@]}" 'grep -q "api.forge.coreweave.com" ~/.netrc 2>/dev/null' < /dev/null; do sleep 15; done
  say "W&B is signed in"
fi

DEADLINE=$(( $(date +%s) + HOURS * 3600 ))
# Detached from this connection entirely (its own session, no terminal, no
# stdin), or ssh waits for it to finish.
"${SSH[@]}" "cd callisto/rl-env && { setsid nohup bash scripts/pod_supervise.sh $DEADLINE $NAME ${(q)@} < /dev/null > /dev/null 2>&1 & }" < /dev/null
say "training $NAME, deadline $(date -r $DEADLINE '+%a %H:%M')"

fetch() {
  mkdir -p checkpoints/$NAME runs/$NAME
  "${RSYNC[@]}" "root@${H}:callisto/rl-env/checkpoints/$NAME/" checkpoints/$NAME/ 2>/dev/null
  "${RSYNC[@]}" "root@${H}:callisto/rl-env/runs/$NAME/" runs/$NAME/ 2>/dev/null
  "${RSYNC[@]}" "root@${H}:callisto/rl-env/train-$NAME.log" "root@${H}:callisto/rl-env/supervise.log" logs/ 2>/dev/null
}
misses=0
while true; do
  sleep $EVERY
  if fetch; then misses=0; else misses=$((misses + 1)); fi
  if [ $(( misses * EVERY )) -ge 3600 ]; then say "the pod has not answered for an hour; it removes itself after its grace period"; exit 1; fi
  if "${SSH[@]}" "test -f callisto/rl-env/done-$NAME" < /dev/null 2>/dev/null; then
    fetch && "${SSH[@]}" "touch callisto/rl-env/fetched-$NAME" < /dev/null
    say "training done; results in checkpoints/$NAME and runs/$NAME; the pod is removing itself"
    exit 0
  fi
  say "copied $(ls checkpoints/$NAME 2>/dev/null | wc -l | tr -d ' ') checkpoints so far"
done

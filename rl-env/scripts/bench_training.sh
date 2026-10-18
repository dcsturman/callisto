#!/usr/bin/env bash
# Measure training speed on this machine: a short run with side-5's settings, then the
# decisions a second and where the time went. Run from rl-env/ after cloud_setup.sh,
# with checkpoints/side-4/final.pt and checkpoints/teachers/ in place.
set -euo pipefail
STEPS=${STEPS:-1500000}
THREADS=${THREADS:-$(( $(nproc 2>/dev/null || sysctl -n hw.ncpu) - 2 ))}
NAME=${NAME:-bench-$(hostname -s)}
# The side-4 teacher played on the GPU, batched (pt~:), or by the engine on the cpu (side~:).
TEACHER=${TEACHER:-pt~:checkpoints/side-4/final.pt}
EXTRA=${EXTRA:-}   # more settings, e.g. "--play-device cpu"
# The per-ship selfplay-9 teacher's share, 0 by default: it runs in the engine, one game
# at a time, and dominated the engine's time.
OLD_TEACHER=${OLD_TEACHER:-0}
GAMES=${GAMES:-128}
GROUPS=${GROUPS:-1}   # the games split into this many groups, whose engines and network overlap
INIT=${INIT:-checkpoints/side-4/final.pt}
.venv/bin/python scripts/side_ppo.py --init-from "$INIT" --steps "$STEPS" --games "$GAMES" --groups "$GROUPS" --threads "$THREADS" \
  --teachers "0.35=$TEACHER$( [ "$OLD_TEACHER" != 0 ] && echo ",$OLD_TEACHER=net~:checkpoints/teachers/selfplay-9.cpol")" \
  --baselines-start 0.08 --baselines-end 0.08 --destroy-goals 1.0 --random-goals 0 --firepower-bonus 0.2 \
  --name "$NAME" $EXTRA > "bench-$NAME.log" 2>&1
.venv/bin/python - "$NAME" "$THREADS" <<'PY'
import glob, sys, numpy as np, torch
from tensorboard.backend.event_processing.event_accumulator import EventAccumulator
name, threads = sys.argv[1], sys.argv[2]
ea = EventAccumulator(sorted(glob.glob(f"runs/{name}/events*"))[-1]); ea.Reload()
def med(tag, skip=10):
    v = np.array([x.value for x in ea.Scalars(tag)]); return float(np.median(v[skip:])) if len(v) > skip else float(np.median(v))
print(f"{name}: games {__import__('os').environ.get('GAMES','128')}, groups {__import__('os').environ.get('GROUPS','1')}, device {torch.cuda.get_device_name(0) if torch.cuda.is_available() else 'cpu/mps'}, threads {threads}")
fps = [x for x in ea.Scalars("time/fps")]
print(f"decisions a second, at the end: {fps[-1].value:,.0f}")
for t in ["time/play_net", "time/play_engine", "time/learn", "time/learn_waiting"]:
    print(f"{t:20s} {med(t):6.3f} s a batch")
PY

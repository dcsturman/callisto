#!/usr/bin/env bash
# Set up a rented Ubuntu GPU machine (a RunPod pod or a Lambda instance) for training, from nothing: see docs/gpu_training.md.
# Run on the machine:  bash cloud_setup.sh   (fetch it with curl, or scp it over)
set -euo pipefail

REPO=${REPO:-https://github.com/dcsturman/callisto.git}
BRANCH=${BRANCH:-rl-selfplay}
HOME_DIR=${HOME_DIR:-$HOME}

# On a RunPod pod we are root and there may be no sudo; on Lambda we are ubuntu.
SUDO=$([ "$(id -u)" = 0 ] && echo "" || echo sudo)
$SUDO apt-get update -qq
$SUDO apt-get install -y -qq build-essential pkg-config libssl-dev git tmux rsync python3-venv curl >/dev/null

# Rust, for the engine.
if ! command -v cargo >/dev/null; then
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal
fi
source "$HOME/.cargo/env"

# The code. A private repository needs a token: REPO=https://<token>@github.com/dcsturman/callisto.git
cd "$HOME_DIR"
if [ -d callisto ]; then git -C callisto fetch -q && git -C callisto checkout -q "$BRANCH" && git -C callisto pull -q;
else git clone -q --branch "$BRANCH" "$REPO" callisto; fi
cd callisto/rl-env

# Python: keep the machine's own CUDA PyTorch if it has one (Lambda Stack does).
python3 -m venv --system-site-packages .venv
.venv/bin/pip install -q --upgrade pip maturin
.venv/bin/python -c "import torch" 2>/dev/null || .venv/bin/pip install -q torch
.venv/bin/maturin develop --release -q -E train
cargo build --release -q --bin versus

.venv/bin/python - <<'PY'
import os, torch
print("torch", torch.__version__, "| cuda", torch.cuda.is_available(),
      "|", torch.cuda.get_device_name(0) if torch.cuda.is_available() else "no GPU", "| cpus", os.cpu_count())
PY
echo "Ready. Copy checkpoints over, then: wandb login, and start training in tmux (docs/gpu_training.md)."

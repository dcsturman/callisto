# callisto-rl

The callisto engine as a reinforcement-learning environment, and the scripts
that train on it. The why and the roadmap are in `docs/rl_plan.md`.

- `src/` (Rust): `game.rs` plays one scenario between two sides and scores
  it; `encode.rs` turns what a ship sees into 1,184 numbers and says which
  choices are open to her; `action.rs` turns the network's choices back into
  orders; `batch.rs` steps many games at once on a thread pool; `python.rs`
  is the Python module.
- `python/callisto_rl/`: `ShipVecEnv`, the engine as a Stable-Baselines3
  vectorised environment with one entry per ship.
- `scripts/`: `bench.py` measures throughput, `train.py` trains.

## Setup, once

```
cd rl-env
uv venv --python 3.12 .venv
uv pip install --python .venv/bin/python maturin numpy gymnasium torch stable-baselines3 sb3-contrib tensorboard pytest
```

## Build the module

After any change to the Rust code. Unsetting `CONDA_PREFIX` keeps maturin
from installing into Anaconda instead.

```
env -u CONDA_PREFIX VIRTUAL_ENV=$PWD/.venv .venv/bin/maturin develop --release
```

## Test

```
cargo test --release                          # Rust: random play is legal; close plays the same through the action space
.venv/bin/python -m pytest -q python/tests    # Python layer
```

## Run

```
.venv/bin/python scripts/bench.py --games 40 --threads 10
.venv/bin/python scripts/train.py --opponent close --steps 2000000
.venv/bin/tensorboard --logdir runs
cargo run --release --example trace -- ../callisto/scenarios/Thebus.json 3   # one random game, round by round
```

`--threads` (default 10) is every core training uses, engine and PyTorch
together, so the machine stays usable. Logs go to `runs/`, checkpoints to
`checkpoints/`; both are ignored by git.

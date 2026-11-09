# callisto

Callisto is a vector-based space flight and combat simulator based.  By vector-based this means a ship's motion is based on the physics of acceleration and velocity, with acceleration being the only tool for piloting a craft.  There is no banking or rapid breaking in a vector-based system!
Callisto scenarios can be populated with ships and planets. Ships can be outfitted with various weapons and sensors.  Each ship enters orders and they are all resolved simultaneously.  A ship's computer allows simple humans to effectively pilot based on acceleration vectors in 3D space.  The heart of the computer is a nonlinear equation solver.

The code is structure in two main parts.  The `callisto` directory contains a Rust crate containing the server and all of the game logic.  It supports a well defined JSON API for communicating with the client.  The `fe/callisto` directory contains the frontend.  The frontend is a React app that uses Three.js for rendering.
The `ai-ships` directory holds the AI that crews ships: the trained network, scripted commanders, and the language-model admiral. It runs as its own service beside the backend. `rl-env` trains the network.

## Building

You need Rust (stable), Node 23+ and Docker. [LOCAL_DEV.md](LOCAL_DEV.md) has the one-time setup: Google credentials, the frontend's `.env`, and optional TLS keys.

```bash
# Backend, without TLS (as local dev and CI run it)
cd callisto && cargo build --features ci,no_tls_upgrade

# AI service. The trained network, ai-ships/networks/side.cside, is compiled
# into the binary, so rebuild after replacing it.
cd ai-ships && cargo build --release

# Frontend (type-checks, then bundles)
cd fe/callisto && npm install && npm run build
```

Lint and test before pushing. CI runs clippy pedantic with warnings as errors.

```bash
cd callisto && cargo fmt -- --check && cargo clippy --all-targets --all-features -- -D warnings
cd callisto && cargo nextest r --all --features ci,no_tls_upgrade
cd fe/callisto && npm test && npx eslint .
```

## Running locally with Docker Compose

The backend and the AI service run in Docker. The frontend runs from Vite.

```bash
scripts/dev-up.sh --build   # build and start the backend (be) and AI service (ai)
scripts/dev-up.sh           # start them without rebuilding
scripts/dev-fe.sh           # in a second terminal: the frontend
```

Then open http://localhost:50001.

- `dev-up.sh` layers `compose.yaml` with your local `compose.override.yaml` (git-ignored). The override turns off TLS and reads scenarios from `callisto/scenarios`. Add `--gcs` to read them from the production bucket instead.
- Edits to scenarios and ship designs on disk reload within about 5 seconds, without a rebuild.
- Changes to Rust code, including a new trained network, need `--build`.
- A ship switched to AI is crewed by the `ai` service. `CALLISTO_AI_MODEL` chooses who commands. The default is `admiral:gemini`, which reads the orders given to fleets and squadrons and needs Google credentials. `network` plays the trained network alone with no credentials. `close` is a simple script.

To run without Docker, `scripts/dev-be.sh` runs the backend through cargo. [LOCAL_DEV.md](LOCAL_DEV.md) also covers the GCS buckets, Sentry, and OAuth troubleshooting.

## Running games headless

The `harness` binary in `ai-ships/` plays a scenario with no frontend: a commander on each side, many games, then a score. It runs the backend in-process, so no server needs to be running. A game of 40 rounds takes well under a second with the scripted commanders.

```bash
cd ai-ships
cargo run --release --bin harness -- \
  --scenario "../callisto/scenarios/Fighter Screen.json" \
  --side Blue=network --side Red=network \
  --games 20 --rounds 40
```

`cargo run` rebuilds only what changed and then runs `target/release/harness`. You can build once with `cargo build --release --bin harness` and run the binary directly, but then rebuild yourself after changing the network.

Name sides by team (`Blue`, `Red`, `Green`, `Gold`), or `ship:<name>` for a ship on no team. Any side not named plays `--default`, which is `close` unless set.

| Commander | Who plays |
|---|---|
| `network` | The trained network built into the binary (`ai-ships/networks/side.cside`) |
| `side:<file.cside>` | Any exported network, read at run time. `side~:` samples as in training instead of taking the best choice. Add `#destroy` (or another goal) after the file to play for that goal instead of the scenario's |
| `fleet:<directive.json>` | Squadrons under written orders, each flown by the built-in network |
| `close`, `hold`, `squadron`, `spread`, `charger`, `kiter`, `random`, `idle` | Scripted commanders |
| `gemini`, `claude`, `haiku` (or `gemini:<model>`, ...) | A language model. Needs `gcloud auth application-default login` and `--project` or `GOOGLE_CLOUD_PROJECT` |
| `replay:<log>:<side>` | Plays back a side's orders from a `--log` file |

Useful options:

- `--seed N`: the first seed. Game *n* uses seed N+*n*, so any game replays exactly.
- `--rounds N`: the round limit (default 40). Keelhaven is meant for 60.
- `--outcomes out.jsonl`: each game's winner, ships and hull.
- `--events ev.jsonl`: every round's fire, hits, damage, interceptions and ships lost.
- `--log intents.jsonl`: each side's orders, round by round.
- `--transcript <dir>`: a language model's prompts and replies, one file per game and side.

For example, `--side Blue=side:../rl-env/checkpoints/side-20-9-wide/final.cside --side Red=network` plays an older network against the current one. More on the harness is in [docs/ai_ships_design.md](docs/ai_ships_design.md) §4.7.

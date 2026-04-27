# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Repository layout

Two-part app communicating over a JSON-over-WebSocket protocol:

- `callisto/` — Rust crate. Both the binary (`src/main.rs`) and library (`src/lib.rs`) live here. All game logic, physics, and the WS server.
- `fe/callisto/` — Vite + React 19 frontend using Three.js via `@react-three/fiber`. Redux Toolkit for state.
- `compose.yaml` + `compose.override.yaml` — local Docker dev. The override sets `TLS_UPGRADE=0` so the backend runs without TLS for local dev.
- `math/BangBang.ipynb` — derivation notebook for the bang-bang flight-path solver used by `computer.rs`.

## Common commands

### Backend (run from `callisto/`)

- Build / run locally without TLS (matches `compose.override.yaml` and CI): `cargo build --features ci,no_tls_upgrade`
- Lint (must pass; CI uses clippy pedantic with `-D warnings`): `cargo fmt -- --check` and `cargo clippy --all-targets --all-features -- -D warnings`
- Unit + integration tests: `cargo test --release` (or `cargo nextest r --all --features ci,no_tls_upgrade` to match CI)
- Run a single test: `cargo test --release <test_name>` — integration tests live in `tests/webserver.rs`; unit tests live next to their modules and are aggregated under `src/unit_tests.rs`
- Coverage: `./run_coverage.sh` (uses `cargo llvm-cov`)
- Integration tests require TLS certs. Generate them once: `cd callisto/keys && bash ../build_keys.sh` (accept defaults). Or run with `--features no_tls_upgrade` to skip TLS entirely.

### Frontend (run from `fe/callisto/`)

- Dev server: `npm start` (alias for `vite`). `LOCAL_DEV.md` shows the full incantation: `export VITE_CALLISTO_BACKEND=http://localhost:30000 && export VITE_NODE_SERVER=http://localhost:50001 && npm start -- --port 50001`
- Build (typechecks first): `npm run build` (= `tsc && vite build`)
- Tests: `npm test` (Vitest)
- Lint: `npx eslint .`

### Full stack via Docker

```
docker compose -f compose.yaml -f compose.override.yaml up be
```

Then start the frontend separately as above. The override mounts `./callisto/scenarios` into the container so scenario JSON edits hot-reload without rebuilding.

## Backend architecture

Entry flow: `main.rs` binds a `TcpListener`, optionally upgrades each connection to TLS (gated by the `no_tls_upgrade` feature), then to a WebSocket via `tokio_tungstenite::accept_hdr_async`. A `HeaderCallback` extracts the session-key cookie during the handshake. Established sockets are forwarded over an mpsc channel to a single long-running `Processor` task.

Core types and where they live:

- `processor::Processor` — owns the WS connection receiver, the auth template, the live `servers` map, and the `ServerMembersTable`. Multiplexes incoming WS frames against connection events and reload notifications using `futures::select`.
- `server::Server` — one per active scenario instance. Holds `Mutex<Entities>` (current state) plus `initial_scenario` (immutable copy used for `Reset`). Server IDs are random and shared between players in the same scenario.
- `entity::Entities` — the per-scenario world: ships, missiles, planets, the queued `ShipActionList` for the next turn, and `MetaData`. Ship/planet/missile values are wrapped `Arc<RwLock<_>>` because actions, combat, and updates need fine-grained borrows.
- `player::PlayerManager` — per-connection wrapper holding a `Weak` reference to its `Server` so dropping a player doesn't keep the scenario alive.
- `payloads.rs` — the JSON wire types. `RequestMsg` (client → server) and `ResponseMsg` (server → client) are the entry points; everything else is reachable from those enums.
- `combat.rs`, `computer.rs`, `crew.rs`, `missile.rs`, `planet.rs`, `ship.rs`, `action.rs`, `rules_tables.rs` — game logic. `computer.rs` solves flight paths via the `gomez` nonlinear solver.
- `authentication.rs` — `Authenticator` trait with `GoogleAuthenticator` (OAuth, validates JWTs against fetched Google public keys) and `MockAuthenticator` (used when `--test` is set).

Global, hot-reloadable state lives in `OnceCell<RwLock<Arc<...>>>`s:

- Scenarios: `lib.rs::SCENARIOS` — populated from `--scenario-dir` (local path or `gs://` GCS bucket).
- Ship templates: `ship::SHIP_TEMPLATES` (similar pattern) — populated from `--design-file`.

A background task (`watch_reloadable_data`) polls fingerprints every 5s and pushes `ReloadNotification::{Scenarios,ShipTemplates}` into the processor when the source changes. This is why edits to files under `callisto/scenarios/` and `callisto/ship_templates/` take effect without restart.

Test mode (`--test`) does three things: swaps in `MockAuthenticator`, seeds RNGs deterministically (`SmallRng::seed_from_u64`), and installs a panic hook that exits cleanly on the magic string "Time to exit" so integration tests can shut the server down.

Logging uses `tracing` with `tracing-stackdriver` so structured fields land in GCP. The constants `LOG_FILE_USE`, `LOG_AUTH_RESULT`, `LOGOUT`, `LOG_SCENARIO_ACTIVITY` are the `target` values used by `event!` calls — search by these to find every log of a given category.

## Frontend architecture

- `src/App.tsx` — top-level router-by-state: shows `Authentication`, `ScenarioManager`, or the in-game `Simulator` based on `socketReady`/`authenticated`/`joinedScenario` flags from Redux. 3D components are `lazy()`-imported to keep the initial bundle small.
- `src/lib/serverManager.ts` — single source of truth for the WebSocket. Builds and dispatches every outgoing request, parses every `ResponseMsg`, and updates Redux slices. Adds `ws://` for http backends, `wss://` for https. The string-form constants like `'"DesignTemplateRequest"'` match Rust enum-as-string serialization.
- `src/state/` — Redux Toolkit slices: `serverSlice` (entities, templates, auth, socket-ready), `userSlice` (email, joined scenario, role+ship), `uiSlice` (events, proposed plan, results), `actionsSlice` (queued ship actions), `tutorialSlice` (tutorial step + `AppMode`). Persisted to `sessionStorage` via `redux-persist`, with `server` blacklisted (always re-fetched).
- `src/components/space/` — Three.js scene (Spaceview, Ships, Effects). `src/components/controls/` — HUD/UI. `src/components/scenarios/` — auth and scenario-selection screens.
- Path aliases configured in `vite.config.ts`: `components/`, `lib/`, `state/`, `assets/`, `@/`. Use these instead of long relative paths.

## Code style

- Rust: `rustfmt.toml` enforces 2-space indent, max width 120, compressed fn-param layout. Clippy pedantic is on with `-D warnings` in CI; `similar_names` is allowed.
- TypeScript: ESLint flat config with `typescript-eslint` recommended + React recommended. `react/no-unknown-property` is off because @react-three/fiber relies on unknown props.

## Things to know

- Scenarios are JSON files under `callisto/scenarios/` and load by filename. Editing one in place reloads it server-side within ~5s.
- `--scenario-dir` can be a GCS path (`gs://callisto-scenarios`); the code paths in `lib.rs` (`list_local_or_cloud_dir`, `read_local_or_cloud_file`, `get_local_or_cloud_dir_fingerprint`) transparently handle both. Cloud mode requires `gcloud auth application-default login` or `GOOGLE_APPLICATION_CREDENTIALS`.
- Game-mechanics deviations from Mongoose Traveller (no dogfighting, missile differences, no planetary gravity, etc.) are documented in `callisto/FAQ.md`. Read it before changing combat or flight-path behavior.
- `LOCAL_DEV.md` has the most accurate local-dev recipe and OAuth troubleshooting tips.

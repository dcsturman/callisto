# Local Development Setup

The repo supports three ways to run Callisto: native (cargo + npm in two terminals),
docker-compose, and cloud (via GitHub Actions). The first two are documented here.
Helper scripts live in `scripts/` and wrap the long command lines.

## One-time setup

1. Install Rust (stable), Node 23+, Docker, and the `gcloud` CLI.
2. Authenticate with Google for GCS access:
   ```bash
   gcloud auth application-default login
   ```
   This writes `~/.config/gcloud/application_default_credentials.json`, which is
   what both `scripts/dev-be.sh --gcs` and `compose.yaml` look for.
3. Make sure `fe/callisto/.env` exists with `VITE_GOOGLE_OAUTH_CLIENT_ID` filled
   in. Use `.env.example` as a template. The OAuth client must allow
   `http://localhost:50001` as a redirect URI.
4. (Only if you plan to run integration tests with TLS) generate dev certs:
   ```bash
   cd callisto/keys && bash ../build_keys.sh
   ```
   For everyday dev the scripts use `--features no_tls_upgrade`, so this is optional.

## Option 1 — Two terminals (native)

Terminal A:
```bash
scripts/dev-be.sh           # local scenario files (callisto/scenarios)
# or
scripts/dev-be.sh --gcs     # read scenarios from gs://callisto-scenarios
```

Terminal B:
```bash
scripts/dev-fe.sh
```

Open http://localhost:50001.

`dev-be.sh` builds with `--features no_tls_upgrade` so the frontend (which uses
`ws://`, not `wss://`, when `VITE_CALLISTO_BACKEND` is `http://...`) can connect
without messing with self-signed certs. Extra args are forwarded to `cargo run`,
so `scripts/dev-be.sh -- --port 31000` works.

## Option 2 — Docker compose

```bash
scripts/dev-up.sh             # local scenario files
scripts/dev-up.sh --gcs       # GCS scenarios (gs://callisto-scenarios)
scripts/dev-up.sh --gcs --build
```

Then start the frontend the same way as option 1:
```bash
scripts/dev-fe.sh
```

`dev-up.sh` layers compose files: `compose.yaml` is the base, your local
`compose.override.yaml` (gitignored, optional) is auto-included, and
`compose.gcs.yaml` is added when `--gcs` is passed.

## Sentry

Both the Rust backend and React frontend report errors to Sentry org `self-vt0`
(projects `callisto` and `callisto-fe`). DSNs are baked into the dev/CI paths so
nothing has to be exported by hand:

- Backend: `scripts/dev-be.sh` exports `SENTRY_DSN` and `SENTRY_ENVIRONMENT=development`.
  `compose.yaml` sets the same vars for docker runs (`SENTRY_ENVIRONMENT=docker-local`).
  `.github/workflows/{canary,prod}-be-merge.yml` pass them to Cloud Run via
  `--update-env-vars`. `--test` mode skips Sentry init entirely.
- Frontend: `fe/callisto/.env` provides `VITE_SENTRY_DSN` and
  `VITE_SENTRY_ENVIRONMENT`. The FE deploy workflows override the environment
  to `canary`/`production` at build time.
- Source-map upload runs only when `SENTRY_AUTH_TOKEN` is set (vite.config.ts
  conditionally includes the plugin). For canary/prod source maps to de-minify
  in the Sentry UI, add `SENTRY_AUTH_TOKEN` (org-level token, scope
  `project:releases`) to the GitHub Actions repo secrets.

## Scenario builder note

The scenario builder UI can create and edit scenarios in memory, but the save
path (writing back to local disk or GCS) is **not yet implemented** — there is
no `SaveScenario` payload or write-side helper in `callisto/src/lib.rs`. Use
`--gcs` if you need the cloud scenario library visible in the picker; you don't
need it just to iterate on the builder UI.

## Troubleshooting

**WebSocket handshake fails:**
- `VITE_CALLISTO_BACKEND` should be `http://localhost:30000` (not `https://`)
  unless you really want TLS — in which case the backend must be built without
  `no_tls_upgrade` and the browser must trust your dev cert.

**OAuth `redirect_uri_mismatch`:**
- `VITE_NODE_SERVER` must be `http://localhost:50001`.
- The Google OAuth client must list `http://localhost:50001` under both
  authorized redirect URIs and authorized JavaScript origins.
- The Vite dev server has to be on port 50001 — `dev-fe.sh` does this.

**OAuth `missing access_token`:**
- Usually a redirect URI mismatch. Frontend and backend `--web-server` must
  agree on the URL.

**GCS errors when using `--gcs`:**
- Run `gcloud auth application-default login`. Confirm with
  `gsutil ls gs://callisto-scenarios`.

**Integration tests fail with "Connection refused":**
- Run `cd callisto/keys && bash ../build_keys.sh` to (re)generate TLS keys.

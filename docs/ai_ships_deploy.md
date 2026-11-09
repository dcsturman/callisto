# Deploying the AI ships service

The AI service runs as a second Cloud Run service, `callisto-ai`, beside the
backend `callisto-be`. `.github/workflows/canary-ai-merge.yml` builds and
deploys it on every push to `canary` that touches `ai-ships/` or the backend
crate it links. The backend's own workflow tells it where the service is.

Nothing here happens until the epic is merged to `canary`. The steps below are
one-time setup for the `callisto-canary` project; prod is the same with its own
project and URLs (see Production below).

## How the two services find and trust each other

| | Backend `callisto-be` | AI service `callisto-ai` |
|---|---|---|
| URL | `https://callisto-be-niqkqove5a-uc.a.run.app` | `https://callisto-ai-niqkqove5a-uc.a.run.app` |
| Runs as | `325308079155-compute@developer.gserviceaccount.com` | `callisto-ai@callisto-canary.iam.gserviceaccount.com` |
| Calls the other with | an identity token for the AI service's URL | an identity token for the backend's URL, in `ServiceLogin` |
| Checks the caller by | `CALLISTO_SERVICE_AUDIENCE` and the authorized-users file | Cloud Run IAM, then `CALLISTO_AI_AUDIENCE` and `CALLISTO_AI_ALLOWED_CALLERS` |

The AI service also calls Vertex AI (Gemini) as itself, billed to
`callisto-canary`.

## One-time setup for canary

Run these once, as a project owner. None of them is run by the workflow.

```sh
P=callisto-canary
AI=callisto-ai@$P.iam.gserviceaccount.com
BE=325308079155-compute@developer.gserviceaccount.com

# Vertex AI, for the model calls.
gcloud services enable aiplatform.googleapis.com --project $P

# The service's own identity, allowed to call models.
gcloud iam service-accounts create callisto-ai --project $P \
  --display-name "Callisto AI ships"
gcloud projects add-iam-policy-binding $P \
  --member serviceAccount:$AI --role roles/aiplatform.user

# The GitHub deployer may deploy a service that runs as it. The deployer is the
# WIF_CANARY_SERVICE_ACCOUNT secret's account.
gcloud iam service-accounts add-iam-policy-binding $AI --project $P \
  --member serviceAccount:<WIF_CANARY_SERVICE_ACCOUNT> \
  --role roles/iam.serviceAccountUser
```

After the first deploy has created the service:

```sh
# Only the backend may call it.
gcloud run services add-iam-policy-binding callisto-ai \
  --region us-central1 --project $P \
  --member serviceAccount:$BE --role roles/run.invoker
```

And the AI service has to be allowed to log in to the backend: add it to
`gs://callisto-canary-user-profiles/authorized_users.json` as an active user,
alongside the people:

```json
{ "email": "callisto-ai@callisto-canary.iam.gserviceaccount.com", "status": "Active", "registered_at": 0 }
```

## Production

The same, in `callisto-1731280702227`, deployed by
`.github/workflows/prod-ai-merge.yml` on every push to `main`:

| | Backend `callisto-be` | AI service `callisto-ai` |
|---|---|---|
| URL | `https://callisto-be-ul7hz2fcvq-uc.a.run.app` | `https://callisto-ai-ul7hz2fcvq-uc.a.run.app` |
| Runs as | `402344016908-compute@developer.gserviceaccount.com` | `callisto-ai@callisto-1731280702227.iam.gserviceaccount.com` |

The GitHub deployer is `github-action-786636640@`. Setup, done 2026-10-25:
the `callisto-ai` service account and the deployer's
`roles/iam.serviceAccountUser` on it; after the first deploy, the
backend's `roles/run.invoker` on the service and the AI service's entry in
`gs://callisto-be-user-profiles/authorized_users.json`. The service plays
with the built-in network, so it needs no Vertex AI.

## Running costs

The service scales to zero (`--min-instances=0`, CPU throttled, as Cloud Run
does by default) and costs nothing while no game has AI ships. A crew is not
a request: it is a WebSocket the service opened to the backend and a loop
waiting on it, and Cloud Run gives an instance CPU, and keeps it, only while
a request is open. So the backend supplies the requests: for each AI side it
holds a WebSocket open to the service's `/crew`, carrying the side's ship
list, and closes it to release the side. While any side is held the instance
runs and bills; when none is, it stops.

A request lasts an hour at most (`--timeout=3600`). When one is cut, or a
redeploy drops it, the backend opens the side again and the service carries
on with the crew it was running; a crew whose connection is not back within
90 seconds is released. If the instance itself was stopped, the side is
crewed afresh when the backend reconnects.

Until 2026-11-05 the service ran with `--min-instances=1
--no-cpu-throttling`, an always-on instance in each of canary and
production, about $45-50 a month apiece whether or not anyone played.

Model calls are on top, at a few cents per game.

## Locally

`docker-compose up --build -d` runs the backend, the AI service and the
frontend (the override file is picked up by itself),
with the shared secret instead of identity tokens and your
application-default credentials for Vertex AI (run `gcloud auth
application-default login` first if needed). `CALLISTO_AI_MODEL` and
`GOOGLE_CLOUD_PROJECT` can be overridden in the environment; the defaults are
`gemini` and `callisto-1731280702227`.

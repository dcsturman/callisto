# Deploying the AI ships service

The AI service runs as a second Cloud Run service, `callisto-ai`, beside the
backend `callisto-be`. `.github/workflows/canary-ai-merge.yml` builds and
deploys it on every push to `canary` that touches `ai-ships/` or the backend
crate it links. The backend's own workflow tells it where the service is.

Nothing here happens until the epic is merged to `canary`. The steps below are
one-time setup for the `callisto-canary` project; prod is the same with its own
project and URLs, and is not set up yet.

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

## Running costs

The service is pinned at one instance with CPU always allocated
(`--min-instances=1 --no-cpu-throttling`). A crew is a WebSocket the service
opened to the backend and a loop waiting on it, not a request, so on Cloud
Run's default settings it would be throttled or stopped between the backend's
calls. That makes it an always-on instance: a few tens of dollars a month at
the smallest size. Model calls are on top, at a few cents per game.

If that is too much while canary is quiet, `--min-instances=0` lets it stop;
the backend calls `/crew` again whenever a ship becomes AI-crewed, but a crew
whose instance stopped mid-game is not restarted until something changes.

## Locally

`docker compose -f compose.yaml -f compose.override.yaml up be ai` runs both,
with the shared secret instead of identity tokens and your
application-default credentials for Vertex AI (run `gcloud auth
application-default login` first if needed). `CALLISTO_AI_MODEL` and
`GOOGLE_CLOUD_PROJECT` can be overridden in the environment; the defaults are
`gemini` and `callisto-1731280702227`.

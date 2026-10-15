//! The HTTP endpoints the backend calls, and who may call them.
//!
//! * `POST /crew {scenario_id, side, team?, ships}` -- crew this side.
//! * `POST /release {scenario_id, side}` -- stop.
//! * `GET /status` -- what is being crewed.
//! * `GET /healthz` -- for Cloud Run.
//!
//! `/crew`, `/release` and `/status` need a bearer token: the shared secret
//! locally, or on Cloud Run a Google identity token naming this service as
//! audience from an allowed account -- the backend's. The same check the
//! backend applies to us, run the other way.

use std::sync::Arc;

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use callisto::authentication::UserDirectory;
use callisto::service_auth::ServiceAuth;
use serde::Deserialize;
use tracing::{info, warn};

use crate::crews::{CrewStatus, Crews};

/// Who may call this service.
pub struct Callers {
  pub auth: ServiceAuth,
  /// Accounts allowed to call, for identity tokens.
  pub allowed: UserDirectory,
}

impl Callers {
  fn admit(&self, headers: &HeaderMap) -> Result<String, StatusCode> {
    let token = headers
      .get("authorization")
      .and_then(|value| value.to_str().ok())
      .and_then(|value| value.strip_prefix("Bearer "))
      .ok_or(StatusCode::UNAUTHORIZED)?;
    self.auth.verify(token, &self.allowed).map_err(|e| {
      warn!("(api) Refused a caller: {e}");
      StatusCode::UNAUTHORIZED
    })
  }
}

pub struct App {
  pub crews: Crews,
  pub callers: Callers,
}

#[derive(Debug, Deserialize)]
pub struct CrewRequest {
  pub scenario_id: String,
  pub side: String,
  pub ships: Vec<String>,
}

#[derive(Debug, Deserialize)]
pub struct ReleaseRequest {
  pub scenario_id: String,
  pub side: String,
}

async fn crew(State(app): State<Arc<App>>, headers: HeaderMap, Json(request): Json<CrewRequest>) -> StatusCode {
  match app.callers.admit(&headers) {
    Ok(caller) => {
      info!(
        "(api) {caller} asks us to crew {}/{} with {:?}.",
        request.scenario_id, request.side, request.ships
      );
      app.crews.crew(&request.scenario_id, &request.side, request.ships);
      StatusCode::NO_CONTENT
    }
    Err(status) => status,
  }
}

async fn release(State(app): State<Arc<App>>, headers: HeaderMap, Json(request): Json<ReleaseRequest>) -> StatusCode {
  match app.callers.admit(&headers) {
    Ok(caller) => {
      info!("(api) {caller} releases {}/{}.", request.scenario_id, request.side);
      app.crews.release(&request.scenario_id, &request.side);
      StatusCode::NO_CONTENT
    }
    Err(status) => status,
  }
}

async fn status(State(app): State<Arc<App>>, headers: HeaderMap) -> Result<Json<Vec<CrewStatus>>, StatusCode> {
  app.callers.admit(&headers)?;
  Ok(Json(app.crews.status()))
}

async fn healthz() -> &'static str {
  "ok"
}

pub fn router(app: Arc<App>) -> Router {
  Router::new()
    .route("/crew", post(crew))
    .route("/release", post(release))
    .route("/status", get(status))
    .route("/healthz", get(healthz))
    .with_state(app)
}

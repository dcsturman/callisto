//! The endpoints the backend calls, and who may call them.
//!
//! * `/crew` -- a WebSocket the backend holds open for one side. Each message
//!   on it is the whole side, `{scenario_id, side, team?, ships}`: the first
//!   starts the crew, later ones change its ships. The backend closing it
//!   releases the side. When the crew ends by itself we close it, with
//!   [`CREW_ENDED`] as the reason, and the backend leaves the side alone until
//!   something about it changes.
//!   The connection also carries the crew's game, as binary frames, one
//!   message each; the backend relays them to itself. An empty binary frame
//!   from the backend says the game closed the crew's connection.
//! * `GET /status` -- what is being crewed.
//! * `GET /healthz` -- for Cloud Run.
//!
//! **Why a held connection.** Cloud Run keeps an instance, and gives it CPU,
//! only while a request is open. The backend's held connections are the
//! requests that keep the service running while there are sides to crew;
//! with none open it scales to zero. They also carry the crews' games, so a
//! crew plays on the backend instance that holds its game. A connection that
//! drops without being closed -- a redeploy, Cloud Run's hour-long limit on a
//! request -- leaves its crew waiting for [`DROPPED_GRACE`], long enough for
//! the backend to hold the side again.
//!
//! `/crew` and `/status` need a bearer token: the shared secret locally, or
//! on Cloud Run a Google identity token naming this service as audience from
//! an allowed account -- the backend's. The same check the backend applies to
//! us, run the other way.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{close_code, CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
pub use callisto::ai_service::CREW_ENDED;
use callisto::authentication::UserDirectory;
use callisto::service_auth::ServiceAuth;
use serde::Deserialize;
use tokio::sync::watch;
use tracing::{info, warn};

use crate::crew::{self, Inbound, TunnelEnds};
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

/// How long a side whose connection dropped is crewed without one, for the
/// backend to hold it again.
pub const DROPPED_GRACE: Duration = Duration::from_secs(90);

async fn crew(State(app): State<Arc<App>>, headers: HeaderMap, upgrade: WebSocketUpgrade) -> Response {
  match app.callers.admit(&headers) {
    Ok(caller) => upgrade.on_upgrade(move |socket| hold(app, caller, socket)),
    Err(status) => status.into_response(),
  }
}

/// How a side's connection ended.
enum Hold {
  /// The backend closed it: let the side go.
  Released,
  /// It dropped: the backend will be back.
  Dropped,
  /// The crew finished by itself.
  CrewEnded,
}

/// Becomes ready when the crew finishes; never, before there is one.
async fn finished(done: &mut Option<watch::Receiver<bool>>) {
  match done {
    Some(done) => {
      let _ = done.wait_for(|finished| *finished).await;
    }
    None => std::future::pending().await,
  }
}

/// The next message a crew sends its game, or never, once it has stopped.
async fn from_crew(ends: &mut Option<TunnelEnds>) -> Option<String> {
  match ends {
    Some(ends) => ends.outbound.recv().await,
    None => std::future::pending().await,
  }
}

/// Crew the side this connection states, for as long as it is held. The
/// connection carries the crew's game too: its binary frames to and from the
/// crew.
async fn hold(app: Arc<App>, caller: String, mut socket: WebSocket) {
  let mut side: Option<(String, String)> = None;
  let mut done = None;
  // The crew's end goes with the first request; ours stays here.
  let (tunnel, ends) = crew::tunnel();
  let (mut tunnel, mut ends) = (Some(tunnel), Some(ends));
  let ending = loop {
    tokio::select! {
      outbound = from_crew(&mut ends) => match outbound {
        Some(text) => {
          if socket.send(Message::Binary(text.into_bytes().into())).await.is_err() {
            break Hold::Dropped;
          }
        }
        // The crew let this connection's game go; the side is still held.
        None => ends = None,
      },
      message = socket.recv() => match message {
        Some(Ok(Message::Binary(bytes))) => {
          if let Some(ends) = &ends {
            let inbound = if bytes.is_empty() {
              Inbound::Closed
            } else {
              Inbound::Text(String::from_utf8_lossy(&bytes).into_owned())
            };
            let _ = ends.inbound.send(inbound);
          }
        }
        Some(Ok(Message::Text(text))) => match serde_json::from_str::<CrewRequest>(&text) {
          Ok(request) => {
            let key = (request.scenario_id.clone(), request.side.clone());
            if side.as_ref() != Some(&key) {
              if let Some((scenario_id, side)) = &side {
                app.crews.unhold(scenario_id, side);
              }
              app.crews.hold(&key.0, &key.1);
              side = Some(key);
            }
            info!("(api) {caller} asks us to crew {}/{} with {:?}.", request.scenario_id, request.side, request.ships);
            done = Some(app.crews.crew(&request.scenario_id, &request.side, request.ships, tunnel.take()));
          }
          Err(e) => warn!("(api) Could not read a side from {caller}: {e}"),
        },
        Some(Ok(Message::Close(_))) => break Hold::Released,
        Some(Ok(_)) => {}
        Some(Err(_)) | None => break Hold::Dropped,
      },
      () = finished(&mut done) => {
        let close = CloseFrame { code: close_code::NORMAL, reason: CREW_ENDED.into() };
        let _ = socket.send(Message::Close(Some(close))).await;
        break Hold::CrewEnded;
      }
    }
  };
  let Some((scenario_id, side)) = side else {
    return;
  };
  if app.crews.unhold(&scenario_id, &side) {
    return;
  }
  match ending {
    Hold::Released => {
      info!("(api) {caller} releases {scenario_id}/{side}.");
      app.crews.release(&scenario_id, &side);
    }
    Hold::Dropped => {
      warn!("(api) Lost {caller}'s connection for {scenario_id}/{side}; crewing on for {DROPPED_GRACE:?}.");
      tokio::spawn(async move {
        tokio::time::sleep(DROPPED_GRACE).await;
        if !app.crews.held(&scenario_id, &side) {
          info!("(api) Nobody held {scenario_id}/{side} again; releasing it.");
          app.crews.release(&scenario_id, &side);
        }
      });
    }
    Hold::CrewEnded => {}
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
    .route("/crew", get(crew))
    .route("/status", get(status))
    .route("/healthz", get(healthz))
    .with_state(app)
}

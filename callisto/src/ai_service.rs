//! Telling the AI service which ships to crew.
//!
//! The backend does not decide anything for an AI ship; the AI service does,
//! connecting as a player. What the backend does is notice when ships become
//! AI-controlled -- from a scenario file, or because someone pressed "Switch
//! to AI" -- and ask the service to crew them, and notice when they stop.
//!
//! Ships are crewed by **side**: every AI ship on one team is one side, and
//! the service makes one decision for the side each round. An AI ship on no
//! team is a side of its own, since unaligned ships are not allies.
//!
//! The backend keeps what it last told the service for each scenario and,
//! after every request, works out what it should say now. Each side is a
//! WebSocket held open to the service's `/crew` (see [`AiServiceClient`]): a
//! side whose ships changed is sent its full ship list; a side that is gone
//! has its connection closed. Because every message states the whole side, a
//! repeated one is harmless, and a connection opened again after a drop
//! restates the side in full.
//!
//! The connections run on their own tasks and never hold up the game. If the
//! service is down the failure is logged, the connection is retried, and the
//! ships sit there meanwhile, as a ship with no crew does.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;

use crate::entity::{Entities, Entity};
use crate::ship::{Controller, Team};
use crate::{info, warn};

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const METADATA_IDENTITY_URL: &str =
  "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity";

/// The AI ships of one scenario that share a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSide {
  pub team: Option<Team>,
  pub ships: BTreeSet<String>,
}

/// A scenario's sides, keyed by a stable id: the team's name, or `ship:` and
/// the ship's name for an unaligned one.
pub type Sides = BTreeMap<String, AiSide>;

/// Which ships in this world the AI should be crewing, and on which sides.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn sides_of(entities: &Entities) -> Sides {
  let mut sides = Sides::new();
  for ship in entities.ships.values() {
    let ship = ship.read().unwrap();
    if ship.controller != Controller::Ai {
      continue;
    }
    let side = ship
      .team
      .map_or_else(|| format!("ship:{}", ship.get_name()), |team| format!("{team:?}"));
    sides
      .entry(side)
      .or_insert_with(|| AiSide {
        team: ship.team,
        ships: BTreeSet::new(),
      })
      .ships
      .insert(ship.get_name().to_string());
  }
  sides
}

/// One thing to tell the AI service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum AiCall {
  /// Crew these ships, as one side: the whole side, not a change to it.
  Crew {
    scenario_id: String,
    side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    team: Option<Team>,
    ships: Vec<String>,
  },
  /// Stop crewing this side.
  Release { scenario_id: String, side: String },
}

/// What to tell the service to get from what it was last told to what is
/// true now.
#[must_use]
pub fn plan_calls(scenario_id: &str, before: &Sides, after: &Sides) -> Vec<AiCall> {
  let crew = after
    .iter()
    .filter(|(side, now)| before.get(*side) != Some(*now))
    .map(|(side, now)| AiCall::Crew {
      scenario_id: scenario_id.to_string(),
      side: side.clone(),
      team: now.team,
      ships: now.ships.iter().cloned().collect(),
    });
  let release = before
    .keys()
    .filter(|side| !after.contains_key(*side))
    .map(|side| AiCall::Release {
      scenario_id: scenario_id.to_string(),
      side: side.clone(),
    });
  crew.chain(release).collect()
}

/// How the AI service says a side's crew has finished by itself: the
/// backend it plays on closed on it, or the game is gone. Not worth holding
/// the side open again until something about it changes.
pub const CREW_ENDED: &str = "crew ended";

/// Keepalive on a held side, and how a dead connection is noticed.
const HOLD_PING: Duration = Duration::from_secs(30);
/// Wait before retrying a side that could not be held; doubled up to
/// `MAX_RETRY`.
const FIRST_RETRY: Duration = Duration::from_secs(1);
const MAX_RETRY: Duration = Duration::from_secs(30);

/// A side the service is crewing for us: the connection held open for it,
/// and what it was last told.
struct Held {
  side: watch::Sender<AiCall>,
  task: JoinHandle<()>,
}

/// Where the AI service is, and how to prove to it that we are the backend.
///
/// Each AI side is a WebSocket held open to the service's `/crew`, carrying
/// the side's whole ship list when it opens and again whenever it changes;
/// closing it releases the side. The service runs on Cloud Run, which keeps
/// an instance, and gives it CPU, only while a request is open: the held
/// connections are those requests, so the service runs while there are AI
/// sides to crew and scales to zero when there are none. A connection that
/// drops (a redeploy, Cloud Run's hour-long request limit) is opened again;
/// one the service closes with [`CREW_ENDED`] is not.
#[derive(Clone)]
pub struct AiServiceClient {
  /// The service's `https://` or `http://` URL, which is also the audience
  /// its identity tokens name.
  url: String,
  /// The shared secret, for local Compose. Without it we ask the Cloud Run
  /// metadata server for an identity token naming the service as audience.
  shared_secret: Option<String>,
  http: reqwest::Client,
  held: Arc<Mutex<BTreeMap<(String, String), Held>>>,
}

impl std::fmt::Debug for AiServiceClient {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AiServiceClient")
      .field("url", &self.url)
      .field("shared_secret", &self.shared_secret.as_ref().map(|_| "<set>"))
      .finish_non_exhaustive()
  }
}

impl AiServiceClient {
  /// # Panics
  /// Panics if the HTTP client cannot be built, which only a broken TLS setup
  /// at startup can cause.
  #[must_use]
  pub fn new(url: &str, shared_secret: Option<String>) -> Self {
    AiServiceClient {
      url: url.trim_end_matches('/').to_string(),
      shared_secret: shared_secret.filter(|secret| !secret.is_empty()),
      http: reqwest::Client::builder()
        .timeout(CALL_TIMEOUT)
        .build()
        .expect("(AiServiceClient::new) Unable to build an HTTP client"),
      held: Arc::new(Mutex::new(BTreeMap::new())),
    }
  }

  /// Act on a call without waiting for it: hold the side open, tell a held
  /// side its ships changed, or let it go.
  ///
  /// # Panics
  /// Panics if the table of held sides' lock is poisoned.
  pub fn send(&self, call: AiCall) {
    let key = match &call {
      AiCall::Crew { scenario_id, side, .. } | AiCall::Release { scenario_id, side } => {
        (scenario_id.clone(), side.clone())
      }
    };
    let mut held = self.held.lock().unwrap();
    if matches!(call, AiCall::Release { .. }) {
      // Dropping the sender is the signal: the connection is closed.
      if held.remove(&key).is_some() {
        info!("(AiServiceClient) Releasing {}/{}.", key.0, key.1);
      }
      return;
    }
    if let Some(side) = held.get(&key).filter(|side| !side.task.is_finished()) {
      side.side.send_replace(call);
      return;
    }
    let (sender, receiver) = watch::channel(call);
    let task = tokio::spawn(self.clone().hold(receiver));
    held.insert(key, Held { side: sender, task });
  }

  /// Hold one side open until it is released or its crew ends, opening the
  /// connection again whenever it drops.
  async fn hold(self, mut side: watch::Receiver<AiCall>) {
    let name = format!("{:?}", side.borrow());
    let mut retry = FIRST_RETRY;
    loop {
      match self.hold_once(&mut side).await {
        HoldEnding::Released | HoldEnding::CrewEnded => return,
        HoldEnding::Dropped { was_open, why } => {
          if side.has_changed().is_err() {
            return;
          }
          if was_open {
            retry = FIRST_RETRY;
          }
          warn!("(AiServiceClient) Lost the AI service for {name} ({why}); trying again in {retry:?}.");
          tokio::time::sleep(retry).await;
          retry = (retry * 2).min(MAX_RETRY);
        }
      }
    }
  }

  /// One connection's worth of holding a side.
  async fn hold_once(&self, side: &mut watch::Receiver<AiCall>) -> HoldEnding {
    let dropped = |was_open: bool, why: String| HoldEnding::Dropped { was_open, why };
    let mut socket = match self.connect().await {
      Ok(socket) => socket,
      Err(e) => return dropped(false, e.to_string()),
    };
    let mut call = side.borrow_and_update().clone();
    if let Err(e) = socket
      .send(Message::text(serde_json::to_string(&call).unwrap_or_default()))
      .await
    {
      return dropped(false, e.to_string());
    }
    info!("(AiServiceClient) Holding {call:?}.");
    let mut ping = tokio::time::interval(HOLD_PING);
    ping.tick().await;
    loop {
      tokio::select! {
        changed = side.changed() => {
          if changed.is_err() {
            let _ = socket.close(None).await;
            return HoldEnding::Released;
          }
          call = side.borrow_and_update().clone();
          if let Err(e) = socket.send(Message::text(serde_json::to_string(&call).unwrap_or_default())).await {
            return dropped(true, e.to_string());
          }
          info!("(AiServiceClient) Told the AI service: {call:?}");
        }
        frame = socket.next() => match frame {
          Some(Ok(Message::Close(Some(close)))) if close.reason == CREW_ENDED => {
            info!("(AiServiceClient) The AI service's crew for {call:?} has ended.");
            return HoldEnding::CrewEnded;
          }
          Some(Ok(Message::Close(close))) => return dropped(true, format!("closed: {close:?}")),
          Some(Ok(_)) => {}
          Some(Err(e)) => return dropped(true, e.to_string()),
          None => return dropped(true, "connection ended".to_string()),
        },
        _ = ping.tick() => {
          if let Err(e) = socket.send(Message::Ping(Vec::new().into())).await {
            return dropped(true, e.to_string());
          }
        }
      }
    }
  }

  /// Open a connection to the service's `/crew`, with our credential.
  async fn connect(
    &self,
  ) -> Result<
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>,
    Box<dyn std::error::Error + Send + Sync>,
  > {
    let bearer = match &self.shared_secret {
      Some(secret) => secret.clone(),
      None => self.identity_token().await?,
    };
    let url = format!(
      "{}/crew",
      self.url.replacen("https://", "wss://", 1).replacen("http://", "ws://", 1)
    );
    let mut request = url.into_client_request()?;
    request
      .headers_mut()
      .insert("authorization", format!("Bearer {bearer}").parse()?);
    let (socket, _) = tokio::time::timeout(CALL_TIMEOUT, tokio_tungstenite::connect_async(request)).await??;
    Ok(socket)
  }

  /// A Google identity token for this backend's service account, naming the
  /// AI service as audience: the ordinary Cloud Run service-to-service
  /// arrangement. Fresh on every call; the metadata server caches.
  async fn identity_token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(
      self
        .http
        .get(METADATA_IDENTITY_URL)
        .query(&[("audience", self.url.as_str())])
        .header("Metadata-Flavor", "Google")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?,
    )
  }
}

/// How one connection holding a side ended.
enum HoldEnding {
  Released,
  CrewEnded,
  Dropped { was_open: bool, why: String },
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ship::{config_test_ship_templates, get_ship_template, Controller};
  use cgmath::Zero;

  fn side(team: Option<Team>, ships: &[&str]) -> AiSide {
    AiSide {
      team,
      ships: ships.iter().map(ToString::to_string).collect(),
    }
  }

  #[tokio::test]
  async fn sides_group_ai_ships_by_team_and_leave_people_alone() {
    config_test_ship_templates().await;
    let design = get_ship_template("Buccaneer").unwrap();
    let mut entities = Entities::new();
    for (name, team, ai) in [
      ("Executor", Some(Team::Blue), false),
      ("Invigilator", Some(Team::Red), true),
      ("Cicada", Some(Team::Red), true),
      ("Drifter", None, true),
    ] {
      entities.add_ship(
        name.to_string(),
        crate::entity::Vec3::zero(),
        crate::entity::Vec3::zero(),
        &design,
        None,
        None,
      );
      let mut ship = entities.ships.get(name).unwrap().write().unwrap();
      ship.team = team;
      if ai {
        ship.controller = Controller::Ai;
      }
    }
    let sides = sides_of(&entities);
    assert_eq!(sides.len(), 2);
    assert_eq!(sides["Red"], side(Some(Team::Red), &["Cicada", "Invigilator"]));
    assert_eq!(sides["ship:Drifter"], side(None, &["Drifter"]));
  }

  #[test]
  fn only_changed_sides_are_called_and_gone_ones_released() {
    let before = Sides::from([
      ("Red".to_string(), side(Some(Team::Red), &["Cicada", "Invigilator"])),
      ("Gold".to_string(), side(Some(Team::Gold), &["Lurker"])),
      ("Green".to_string(), side(Some(Team::Green), &["Picket"])),
    ]);
    let after = Sides::from([
      // Cicada was destroyed: Red is re-stated without it.
      ("Red".to_string(), side(Some(Team::Red), &["Invigilator"])),
      // Green is unchanged, so nothing is said about it.
      ("Green".to_string(), side(Some(Team::Green), &["Picket"])),
      // Someone switched a Blue ship to AI.
      ("Blue".to_string(), side(Some(Team::Blue), &["Executor"])),
    ]);
    let calls = plan_calls("s", &before, &after);
    assert_eq!(
      calls,
      vec![
        AiCall::Crew {
          scenario_id: "s".to_string(),
          side: "Blue".to_string(),
          team: Some(Team::Blue),
          ships: vec!["Executor".to_string()],
        },
        AiCall::Crew {
          scenario_id: "s".to_string(),
          side: "Red".to_string(),
          team: Some(Team::Red),
          ships: vec!["Invigilator".to_string()],
        },
        AiCall::Release {
          scenario_id: "s".to_string(),
          side: "Gold".to_string(),
        },
      ]
    );
    assert!(plan_calls("s", &after, &after).is_empty(), "nothing changed, nothing said");
  }

  fn crew(ships: &[&str]) -> AiCall {
    AiCall::Crew {
      scenario_id: "thebus-1".to_string(),
      side: "Red".to_string(),
      team: Some(Team::Red),
      ships: ships.iter().map(ToString::to_string).collect(),
    }
  }

  /// A fake AI service: each connection to it, its path and bearer token, and
  /// then every message it is sent; `None` when the connection closes.
  // The handshake callback's error type is tungstenite's, not ours.
  #[allow(clippy::result_large_err)]
  async fn fake_service() -> (String, tokio::sync::mpsc::UnboundedReceiver<Option<String>>) {
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/", listener.local_addr().unwrap());
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
    tokio::spawn(async move {
      while let Ok((stream, _)) = listener.accept().await {
        let tx = tx.clone();
        tokio::spawn(async move {
          let mut opened = String::new();
          let mut socket = tokio_tungstenite::accept_hdr_async(stream, |request: &Request, response: Response| {
            let bearer = request
              .headers()
              .get("authorization")
              .and_then(|value| value.to_str().ok())
              .unwrap_or_default();
            opened = format!("{} {bearer}", request.uri().path());
            Ok(response)
          })
          .await
          .unwrap();
          let _ = tx.send(Some(opened));
          while let Some(Ok(message)) = socket.next().await {
            match message {
              Message::Text(text) => {
                let _ = tx.send(Some(text.to_string()));
              }
              Message::Close(_) => break,
              _ => {}
            }
          }
          let _ = tx.send(None);
        });
      }
    });
    (url, rx)
  }

  async fn next(rx: &mut tokio::sync::mpsc::UnboundedReceiver<Option<String>>) -> Option<String> {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
      .await
      .expect("the fake service heard nothing")
      .expect("the fake service stopped")
  }

  /// A side is held open on `/crew` with our credential, told the whole
  /// side, told again when it changes, and closed when it is released.
  #[tokio::test]
  async fn the_client_holds_a_side_open_until_it_is_released() {
    let (url, mut rx) = fake_service().await;
    let client = AiServiceClient::new(&url, Some("local-secret".to_string()));
    client.send(crew(&["Invigilator"]));
    assert_eq!(next(&mut rx).await.as_deref(), Some("/crew Bearer local-secret"));
    assert_eq!(
      next(&mut rx).await.as_deref(),
      Some(r#"{"scenario_id":"thebus-1","side":"Red","team":"Red","ships":["Invigilator"]}"#)
    );

    client.send(crew(&["Invigilator", "Cicada"]));
    assert_eq!(
      next(&mut rx).await.as_deref(),
      Some(r#"{"scenario_id":"thebus-1","side":"Red","team":"Red","ships":["Invigilator","Cicada"]}"#),
      "a change goes down the same connection"
    );

    client.send(AiCall::Release {
      scenario_id: "thebus-1".to_string(),
      side: "Red".to_string(),
    });
    assert_eq!(next(&mut rx).await, None, "released: the connection is closed");
    assert!(!format!("{client:?}").contains("local-secret"));
  }

  /// A side whose connection drops is held again, restated in full.
  #[tokio::test]
  async fn a_dropped_side_is_held_again() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let client = AiServiceClient::new(&url, Some("local-secret".to_string()));
    client.send(crew(&["Invigilator"]));
    for _ in 0..2 {
      let (stream, _) = listener.accept().await.unwrap();
      let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
      let Some(Ok(Message::Text(text))) = socket.next().await else {
        panic!("the side was not stated");
      };
      assert!(text.contains(r#""ships":["Invigilator"]"#), "{text}");
      // Drop the connection without closing it, as a redeploy would.
      drop(socket);
    }
  }

  /// A side whose crew has ended is not held again.
  #[tokio::test]
  async fn a_side_whose_crew_ended_is_left_alone() {
    use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
    use tokio_tungstenite::tungstenite::protocol::CloseFrame;
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let client = AiServiceClient::new(&url, Some("local-secret".to_string()));
    client.send(crew(&["Invigilator"]));
    let (stream, _) = listener.accept().await.unwrap();
    let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
    let _ = socket.next().await;
    socket
      .close(Some(CloseFrame {
        code: CloseCode::Normal,
        reason: CREW_ENDED.into(),
      }))
      .await
      .unwrap();
    let again = tokio::time::timeout(Duration::from_secs(3), listener.accept()).await;
    assert!(again.is_err(), "the side was held again after its crew ended");
  }
}

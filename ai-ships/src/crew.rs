//! One side's connection to the backend, and its round loop.
//!
//! A crew plays the way a human crew does, over the same WebSocket and the same
//! messages: log in, join the scenario, claim the side's ships, and then once a
//! round decide, give the orders, and say Ready.
//!
//! **When to reconnect.** The backend closes every socket after half an hour
//! with nobody doing anything, so Cloud Run can scale to zero. A crew that
//! reconnected after that would keep an abandoned game alive for ever. So the
//! rule is: if the backend *closed* the connection, the crew is done until the
//! backend asks again with `/crew`. If the connection merely *dropped* -- a
//! redeploy, a network blip -- the crew reconnects a few times with backoff,
//! and then gives up.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use callisto::payloads::{ChangeRole, JoinScenarioMsg, RequestMsg, ResponseMsg, Role, ServiceLoginMsg, SetAiReport};
use callisto::ship::{merge_ship_templates, AiReport, AiStatus, ShipDesignTemplate};
use futures_util::{SinkExt, StreamExt};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;
use tracing::{info, warn};

use crate::auth::Credential;
use crate::commander::Commander;
use crate::observation::observe;
use callisto::entity::Entities;
use callisto::tactics;

/// Keepalive, matching the browser client's. Pings do not count as activity on
/// the backend, so they keep the socket open without keeping a game alive.
const PING_INTERVAL: Duration = Duration::from_secs(60);

/// Where the backend is and how to log in to it, shared by every crew.
#[derive(Debug, Clone)]
pub struct CrewConfig {
  /// `ws://` or `wss://` URL of the backend.
  pub backend: String,
  pub credential: Credential,
  /// Reconnects allowed after a dropped connection before the crew gives up.
  pub reconnect_attempts: u32,
  /// Wait before the first reconnect; doubled for each after.
  pub reconnect_backoff: Duration,
}

/// How a crew's time with the backend ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Ending {
  /// The service released the side.
  Released,
  /// The backend closed the connection: it is idle, or shutting the game.
  ClosedByBackend,
  /// The scenario is not running on the backend.
  ScenarioGone,
  /// The backend would not log us in.
  Refused(String),
  /// The connection dropped or failed; worth trying again.
  Dropped(String),
}

/// Crew one side until it is released, the backend closes on us, or the
/// connection cannot be kept.
pub async fn run(
  config: Arc<CrewConfig>, scenario_id: String, side: String, mut ships: watch::Receiver<Vec<String>>,
  mut commander: Box<dyn Commander>,
) -> Ending {
  let mut attempts = 0;
  loop {
    let ending = session(&config, &scenario_id, &mut ships, commander.as_mut()).await;
    match ending {
      Ending::Dropped(why) if attempts < config.reconnect_attempts => {
        let wait = config.reconnect_backoff * 2u32.pow(attempts);
        attempts += 1;
        warn!("(crew {scenario_id}/{side}) Connection dropped ({why}); reconnecting in {wait:?}.");
        tokio::time::sleep(wait).await;
      }
      ending => {
        info!("(crew {scenario_id}/{side}) Finished: {ending:?}.");
        return ending;
      }
    }
  }
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// What the next frame from the backend was.
enum Frame {
  Response(Box<ResponseMsg>),
  Closed,
  Dropped(String),
}

async fn next_frame(socket: &mut Socket) -> Frame {
  loop {
    match socket.next().await {
      Some(Ok(Message::Text(text))) => match serde_json::from_str::<ResponseMsg>(&text) {
        Ok(response) => return Frame::Response(Box::new(response)),
        Err(e) => warn!("(crew) Could not read a message from the backend: {e}"),
      },
      Some(Ok(Message::Close(_))) => return Frame::Closed,
      Some(Ok(_)) => {}
      Some(Err(e)) => return Frame::Dropped(e.to_string()),
      None => return Frame::Dropped("connection ended".to_string()),
    }
  }
}

async fn send(socket: &mut Socket, request: &RequestMsg) -> Result<(), String> {
  let text = serde_json::to_string(request).map_err(|e| e.to_string())?;
  socket.send(Message::text(text)).await.map_err(|e| e.to_string())
}

/// The backend's designs, so its entity updates can be read.
fn learn_designs(designs: HashMap<String, ShipDesignTemplate>) {
  let designs: HashMap<String, Arc<ShipDesignTemplate>> =
    designs.into_iter().map(|(name, design)| (name, Arc::new(design))).collect();
  merge_ship_templates(designs);
}

/// What the side's crew tells the people it reports to about a ship this
/// round: still deciding, or the plan and why this ship has her orders.
fn report(ship: &str, round: u32, status: AiStatus, plan: Option<&str>, rationale: Option<&str>) -> RequestMsg {
  RequestMsg::SetAiReport(SetAiReport {
    ship_name: ship.to_string(),
    report: Some(AiReport {
      round,
      status,
      plan: plan.map(str::to_string),
      rationale: rationale.map(str::to_string),
    }),
  })
}

/// The side's orders for the round, and its reports. The commander sees only
/// what the side may know; its intents are carried out against the world, as
/// a console would.
async fn decide(commander: &mut dyn Commander, world: &Entities, ships: &[String]) -> Vec<RequestMsg> {
  let view = observe(world, ships);
  let intents = commander.decide(&view).await;
  let status = if commander.answered() {
    AiStatus::Ready
  } else {
    AiStatus::NoAnswer
  };
  let mut requests: Vec<RequestMsg> = ships
    .iter()
    .filter(|ship| world.ships.contains_key(*ship))
    .map(|ship| {
      let rationale = intents
        .iter()
        .find(|(name, _)| name == ship)
        .map(|(_, intent)| intent.rationale.as_str())
        .filter(|text| !text.is_empty());
      report(ship, view.round, status, commander.plan(), rationale)
    })
    .collect();
  requests.extend(
    intents
      .into_iter()
      .filter(|(ship, _)| ships.contains(ship))
      .flat_map(|(ship, intent)| {
        tactics::orders(world, &ship, &intent).unwrap_or_else(|e| {
          warn!("(crew) Could not carry out {ship}'s orders: {e}");
          vec![]
        })
      }),
  );
  requests
}

fn claim(ships: &[String]) -> RequestMsg {
  RequestMsg::SetRole(ChangeRole {
    roles: vec![Role::General],
    ship: None,
    ships: ships.to_vec(),
  })
}

/// One connection's worth of crewing.
async fn session(
  config: &CrewConfig, scenario_id: &str, ships: &mut watch::Receiver<Vec<String>>, commander: &mut dyn Commander,
) -> Ending {
  let token = match config.credential.token().await {
    Ok(token) => token,
    Err(e) => return Ending::Dropped(format!("no credential: {e}")),
  };
  let mut socket = match tokio_tungstenite::connect_async(config.backend.as_str()).await {
    Ok((socket, _)) => socket,
    Err(e) => return Ending::Dropped(e.to_string()),
  };

  // Log in.
  if let Err(e) = send(&mut socket, &RequestMsg::ServiceLogin(ServiceLoginMsg { token })).await {
    return Ending::Dropped(e);
  }
  loop {
    match next_frame(&mut socket).await {
      Frame::Response(response) => match *response {
        ResponseMsg::AuthResponse(_) => break,
        ResponseMsg::Error(tag) => return Ending::Refused(tag),
        _ => {}
      },
      Frame::Closed => return Ending::ClosedByBackend,
      Frame::Dropped(why) => return Ending::Dropped(why),
    }
  }

  // Join. Designs arrive with the login, before any entity update.
  let join = RequestMsg::JoinScenario(JoinScenarioMsg {
    scenario_name: scenario_id.to_string(),
  });
  if let Err(e) = send(&mut socket, &join).await {
    return Ending::Dropped(e);
  }
  loop {
    match next_frame(&mut socket).await {
      Frame::Response(response) => match *response {
        ResponseMsg::DesignTemplateResponse(designs) => learn_designs(designs),
        ResponseMsg::JoinedScenario(_) => break,
        ResponseMsg::Error(_) => return Ending::ScenarioGone,
        _ => {}
      },
      Frame::Closed => return Ending::ClosedByBackend,
      Frame::Dropped(why) => return Ending::Dropped(why),
    }
  }

  // Claim the side. Sent before anything is read past the join, so the backend
  // has the claim before this connection ever says Ready -- a connection
  // holding no ships would count as a referee.
  let mut claimed = ships.borrow_and_update().clone();
  if let Err(e) = send(&mut socket, &claim(&claimed)).await {
    return Ending::Dropped(e);
  }
  info!("(crew {scenario_id}) Crewing {claimed:?}.");

  let mut last_round = None;
  let mut ping = tokio::time::interval(PING_INTERVAL);
  ping.tick().await;
  loop {
    tokio::select! {
      frame = next_frame(&mut socket) => match frame {
        Frame::Response(response) => match *response {
          ResponseMsg::DesignTemplateResponse(designs) => learn_designs(designs),
          ResponseMsg::EntityResponse(world) if last_round != Some(world.round) => {
            // A new round, or the first we have seen: decide, then say Ready.
            // Updates within a round (someone changed a plan) are not a new
            // decision.
            last_round = Some(world.round);
            // Say so first: a model can take half a minute, and the table
            // should see the AI thinking, not a game that has stopped.
            for ship in claimed.iter().filter(|ship| world.ships.contains_key(*ship)) {
              if let Err(e) = send(&mut socket, &report(ship, world.round, AiStatus::Deciding, commander.plan(), None)).await {
                return Ending::Dropped(e);
              }
            }
            let mut orders = decide(commander, &world, &claimed).await;
            orders.push(RequestMsg::SetReady(true));
            for order in &orders {
              if let Err(e) = send(&mut socket, order).await {
                return Ending::Dropped(e);
              }
            }
          }
          ResponseMsg::PleaseLogin => return Ending::Dropped("the backend forgot our session".to_string()),
          _ => {}
        },
        Frame::Closed => return Ending::ClosedByBackend,
        Frame::Dropped(why) => return Ending::Dropped(why),
      },
      changed = ships.changed() => {
        if changed.is_err() {
          // Released: the sender is gone.
          let _ = socket.close(None).await;
          return Ending::Released;
        }
        claimed.clone_from(&ships.borrow_and_update());
        if let Err(e) = send(&mut socket, &claim(&claimed)).await {
          return Ending::Dropped(e);
        }
        info!("(crew {scenario_id}) Now crewing {claimed:?}.");
      },
      _ = ping.tick() => {
        if let Err(e) = send(&mut socket, &RequestMsg::Ping).await {
          return Ending::Dropped(e);
        }
      },
    }
  }
}

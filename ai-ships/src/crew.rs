//! One side's connection to the backend, and its round loop.
//!
//! A crew plays the way a human crew does, over the same WebSocket and the same
//! messages: log in, join the scenario, claim the side's ships, and then once a
//! round decide, give the orders, and say Ready.
//!
//! **How a crew reaches the game.** Over a [`Tunnel`]: the game's messages
//! ride on the side's own `/crew` connection, which the backend relays to
//! itself, so the crew always reaches the instance that holds the game. Cloud
//! Run may run several backends, and a crew that dialled the backend's public
//! URL could land on any of them.
//!
//! **When it ends.** The backend closes every socket after half an hour with
//! nobody doing anything, so Cloud Run can scale to zero, and a crew that came
//! back after that would keep an abandoned game alive for ever. So if the
//! backend *closes* the game's connection, the crew is done until the backend
//! states the side again on `/crew`. If the `/crew` connection merely *drops*
//! -- a redeploy, Cloud Run's hour-long request limit -- the backend holds the
//! side again, with a new tunnel, and the crew plays on over that.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use callisto::payloads::{ChangeRole, JoinScenarioMsg, RequestMsg, ResponseMsg, Role, ServiceLoginMsg, SetAiReport};
use callisto::ship::{merge_ship_templates, AiReport, AiStatus, ShipDesignTemplate};
use tokio::sync::{mpsc, watch};
use tracing::{info, warn};

use crate::auth::Credential;
use crate::commander::Commander;
use crate::observation::observe;
use callisto::entity::Entities;
use callisto::tactics;

/// The shortest a round lasts with only AI sides to wait on.
const MIN_ROUND: std::time::Duration = std::time::Duration::from_secs(2);
/// Keepalive, matching the browser client's. Pings do not count as activity on
/// the backend, so they keep the socket open without keeping a game alive.
const PING_INTERVAL: Duration = Duration::from_secs(60);

/// How to log in to the backend, shared by every crew.
#[derive(Debug, Clone)]
pub struct CrewConfig {
  pub credential: Credential,
}

/// How long a crew waits for the backend to hold its side again after the
/// connection carrying its game drops.
const TUNNEL_WAIT: Duration = Duration::from_secs(120);

/// What arrives over a tunnel from the backend's game connection.
#[derive(Debug)]
pub enum Inbound {
  /// One message, as the backend sent it.
  Text(String),
  /// The backend closed the game connection.
  Closed,
}

/// A game connection carried on a side's `/crew` connection. Dropping it
/// is the crew's end of the connection going away.
#[derive(Debug)]
pub struct Tunnel {
  /// From the game. The sender going away means the `/crew` connection
  /// dropped.
  pub inbound: mpsc::UnboundedReceiver<Inbound>,
  /// To the game.
  pub outbound: mpsc::UnboundedSender<String>,
}

/// The other ends of a [`Tunnel`], for whoever carries it.
#[derive(Debug)]
pub struct TunnelEnds {
  pub inbound: mpsc::UnboundedSender<Inbound>,
  pub outbound: mpsc::UnboundedReceiver<String>,
}

/// A tunnel and the ends that carry it.
#[must_use]
pub fn tunnel() -> (Tunnel, TunnelEnds) {
  let (to_crew, inbound) = mpsc::unbounded_channel();
  let (outbound, from_crew) = mpsc::unbounded_channel();
  (
    Tunnel { inbound, outbound },
    TunnelEnds {
      inbound: to_crew,
      outbound: from_crew,
    },
  )
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
  /// The connection dropped, and the backend did not hold the side again.
  Dropped(String),
}

/// Crew one side until it is released, the backend closes on us, or the
/// backend stops holding the side. Each tunnel that arrives on `tunnels` is a
/// connection to the game: the first, and a new one each time the backend
/// holds the side again.
pub async fn run(
  config: Arc<CrewConfig>, scenario_id: String, side: String, mut ships: watch::Receiver<Vec<String>>,
  mut commander: Box<dyn Commander>, mut tunnels: mpsc::UnboundedReceiver<Tunnel>,
) -> Ending {
  loop {
    let ending = match next_tunnel(&mut tunnels, &mut ships).await {
      Ok(tunnel) => match session(tunnel, &config, &scenario_id, &mut ships, commander.as_mut()).await {
        Ending::Dropped(why) => {
          warn!("(crew {scenario_id}/{side}) {why}; waiting for the backend to hold the side again.");
          continue;
        }
        ending => ending,
      },
      Err(ending) => ending,
    };
    info!("(crew {scenario_id}/{side}) Finished: {ending:?}.");
    return ending;
  }
}

/// What the next frame from the backend was.
enum Frame {
  Response(Box<ResponseMsg>),
  Closed,
  Dropped(String),
}

fn read(text: &str) -> Option<Frame> {
  match serde_json::from_str::<ResponseMsg>(text) {
    Ok(response) => Some(Frame::Response(Box::new(response))),
    Err(e) => {
      warn!("(crew) Could not read a message from the backend: {e}");
      None
    }
  }
}

impl Tunnel {
  async fn next_frame(&mut self) -> Frame {
    loop {
      match self.inbound.recv().await {
        Some(Inbound::Text(text)) => {
          if let Some(frame) = read(&text) {
            return frame;
          }
        }
        Some(Inbound::Closed) => return Frame::Closed,
        None => return Frame::Dropped("the side's connection dropped".to_string()),
      }
    }
  }

  fn send(&self, request: &RequestMsg) -> Result<(), String> {
    let text = serde_json::to_string(request).map_err(|e| e.to_string())?;
    self
      .outbound
      .send(text)
      .map_err(|_| "the side's connection dropped".to_string())
  }
}

/// The next tunnel the backend sends, waiting a while for it.
async fn next_tunnel(
  tunnels: &mut mpsc::UnboundedReceiver<Tunnel>, ships: &mut watch::Receiver<Vec<String>>,
) -> Result<Tunnel, Ending> {
  let deadline = tokio::time::sleep(TUNNEL_WAIT);
  tokio::pin!(deadline);
  loop {
    tokio::select! {
      tunnel = tunnels.recv() => return tunnel.ok_or(Ending::Released),
      changed = ships.changed() => if changed.is_err() {
        return Err(Ending::Released);
      },
      () = &mut deadline => return Err(Ending::Dropped("the backend did not hold the side again".to_string())),
    }
  }
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
  mut tunnel: Tunnel, config: &CrewConfig, scenario_id: &str, ships: &mut watch::Receiver<Vec<String>>,
  commander: &mut dyn Commander,
) -> Ending {
  let token = match config.credential.token().await {
    Ok(token) => token,
    Err(e) => return Ending::Dropped(format!("no credential: {e}")),
  };
  let socket = &mut tunnel;

  // Log in.
  if let Err(e) = socket.send(&RequestMsg::ServiceLogin(ServiceLoginMsg { token })) {
    return Ending::Dropped(e);
  }
  loop {
    match socket.next_frame().await {
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
  if let Err(e) = socket.send(&join) {
    return Ending::Dropped(e);
  }
  loop {
    match socket.next_frame().await {
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
  if let Err(e) = socket.send(&claim(&claimed)) {
    return Ending::Dropped(e);
  }
  info!("(crew {scenario_id}) Crewing {claimed:?}.");

  let mut last_round = None;
  let mut ping = tokio::time::interval(PING_INTERVAL);
  ping.tick().await;
  loop {
    tokio::select! {
      frame = socket.next_frame() => match frame {
        Frame::Response(response) => match *response {
          ResponseMsg::DesignTemplateResponse(designs) => learn_designs(designs),
          ResponseMsg::EntityResponse(world) if last_round != Some(world.round) => {
            // A new round, or the first we have seen: decide, then say Ready.
            // Updates within a round (someone changed a plan) are not a new
            // decision.
            last_round = Some(world.round);
            let began = tokio::time::Instant::now();
            // Say so first: a model can take half a minute, and the table
            // should see the AI thinking, not a game that has stopped.
            for ship in claimed.iter().filter(|ship| world.ships.contains_key(*ship)) {
              if let Err(e) = socket.send(&report(ship, world.round, AiStatus::Deciding, commander.plan(), None)) {
                return Ending::Dropped(e);
              }
            }
            let orders = decide(commander, &world, &claimed).await;
            for order in &orders {
              if let Err(e) = socket.send(order) {
                return Ending::Dropped(e);
              }
            }
            // Not Ready before a round has lasted MIN_ROUND: with nobody
            // else to wait on, an AI side would otherwise run the rounds as
            // fast as it decides, too fast to watch.
            tokio::time::sleep_until(began + MIN_ROUND).await;
            if let Err(e) = socket.send(&RequestMsg::SetReady(true)) {
              return Ending::Dropped(e);
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
          return Ending::Released;
        }
        claimed.clone_from(&ships.borrow_and_update());
        if let Err(e) = socket.send(&claim(&claimed)) {
          return Ending::Dropped(e);
        }
        info!("(crew {scenario_id}) Now crewing {claimed:?}.");
      },
      _ = ping.tick() => {
        if let Err(e) = socket.send(&RequestMsg::Ping) {
          return Ending::Dropped(e);
        }
      },
    }
  }
}

//! A crew against a fake backend at the far end of its tunnel: scripted
//! replies, as the backend's game would send them through the `/crew` relay.

use std::sync::Arc;
use std::time::Duration;

use ai_ships::auth::Credential;
use ai_ships::commander::Idle;
use ai_ships::crew::{run, tunnel, CrewConfig, Ending, Inbound, Tunnel, TunnelEnds};
use callisto::entity::Entities;
use callisto::payloads::{AuthResponse, RequestMsg, ResponseMsg};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

fn config() -> Arc<CrewConfig> {
  Arc::new(CrewConfig {
    credential: Credential::SharedSecret("local-secret".to_string()),
  })
}

fn world(round: u32) -> ResponseMsg {
  let mut world = Entities::new();
  world.round = round;
  ResponseMsg::EntityResponse(Box::new(world))
}

/// The next request the crew sent, skipping keepalives; `None` once it lets
/// the tunnel go or says nothing for a while.
async fn request(ends: &mut TunnelEnds) -> Option<RequestMsg> {
  loop {
    let text = tokio::time::timeout(Duration::from_secs(5), ends.outbound.recv())
      .await
      .ok()??;
    match serde_json::from_str::<RequestMsg>(&text).unwrap() {
      RequestMsg::Ping => {}
      request => return Some(request),
    }
  }
}

fn reply(ends: &TunnelEnds, response: &ResponseMsg) {
  ends
    .inbound
    .send(Inbound::Text(serde_json::to_string(response).unwrap()))
    .unwrap();
}

fn logged_in() -> ResponseMsg {
  ResponseMsg::AuthResponse(AuthResponse {
    email: "ai-ships@callisto.local".to_string(),
    scenario: None,
    roles: None,
    ship: None,
    ai_allowed: false,
  })
}

/// Logs the crew in and lets it join, as the backend would.
async fn admit(ends: &mut TunnelEnds) {
  match request(ends).await {
    Some(RequestMsg::ServiceLogin(login)) => assert_eq!(login.token, "local-secret"),
    other => panic!("expected a service login, got {other:?}"),
  }
  reply(ends, &logged_in());
  reply(ends, &ResponseMsg::DesignTemplateResponse(std::collections::HashMap::new()));
  match request(ends).await {
    Some(RequestMsg::JoinScenario(join)) => assert_eq!(join.scenario_name, "thebus-1"),
    other => panic!("expected a join, got {other:?}"),
  }
  reply(ends, &ResponseMsg::JoinedScenario("thebus-1".to_string()));
}

/// A crew for `ships`, with its first tunnel: the crew, the ends of that
/// tunnel, and where to send it more.
fn crew(ships: watch::Receiver<Vec<String>>) -> (JoinHandle<Ending>, TunnelEnds, mpsc::UnboundedSender<Tunnel>) {
  let (tunnels, arriving) = mpsc::unbounded_channel();
  let (first, ends) = tunnel();
  tunnels.send(first).unwrap();
  let crew = tokio::spawn(run(
    config(),
    "thebus-1".to_string(),
    "Red".to_string(),
    ships,
    Box::new(Idle),
    arriving,
  ));
  (crew, ends, tunnels)
}

async fn ending(crew: JoinHandle<Ending>) -> Ending {
  tokio::time::timeout(Duration::from_secs(5), crew).await.unwrap().unwrap()
}

#[tokio::test]
async fn a_crew_claims_its_side_and_says_ready_once_a_round() {
  let (ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string(), "Cicada".to_string()]);
  let (crew, mut ends, _tunnels) = crew(ships_rx);
  admit(&mut ends).await;
  // The world as it stands on joining.
  reply(&ends, &world(3));

  match request(&mut ends).await {
    Some(RequestMsg::SetRole(role)) => {
      assert_eq!(role.ship, None);
      assert_eq!(role.ships, vec!["Invigilator".to_string(), "Cicada".to_string()]);
    }
    other => panic!("expected the side to be claimed first, got {other:?}"),
  }
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetReady(true))));

  // Someone changes a plan mid-round: not a new decision.
  reply(&ends, &world(3));
  // The round advances: decide again and say Ready again.
  reply(&ends, &world(4));
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetReady(true))));

  // Cicada is destroyed; the backend re-states the side.
  ships_tx.send_replace(vec!["Invigilator".to_string()]);
  match request(&mut ends).await {
    Some(RequestMsg::SetRole(role)) => assert_eq!(role.ships, vec!["Invigilator".to_string()]),
    other => panic!("expected a new claim, got {other:?}"),
  }

  // Released.
  drop(ships_tx);
  assert_eq!(ending(crew).await, Ending::Released);
}

/// A backend that closes the game's connection -- idle, or ending the game --
/// is not come back to: that would keep an abandoned game alive.
#[tokio::test]
async fn a_crew_the_backend_closes_on_does_not_come_back() {
  let (_ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let (crew, mut ends, tunnels) = crew(ships_rx);
  admit(&mut ends).await;
  ends.inbound.send(Inbound::Closed).unwrap();
  assert_eq!(ending(crew).await, Ending::ClosedByBackend);
  assert!(
    tunnels.send(tunnel().0).is_err(),
    "the crew is gone, not waiting for another tunnel"
  );
}

/// A crew plays on over the next tunnel when one drops, logging in afresh.
#[tokio::test]
async fn a_crew_plays_on_over_the_next_tunnel() {
  let (_ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let (crew, mut ends, tunnels) = crew(ships_rx);
  admit(&mut ends).await;
  reply(&ends, &world(1));
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetRole(_))));
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetReady(true))));

  // The side's connection drops: the backend holds the side again, with a
  // new tunnel.
  drop(ends);
  let (second, mut ends) = tunnel();
  tunnels.send(second).unwrap();
  admit(&mut ends).await;
  reply(&ends, &world(2));
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetRole(_))));
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::SetReady(true))));

  ends.inbound.send(Inbound::Closed).unwrap();
  assert_eq!(ending(crew).await, Ending::ClosedByBackend);
}

/// A crew waiting for the backend to hold its side again stops when the side
/// is released instead.
#[tokio::test]
async fn a_crew_waiting_for_a_tunnel_is_released() {
  let (ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let (crew, mut ends, _tunnels) = crew(ships_rx);
  admit(&mut ends).await;
  drop(ends);
  tokio::time::sleep(Duration::from_millis(50)).await;
  drop(ships_tx);
  assert_eq!(ending(crew).await, Ending::Released);
}

/// A join that finds no such game ends the crew: over its tunnel it is on the
/// instance that holds every game it is asked to crew, so this one is over.
#[tokio::test]
async fn a_join_that_finds_no_game_ends_the_crew() {
  let (_ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let (crew, mut ends, _tunnels) = crew(ships_rx);
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::ServiceLogin(_))));
  reply(&ends, &logged_in());
  assert!(matches!(request(&mut ends).await, Some(RequestMsg::JoinScenario(_))));
  reply(&ends, &ResponseMsg::Error("No such scenario".to_string()));
  assert_eq!(ending(crew).await, Ending::ScenarioGone);
}

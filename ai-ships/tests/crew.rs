//! A crew against a fake backend: a real WebSocket, scripted replies.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use ai_ships::auth::Credential;
use ai_ships::commander::Idle;
use ai_ships::crew::{run, CrewConfig, Ending};
use callisto::entity::Entities;
use callisto::payloads::{AuthResponse, RequestMsg, ResponseMsg};
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpListener;
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

type Backend = tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>;

async fn reply(socket: &mut Backend, response: ResponseMsg) {
  socket
    .send(Message::text(serde_json::to_string(&response).unwrap()))
    .await
    .unwrap();
}

/// The next request the crew sent, skipping keepalives.
async fn request(socket: &mut Backend) -> Option<RequestMsg> {
  loop {
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.next()).await.ok()??;
    match frame.ok()? {
      Message::Text(text) => match serde_json::from_str::<RequestMsg>(&text).unwrap() {
        RequestMsg::Ping => {}
        request => return Some(request),
      },
      Message::Close(_) => return None,
      _ => {}
    }
  }
}

fn world(round: u32) -> ResponseMsg {
  let mut world = Entities::new();
  world.round = round;
  ResponseMsg::EntityResponse(Box::new(world))
}

fn config(port: u16, attempts: u32) -> Arc<CrewConfig> {
  Arc::new(CrewConfig {
    backend: format!("ws://127.0.0.1:{port}"),
    credential: Credential::SharedSecret("local-secret".to_string()),
    reconnect_attempts: attempts,
    reconnect_backoff: Duration::from_millis(10),
  })
}

/// Logs the crew in and lets it join, as the backend would.
async fn admit(socket: &mut Backend) {
  match request(socket).await {
    Some(RequestMsg::ServiceLogin(login)) => assert_eq!(login.token, "local-secret"),
    other => panic!("expected a service login, got {other:?}"),
  }
  reply(
    socket,
    ResponseMsg::AuthResponse(AuthResponse {
      email: "ai-ships@callisto.local".to_string(),
      scenario: None,
      roles: None,
      ship: None,
      ai_allowed: false,
    }),
  )
  .await;
  reply(socket, ResponseMsg::DesignTemplateResponse(std::collections::HashMap::new())).await;
  match request(socket).await {
    Some(RequestMsg::JoinScenario(join)) => assert_eq!(join.scenario_name, "thebus-1"),
    other => panic!("expected a join, got {other:?}"),
  }
  reply(socket, ResponseMsg::JoinedScenario("thebus-1".to_string())).await;
}

#[tokio::test]
async fn a_crew_claims_its_side_and_says_ready_once_a_round() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let (ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string(), "Cicada".to_string()]);
  let crew = tokio::spawn(run(
    config(port, 0),
    "thebus-1".to_string(),
    "Red".to_string(),
    ships_rx,
    Box::new(Idle),
  ));

  let (stream, _) = listener.accept().await.unwrap();
  let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
  admit(&mut socket).await;
  // The world as it stands on joining.
  reply(&mut socket, world(3)).await;

  match request(&mut socket).await {
    Some(RequestMsg::SetRole(role)) => {
      assert_eq!(role.ship, None);
      assert_eq!(role.ships, vec!["Invigilator".to_string(), "Cicada".to_string()]);
    }
    other => panic!("expected the side to be claimed first, got {other:?}"),
  }
  assert!(matches!(request(&mut socket).await, Some(RequestMsg::SetReady(true))));

  // Someone changes a plan mid-round: not a new decision.
  reply(&mut socket, world(3)).await;
  // The round advances: decide again and say Ready again.
  reply(&mut socket, world(4)).await;
  assert!(matches!(request(&mut socket).await, Some(RequestMsg::SetReady(true))));

  // Cicada is destroyed; the backend re-states the side.
  ships_tx.send_replace(vec!["Invigilator".to_string()]);
  match request(&mut socket).await {
    Some(RequestMsg::SetRole(role)) => assert_eq!(role.ships, vec!["Invigilator".to_string()]),
    other => panic!("expected a new claim, got {other:?}"),
  }

  // Released.
  drop(ships_tx);
  assert!(request(&mut socket).await.is_none(), "the crew closes its connection");
  assert_eq!(crew.await.unwrap(), Ending::Released);
}

/// A backend that closes on us -- idle, or ending the game -- is not
/// reconnected to: that would keep an abandoned game alive.
#[tokio::test]
async fn a_crew_the_backend_closes_on_does_not_come_back() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let (_ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let accepts = Arc::new(AtomicUsize::new(0));
  let counted = accepts.clone();
  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      counted.fetch_add(1, Ordering::SeqCst);
      let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
      admit(&mut socket).await;
      let _ = socket.close(None).await;
    }
  });

  let ending = run(
    config(port, 5),
    "thebus-1".to_string(),
    "Red".to_string(),
    ships_rx,
    Box::new(Idle),
  )
  .await;
  assert_eq!(ending, Ending::ClosedByBackend);
  assert_eq!(accepts.load(Ordering::SeqCst), 1);
}

/// A connection that merely drops is retried, up to the limit.
#[tokio::test]
async fn a_dropped_connection_is_retried_then_given_up() {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let (_ships_tx, ships_rx) = watch::channel(vec!["Invigilator".to_string()]);
  let accepts = Arc::new(AtomicUsize::new(0));
  let counted = accepts.clone();
  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      counted.fetch_add(1, Ordering::SeqCst);
      // Accept the TCP connection and drop it mid-handshake.
      drop(stream);
    }
  });

  let ending = run(
    config(port, 2),
    "thebus-1".to_string(),
    "Red".to_string(),
    ships_rx,
    Box::new(Idle),
  )
  .await;
  assert!(matches!(ending, Ending::Dropped(_)), "{ending:?}");
  assert_eq!(accepts.load(Ordering::SeqCst), 3, "the first try and two retries");
}

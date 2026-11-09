//! The endpoints: who may call them, and what a side's held connection does.

use std::sync::Arc;
use std::time::Duration;

use ai_ships::api::{router, App, Callers, CREW_ENDED};
use ai_ships::auth::Credential;
use ai_ships::commander::{Commander, Idle};
use ai_ships::crew::CrewConfig;
use ai_ships::crews::{CrewStatus, Crews};
use axum::body::Body;
use axum::http::{Request, StatusCode};
use callisto::authentication::UserDirectory;
use callisto::service_auth::ServiceAuth;
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;

/// The app, with crews pointed at `backend`.
fn app(backend: &str) -> Arc<App> {
  let config = CrewConfig {
    backend: backend.to_string(),
    credential: Credential::SharedSecret("local-secret".to_string()),
    reconnect_attempts: 0,
    reconnect_backoff: Duration::from_millis(1),
  };
  Arc::new(App {
    crews: Crews::new(config, Arc::new(|_: &str, _: &str| Box::new(Idle) as Box<dyn Commander>)),
    callers: Callers {
      auth: ServiceAuth::new(Some("local-secret".to_string()), None, None),
      allowed: UserDirectory::default(),
    },
  })
}

/// A backend that takes a crew's connection and keeps it, saying nothing, so
/// the crew runs until it is released.
async fn quiet_backend() -> String {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("ws://{}", listener.local_addr().unwrap());
  tokio::spawn(async move {
    while let Ok((stream, _)) = listener.accept().await {
      tokio::spawn(async move {
        let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
        while socket.next().await.is_some() {}
      });
    }
  });
  url
}

/// Serve the app; its `ws://` URL.
async fn serve(app: Arc<App>) -> String {
  let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
  let url = format!("ws://{}", listener.local_addr().unwrap());
  tokio::spawn(async move { axum::serve(listener, router(app)).await.unwrap() });
  url
}

type Socket = tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn hold(url: &str, token: Option<&str>) -> Result<Socket, Box<tokio_tungstenite::tungstenite::Error>> {
  let mut request = format!("{url}/crew").into_client_request().unwrap();
  if let Some(token) = token {
    request
      .headers_mut()
      .insert("authorization", format!("Bearer {token}").parse().unwrap());
  }
  tokio_tungstenite::connect_async(request)
    .await
    .map(|(socket, _)| socket)
    .map_err(Box::new)
}

const CREW: &str = r#"{"scenario_id":"thebus-1","side":"Red","team":"Red","ships":["Invigilator"]}"#;

/// What the service is crewing, once it has had a moment to act.
async fn crewing(app: &App, want: usize) -> Vec<CrewStatus> {
  for _ in 0..100 {
    if app.crews.status().len() == want {
      break;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  app.crews.status()
}

#[tokio::test]
async fn only_a_caller_with_the_credential_is_heard() {
  let app = app("ws://127.0.0.1:9");
  let url = serve(app.clone()).await;
  for token in [None, Some("guess")] {
    match hold(&url, token).await.map_err(|e| *e) {
      Err(tokio_tungstenite::tungstenite::Error::Http(response)) => {
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
      }
      other => panic!("a caller without the credential was let in: {other:?}"),
    }
  }
  assert_eq!(app.crews.status(), vec![], "nothing was started");

  let health = router(app)
    .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(health.status(), StatusCode::OK, "health needs no credential");
}

/// Holding a side crews it, a later message changes its ships, and closing
/// the connection releases it.
#[tokio::test]
async fn a_side_is_crewed_while_it_is_held() {
  let app = app(&quiet_backend().await);
  let url = serve(app.clone()).await;
  let mut socket = hold(&url, Some("local-secret")).await.unwrap();
  socket.send(Message::text(CREW)).await.unwrap();
  let crews = crewing(&app, 1).await;
  assert_eq!(
    (crews[0].scenario_id.as_str(), crews[0].side.as_str(), crews[0].ships.clone()),
    ("thebus-1", "Red", vec!["Invigilator".to_string()])
  );

  socket
    .send(Message::text(CREW.replace(r#"["Invigilator"]"#, r#"["Invigilator","Cicada"]"#)))
    .await
    .unwrap();
  for _ in 0..100 {
    if app.crews.status()[0].ships.len() == 2 {
      break;
    }
    tokio::time::sleep(Duration::from_millis(20)).await;
  }
  assert_eq!(app.crews.status()[0].ships, vec!["Invigilator", "Cicada"]);

  socket.close(None).await.unwrap();
  assert_eq!(crewing(&app, 0).await, vec![], "closing released the side");
}

/// A connection that drops leaves its crew playing, for the backend to hold
/// it again.
#[tokio::test]
async fn a_dropped_side_is_crewed_on() {
  let app = app(&quiet_backend().await);
  let url = serve(app.clone()).await;
  let mut socket = hold(&url, Some("local-secret")).await.unwrap();
  socket.send(Message::text(CREW)).await.unwrap();
  assert_eq!(crewing(&app, 1).await.len(), 1);
  drop(socket);
  tokio::time::sleep(Duration::from_millis(300)).await;
  assert_eq!(app.crews.status().len(), 1, "a drop is not a release");
  assert!(app.crews.status()[0].running);
}

/// A crew that ends by itself closes its side's connection, saying so.
#[tokio::test]
async fn a_crew_that_ends_says_so() {
  // Nothing listens on port 9, so the crew fails at once.
  let app = app("ws://127.0.0.1:9");
  let url = serve(app.clone()).await;
  let mut socket = hold(&url, Some("local-secret")).await.unwrap();
  socket.send(Message::text(CREW)).await.unwrap();
  let closed = tokio::time::timeout(Duration::from_secs(5), async {
    loop {
      match socket.next().await {
        Some(Ok(Message::Close(frame))) => return frame,
        Some(Ok(_)) => {}
        other => panic!("the connection ended without a close: {other:?}"),
      }
    }
  })
  .await
  .expect("the service never closed the side");
  assert_eq!(closed.map(|frame| frame.reason.to_string()).as_deref(), Some(CREW_ENDED));
}

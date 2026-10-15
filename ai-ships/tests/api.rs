//! The HTTP endpoints: who may call them, and what they do.

use std::sync::Arc;
use std::time::Duration;

use ai_ships::api::{router, App, Callers};
use ai_ships::auth::Credential;
use ai_ships::commander::{Commander, Idle};
use ai_ships::crew::CrewConfig;
use ai_ships::crews::Crews;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use callisto::authentication::UserDirectory;
use callisto::service_auth::ServiceAuth;
use http_body_util::BodyExt;
use tower::ServiceExt;

fn app() -> Arc<App> {
  // Nothing listens on port 9, so crews started here fail quietly.
  let config = CrewConfig {
    backend: "ws://127.0.0.1:9".to_string(),
    credential: Credential::SharedSecret("local-secret".to_string()),
    reconnect_attempts: 0,
    reconnect_backoff: Duration::from_millis(1),
  };
  Arc::new(App {
    crews: Crews::new(config, Arc::new(|| Box::new(Idle) as Box<dyn Commander>)),
    callers: Callers {
      auth: ServiceAuth::new(Some("local-secret".to_string()), None, None),
      allowed: UserDirectory::default(),
    },
  })
}

fn post(path: &str, token: Option<&str>, body: &str) -> Request<Body> {
  let builder = Request::post(path).header("content-type", "application/json");
  let builder = match token {
    Some(token) => builder.header("authorization", format!("Bearer {token}")),
    None => builder,
  };
  builder.body(Body::from(body.to_string())).unwrap()
}

fn get(path: &str, token: &str) -> Request<Body> {
  Request::get(path)
    .header("authorization", format!("Bearer {token}"))
    .body(Body::empty())
    .unwrap()
}

const CREW: &str = r#"{"scenario_id":"thebus-1","side":"Red","team":"Red","ships":["Invigilator"]}"#;

#[tokio::test]
async fn only_a_caller_with_the_credential_is_heard() {
  let app = app();
  for token in [None, Some("guess")] {
    let response = router(app.clone()).oneshot(post("/crew", token, CREW)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
  }
  assert_eq!(app.crews.status(), vec![], "nothing was started");

  let health = router(app)
    .oneshot(Request::get("/healthz").body(Body::empty()).unwrap())
    .await
    .unwrap();
  assert_eq!(health.status(), StatusCode::OK, "health needs no credential");
}

#[tokio::test]
async fn crew_then_release_a_side() {
  let app = app();
  let response = router(app.clone())
    .oneshot(post("/crew", Some("local-secret"), CREW))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NO_CONTENT);

  let status = router(app.clone()).oneshot(get("/status", "local-secret")).await.unwrap();
  let body = status.into_body().collect().await.unwrap().to_bytes();
  let crews: serde_json::Value = serde_json::from_slice(&body).unwrap();
  assert_eq!(crews[0]["scenario_id"], "thebus-1");
  assert_eq!(crews[0]["side"], "Red");
  assert_eq!(crews[0]["ships"], serde_json::json!(["Invigilator"]));

  let response = router(app.clone())
    .oneshot(post(
      "/release",
      Some("local-secret"),
      r#"{"scenario_id":"thebus-1","side":"Red"}"#,
    ))
    .await
    .unwrap();
  assert_eq!(response.status(), StatusCode::NO_CONTENT);
  assert_eq!(app.crews.status(), vec![]);
}

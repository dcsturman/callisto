//! The AI service's entry point. See the library docs for what it does.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ai_ships::api::{router, App, Callers};
use ai_ships::auth::Credential;
use ai_ships::commander::{Commander, Idle};
use ai_ships::crew::CrewConfig;
use ai_ships::crews::Crews;
use callisto::authentication::{GoogleAuthenticator, UserDirectory};
use callisto::service_auth::ServiceAuth;
use clap::Parser;
use tracing::info;

#[derive(Parser, Debug)]
#[command(version, about = "Crews AI-controlled ships in callisto.")]
struct Args {
  /// Port to listen on. Cloud Run sets `PORT`.
  #[arg(long, env = "PORT", default_value_t = 8080)]
  port: u16,

  /// The backend's WebSocket URL.
  #[arg(long, env = "CALLISTO_BACKEND", default_value = "ws://localhost:30000")]
  backend: String,

  /// Shared secret for local Compose, both to log in to the backend and to
  /// admit its calls. Unset on Cloud Run, which uses identity tokens.
  #[arg(long, env = "CALLISTO_SERVICE_TOKEN", hide_env_values = true)]
  service_token: Option<String>,

  /// The audience our identity token names when logging in to the backend:
  /// its URL. Defaults to the backend URL with `ws` read as `http`.
  #[arg(long, env = "CALLISTO_BACKEND_AUDIENCE")]
  backend_audience: Option<String>,

  /// This service's own URL: the audience a caller's identity token must name.
  #[arg(long, env = "CALLISTO_AI_AUDIENCE")]
  audience: Option<String>,

  /// Accounts allowed to call this service with an identity token, comma
  /// separated: the backend's service account.
  #[arg(long, env = "CALLISTO_AI_ALLOWED_CALLERS", value_delimiter = ',')]
  allowed_callers: Vec<String>,
}

fn http_audience(backend: &str) -> String {
  backend
    .strip_prefix("wss://")
    .map(|rest| format!("https://{rest}"))
    .or_else(|| backend.strip_prefix("ws://").map(|rest| format!("http://{rest}")))
    .unwrap_or_else(|| backend.to_string())
}

#[tokio::main]
async fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  let args = Args::parse();

  let secret = args.service_token.clone().filter(|secret| !secret.is_empty());
  let credential = secret.clone().map_or_else(
    || Credential::CloudRun {
      audience: args.backend_audience.clone().unwrap_or_else(|| http_audience(&args.backend)),
    },
    Credential::SharedSecret,
  );
  let google_keys = match &args.audience {
    Some(_) => Some(GoogleAuthenticator::fetch_google_public_keys().await),
    None => None,
  };
  let callers = Callers {
    auth: ServiceAuth::new(secret, args.audience.clone(), google_keys),
    allowed: UserDirectory {
      active: args
        .allowed_callers
        .iter()
        .map(|email| email.trim().to_lowercase())
        .collect::<HashSet<_>>(),
      ..UserDirectory::default()
    },
  };
  let config = CrewConfig {
    backend: args.backend.clone(),
    credential,
    reconnect_attempts: 5,
    reconnect_backoff: Duration::from_secs(2),
  };
  info!("(main) Backend {}, logging in with {:?}.", config.backend, config.credential);
  let commander = Arc::new(|| Box::new(Idle) as Box<dyn Commander>);
  let app = Arc::new(App {
    crews: Crews::new(config, commander),
    callers,
  });

  let listener = tokio::net::TcpListener::bind(("0.0.0.0", args.port))
    .await
    .expect("(main) Unable to bind the port");
  info!("(main) ai-ships listening on {}.", args.port);
  axum::serve(listener, router(app)).await.expect("(main) Server failed");
}

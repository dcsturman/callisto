//! The AI service's entry point. See the library docs for what it does.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use ai_ships::api::{router, App, Callers};
use ai_ships::auth::Credential;
use ai_ships::commander::Commander;
use ai_ships::crew::CrewConfig;
use ai_ships::crews::Crews;
use ai_ships::llm::{self, Llm};
use ai_ships::provider::{ModelSpec, Vertex};
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

  /// Who decides for the ships: `network`, the trained network built into
  /// the service; a model (`gemini`, `gemini:<model>`, `claude`,
  /// `claude:<model>`, `deepseek`, `maas:<publisher>/<model>`); or a
  /// baseline (`close`, `hold`, `idle`).
  #[arg(long, env = "CALLISTO_AI_MODEL", default_value = "network")]
  model: String,

  /// The GCP project the model calls are billed to. Defaults to the
  /// credential's own project.
  #[arg(long, env = "GOOGLE_CLOUD_PROJECT")]
  gcp_project: Option<String>,

  /// Record every model call here, one JSON-lines file per side per game,
  /// beside the backend's record of the game. Unset means not recorded.
  #[arg(long, env = "CALLISTO_GAME_LOG_DIR")]
  game_log_dir: Option<String>,

  /// The Vertex AI region to call models in.
  #[arg(long, env = "CALLISTO_VERTEX_REGION", default_value = "global")]
  vertex_region: String,
}

/// A side's call record for a game: `<started>-ai-<game>-<side>.jsonl`.
fn call_log(dir: &str, game: &str, side: &str) -> Option<std::fs::File> {
  let safe = |name: &str| -> String {
    name
      .chars()
      .map(|c| {
        if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
          c
        } else {
          '_'
        }
      })
      .collect()
  };
  let started = std::time::SystemTime::now()
    .duration_since(std::time::UNIX_EPOCH)
    .map_or(0, |elapsed| elapsed.as_secs());
  let path = std::path::Path::new(dir).join(format!("{started}-ai-{}-{}.jsonl", safe(game), safe(side)));
  std::fs::create_dir_all(dir).ok()?;
  std::fs::File::create(&path)
    .map_err(|e| tracing::warn!("(main) Not recording {game}/{side}'s calls: {e}"))
    .ok()
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

  let vertex = match llm::model_of(&args.model) {
    Some(_) => Some(Arc::new(
      Vertex::new(args.gcp_project.as_deref(), &args.vertex_region)
        .await
        .expect("(main) Cannot reach Vertex AI"),
    )),
    None => None,
  };
  let model = args.model.clone();
  llm::named(&model, 0, vertex.as_ref()).expect("(main) Unknown commander or model");
  info!(
    "(main) Deciding with {model}{}.",
    vertex
      .as_ref()
      .map_or_else(String::new, |vertex| format!(" in project {}", vertex.project()))
  );
  let game_log_dir = args.game_log_dir.clone().filter(|dir| !dir.is_empty());
  let commander = Arc::new(move |game: &str, side: &str| -> Box<dyn Commander> {
    // A model side records every call beside the backend's record of the game.
    if let (Some(spec), Some(vertex), Some(dir)) = (ModelSpec::parse(&model), vertex.as_ref(), game_log_dir.as_ref()) {
      if let Some(file) = call_log(dir, game, side) {
        let system = llm::system_for(&model).expect("checked at startup");
        return Box::new(
          Llm::new(Arc::clone(vertex), spec)
            .with_system(system)
            .with_call_log(game, Box::new(file)),
        );
      }
    }
    // A fleet commander records its calls the same way.
    if let (Some(spec), Some(vertex), Some(dir), true) = (
      llm::model_of(&model),
      vertex.as_ref(),
      game_log_dir.as_ref(),
      model.starts_with("admiral:"),
    ) {
      if let (Some(file), Some(net)) = (call_log(dir, game, side), ai_ships::network::commander::built_in()) {
        return Box::new(ai_ships::admiral::Admiral::new(net, Arc::clone(vertex), spec).with_call_log(Box::new(file)));
      }
    }
    llm::named(&model, 0, vertex.as_ref()).expect("checked at startup")
  });
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

//! The AI service's entry point. See the library docs for what it does.

use std::collections::HashSet;
use std::sync::Arc;

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

  /// Shared secret for local Compose, both to log in to the backend and to
  /// admit its calls. Unset on Cloud Run, which uses identity tokens.
  #[arg(long, env = "CALLISTO_SERVICE_TOKEN", hide_env_values = true)]
  service_token: Option<String>,

  /// The audience our identity token names when logging in to the backend:
  /// its `https://` URL. Needed unless the shared secret is set. Crews reach
  /// the backend over the `/crew` connection it holds open to us, so this is
  /// the only thing we need to know about it.
  #[arg(long, env = "CALLISTO_BACKEND_AUDIENCE")]
  backend_audience: Option<String>,

  /// This service's own URL: the audience a caller's identity token must name.
  #[arg(long, env = "CALLISTO_AI_AUDIENCE")]
  audience: Option<String>,

  /// Accounts allowed to call this service with an identity token, comma
  /// separated: the backend's service account.
  #[arg(long, env = "CALLISTO_AI_ALLOWED_CALLERS", value_delimiter = ',')]
  allowed_callers: Vec<String>,

  /// Who decides for the ships: `admiral:gemini` (the default), Gemini
  /// writing squadron scripts from the orders and the network fighting them;
  /// `network`, the trained network alone, which ignores orders; a model (`gemini`, `gemini:<model>`, `claude`,
  /// `claude:<model>`, `deepseek`, `maas:<publisher>/<model>`); or a
  /// baseline (`close`, `hold`, `idle`).
  #[arg(long, env = "CALLISTO_AI_MODEL", default_value = "admiral:gemini")]
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

#[tokio::main]
async fn main() {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  let args = Args::parse();

  let secret = args.service_token.clone().filter(|secret| !secret.is_empty());
  let credential = secret.clone().map_or_else(
    || Credential::CloudRun {
      audience: args
        .backend_audience
        .clone()
        .expect("(main) Set CALLISTO_BACKEND_AUDIENCE, or CALLISTO_SERVICE_TOKEN for local use."),
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
  let config = CrewConfig { credential };
  info!("(main) Logging in to the backend with {:?}.", config.credential);

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

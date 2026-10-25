//! Play a scenario many times with a commander on each side, and score it.
//!
//! ```text
//! cargo run --release --bin harness -- \
//!   --scenario ../callisto/scenarios/Thebus.json \
//!   --side Blue=gemini --side Red=close --games 5 --rounds 40 --transcript runs/thebus
//! ```
//!
//! Sides are named as the backend names them: a team (`Red`, `Blue`, `Green`,
//! `Gold`), or `ship:<name>` for a ship on no team. Any side not named plays
//! `--default`. Commanders: `idle`, `hold`, `close`, `random`,
//! `replay:<log>:<side>`, or a model: `gemini`, `gemini:<model>`, `claude`,
//! `claude:<model>`. A model needs Google credentials (`gcloud auth
//! application-default login`) and a project (`--project` or
//! `GOOGLE_CLOUD_PROJECT`).

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  ai_ships::cli::run(&|_, _| None).await
}

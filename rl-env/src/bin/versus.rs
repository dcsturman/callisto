//! The harness, with trained networks as commanders, beside the baselines
//! and the models: `net:<file.cpol>` for the per-ship network and
//! `side:<file.cside>` for the per-side one, playing their best choice; with
//! `~` (`net~:`, `side~:`) drawing as in training; and a goal after the file
//! (`side:<file.cside>#destroy`) to play for that goal rather than the
//! scenario's. Every other commander and option is the harness's own (see
//! `ai_ships::cli`).
//!
//! ```text
//! cargo run --release --bin versus -- --scenario "../callisto/scenarios/Tessaly Crossing.json" \
//!   --side Blue=side:checkpoints/side-2/final.cside#destroy --side Red=haiku+think --games 10
//! ```

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  // Only the networks: the baselines and models are the harness's own.
  let networks = |name: &str, seed: u64| {
    let kind = name.split(':').next().unwrap_or_default().trim_end_matches('~');
    if kind == "net" || kind == "side" {
      callisto_rl::commander::named(name, seed)
    } else {
      None
    }
  };
  ai_ships::cli::run(&networks).await
}

//! The harness, with trained networks as commanders: `net:<file.cpol>` plays
//! its best-scored choice, `net~:<file.cpol>` draws as in training, and a
//! goal after the file (`net:<file.cpol>#destroy`) plays for that goal rather
//! than the scenario's. Every other commander and option is the harness's
//! own (see `ai_ships::cli`).
//!
//! ```text
//! cargo run --release --bin versus -- --scenario "../callisto/scenarios/Tessaly Crossing.json" \
//!   --side Blue=net:checkpoints/selfplay-1/final.cpol --side Red=gemini --games 10
//! ```

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ai_ships::commander::Commander;
use callisto_rl::commander::Net;
use callisto_rl::net::Policy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  // Each file is read once, however many games it plays.
  let loaded: Mutex<HashMap<String, Arc<Policy>>> = Mutex::new(HashMap::new());
  let net = |name: &str, seed: u64| -> Option<Box<dyn Commander>> {
    let (sampling, path) = name
      .strip_prefix("net~:")
      .map(|path| (true, path))
      .or_else(|| name.strip_prefix("net:").map(|path| (false, path)))?;
    let (path, goal) = match path.split_once('#') {
      Some((path, goal)) => {
        let goal = serde_json::from_value(serde_json::Value::String(goal.to_string()))
          .map_err(|_| eprintln!("no goal {goal}: destroy, hold, raid or escape"))
          .ok()?;
        (path, Some(goal))
      }
      None => (path, None),
    };
    let mut loaded = loaded.lock().ok()?;
    let policy = if let Some(policy) = loaded.get(path) {
      Arc::clone(policy)
    } else {
      let policy = Arc::new(Policy::load(path).map_err(|e| eprintln!("{e}")).ok()?);
      loaded.insert(path.to_string(), Arc::clone(&policy));
      policy
    };
    let commander = Net::new(policy);
    let commander = match goal {
      Some(goal) => commander.with_goal(goal),
      None => commander,
    };
    Some(Box::new(if sampling { commander.sampling(seed) } else { commander }))
  };
  ai_ships::cli::run(&net).await
}

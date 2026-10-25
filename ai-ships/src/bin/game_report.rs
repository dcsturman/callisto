//! Summarise the recorded games in a directory: what was played, who gave
//! the orders, and what the AI's model calls cost.
//!
//! ```text
//! cargo run --release --bin game_report -- ../game_logs
//! ```
//!
//! Reads the backend's game records (`<started>-<game>.jsonl`) and the AI
//! service's call records (`<started>-ai-<game>-<side>.jsonl`) that
//! `CALLISTO_GAME_LOG_DIR` produces, and prints one line per game.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};

use clap::Parser;
use serde_json::Value;

#[derive(Parser, Debug)]
#[command(about = "Summarise recorded games: orders by who gave them, and model cost.")]
struct Args {
  /// The directory games are recorded in.
  #[arg(default_value = "../game_logs")]
  dir: String,
}

#[derive(Default)]
struct Game {
  scenario: String,
  started: u64,
  rounds: u64,
  orders: BTreeMap<String, u64>,
  calls: u64,
  unanswered: u64,
  input: u64,
  cached: u64,
  output: u64,
  cost: f64,
}

fn lines(path: &std::path::Path) -> impl Iterator<Item = Value> {
  fs::File::open(path)
    .map(BufReader::new)
    .into_iter()
    .flat_map(BufRead::lines)
    .map_while(Result::ok)
    .filter_map(|line| serde_json::from_str(&line).ok())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
  let args = Args::parse();
  let mut games: BTreeMap<String, Game> = BTreeMap::new();
  for entry in fs::read_dir(&args.dir)? {
    let path = entry?.path();
    if path.extension().and_then(|ext| ext.to_str()) != Some("jsonl") {
      continue;
    }
    for record in lines(&path) {
      if let Some(event) = record["event"].as_str() {
        let Some(game) = record["game"].as_str().map(str::to_string).or_else(|| {
          // Only the start record names the game; the rest belong to the file.
          path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| stem.split_once('-').map_or(stem, |(_, game)| game).to_string())
        }) else {
          continue;
        };
        let game = games.entry(game).or_default();
        match event {
          "start" => {
            game.scenario = record["scenario"].as_str().unwrap_or_default().to_string();
            game.started = record["started_at"].as_u64().unwrap_or_default();
          }
          "order" if record["accepted"].as_bool() == Some(true) => {
            let by = record["by"].as_str().unwrap_or("?").to_string();
            *game.orders.entry(by).or_default() += 1;
          }
          "round" => game.rounds = game.rounds.max(record["round"].as_u64().unwrap_or(0)),
          _ => {}
        }
      } else if let Some(name) = record["game"].as_str() {
        // An AI call.
        let game = games.entry(name.to_string()).or_default();
        game.calls += 1;
        if record["answered"].as_bool() == Some(false) {
          game.unanswered += 1;
        }
        game.input += record["input_tokens"].as_u64().unwrap_or(0);
        game.cached += record["cached_tokens"].as_u64().unwrap_or(0);
        game.output += record["output_tokens"].as_u64().unwrap_or(0);
        game.cost += record["cost_usd"].as_f64().unwrap_or(0.0);
      }
    }
  }

  let mut total = 0.0;
  let mut ordered: Vec<(&String, &Game)> = games.iter().collect();
  ordered.sort_by_key(|(_, game)| game.started);
  for (name, game) in ordered {
    total += game.cost;
    let orders: Vec<String> = game.orders.iter().map(|(by, n)| format!("{by} {n}")).collect();
    println!(
      "{name} ({}): {} rounds; orders {}; {} model calls{}, {} in ({} cached), {} out, ${:.3}",
      if game.scenario.is_empty() { "?" } else { &game.scenario },
      game.rounds,
      if orders.is_empty() {
        "none".to_string()
      } else {
        orders.join(", ")
      },
      game.calls,
      if game.unanswered > 0 {
        format!(" ({} unanswered)", game.unanswered)
      } else {
        String::new()
      },
      game.input,
      game.cached,
      game.output,
      game.cost
    );
  }
  println!("{} games, ${total:.3} in model calls", games.len());
  Ok(())
}

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

use std::collections::BTreeMap;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::Arc;

use ai_ships::commander::Commander;
use ai_ships::harness::{opening_view, play, Outcome};
use ai_ships::llm::{self, Llm};
use ai_ships::prompt;
use ai_ships::provider::{ModelSpec, Vertex};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "Play a scenario many times with a commander on each side, and score it.")]
struct Args {
  /// Scenario file to play.
  #[arg(long)]
  scenario: String,
  /// Ship designs to load.
  #[arg(long, default_value = "../callisto/ship_templates")]
  designs: String,
  /// A side and its commander, as `Side=commander`. Repeat for each side.
  #[arg(long = "side", value_parser = parse_side)]
  sides: Vec<(String, String)>,
  /// The commander for any side not named.
  #[arg(long, default_value = "close")]
  default: String,
  /// Games to play, seeds `seed` onwards.
  #[arg(long, default_value_t = 20)]
  games: u64,
  /// First seed.
  #[arg(long, default_value_t = 1)]
  seed: u64,
  /// Most rounds a game may run.
  #[arg(long, default_value_t = 40)]
  rounds: u32,
  /// Write every side's intents, round by round, to this JSON-lines file.
  #[arg(long)]
  log: Option<String>,
  /// Also write each game's outcome as JSON lines to this file.
  #[arg(long)]
  outcomes: Option<String>,
  /// For model commanders: write each game's prompts and replies to
  /// `<dir>/seed-<n>-<side>.md`.
  #[arg(long)]
  transcript: Option<String>,
  /// The GCP project model calls are billed to.
  #[arg(long, env = "GOOGLE_CLOUD_PROJECT")]
  project: Option<String>,
  /// The Vertex AI region.
  #[arg(long, env = "CALLISTO_VERTEX_REGION", default_value = "global")]
  region: String,
  /// Play nothing: print the system prompt and what this side would be shown
  /// in the opening round, then exit.
  #[arg(long)]
  show_prompt: Option<String>,
}

fn parse_side(text: &str) -> Result<(String, String), String> {
  text
    .split_once('=')
    .map(|(side, commander)| (side.to_string(), commander.to_string()))
    .ok_or_else(|| format!("expected Side=commander, got {text}"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();
  let args = Args::parse();
  replace_ship_templates(load_ship_templates_from_dir(&args.designs).await?);

  if let Some(side) = &args.show_prompt {
    let view = opening_view(&args.scenario, side).await?;
    println!(
      "{}\n\n---\n\n{}",
      prompt::system(),
      prompt::situation(&view, None, &{
        let mut memory = prompt::Memory::default();
        memory.update(&view);
        memory
      })
    );
    return Ok(());
  }

  let assigned: BTreeMap<String, String> = args.sides.iter().cloned().collect();
  let needs_model = assigned
    .values()
    .chain(std::iter::once(&args.default))
    .any(|name| ModelSpec::parse(name).is_some());
  let vertex = if needs_model {
    Some(Arc::new(Vertex::new(args.project.as_deref(), &args.region).await?))
  } else {
    None
  };
  if let Some(dir) = &args.transcript {
    std::fs::create_dir_all(dir)?;
  }
  let commanders = |side: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    let name = assigned.get(side).unwrap_or(&args.default).clone();
    let commander = match (ModelSpec::parse(&name), vertex.as_ref(), args.transcript.as_ref()) {
      (Some(spec), Some(vertex), Some(dir)) => {
        let path = format!("{dir}/seed-{seed}-{}.md", side.replace(':', "_"));
        let file = File::create(&path).ok()?;
        Some(
          Box::new(Llm::new(Arc::clone(vertex), spec).with_transcript(Box::new(BufWriter::new(file))))
            as Box<dyn Commander>,
        )
      }
      _ => llm::named(&name, seed, vertex.as_ref()),
    };
    commander.map(|commander| (name, commander))
  };

  let mut log = args.log.as_deref().map(File::create).transpose()?.map(BufWriter::new);
  let mut outcomes_file = args.outcomes.as_deref().map(File::create).transpose()?.map(BufWriter::new);
  let mut outcomes: Vec<Outcome> = Vec::new();
  for seed in args.seed..args.seed + args.games {
    let outcome = play(
      &args.scenario,
      &commanders,
      args.rounds,
      seed,
      log.as_mut().map(|log| log as &mut dyn Write),
    )
    .await?;
    let line = outcome.sides.iter().map(|(side, result)| {
      format!(
        "{side} ({}) {}/{} ships, hull {}/{}",
        result.commander, result.ships_at_end, result.ships_at_start, result.hull_at_end, result.hull_at_start
      )
    });
    println!(
      "seed {seed:>4}: {:>2} rounds, winner {:<12} {}",
      outcome.rounds,
      outcome.winner.as_deref().unwrap_or("none"),
      line.collect::<Vec<_>>().join("; ")
    );
    if let Some(file) = outcomes_file.as_mut() {
      writeln!(file, "{}", serde_json::to_string(&outcome)?)?;
    }
    outcomes.push(outcome);
  }

  // The summary: wins per side, and the average share of its hull each side
  // kept.
  println!("\n{} games:", outcomes.len());
  let draws = outcomes.iter().filter(|outcome| outcome.winner.is_none()).count();
  let sides: Vec<String> = outcomes.first().map(|o| o.sides.keys().cloned().collect()).unwrap_or_default();
  for side in &sides {
    let wins = outcomes.iter().filter(|o| o.winner.as_deref() == Some(side)).count();
    #[allow(clippy::cast_precision_loss)]
    let kept: f64 = outcomes
      .iter()
      .map(|o| {
        let result = &o.sides[side];
        f64::from(result.hull_at_end) / f64::from(result.hull_at_start.max(1))
      })
      .sum::<f64>()
      / outcomes.len().max(1) as f64;
    let commander = &outcomes[0].sides[side].commander;
    println!(
      "  {side:<12} ({commander:<6}) won {wins:>3}, kept {:>5.1}% of its hull",
      kept * 100.0
    );
  }
  println!("  draws {draws}");
  Ok(())
}

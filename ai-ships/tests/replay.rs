//! A recorded model game, replayed: what CI runs instead of calling a model.
//!
//! `fixtures/thebus_replay.jsonl` is eight rounds of Thebus with Gemini
//! commanding both sides, recorded by the harness (`--log`, seed 1). Replaying
//! both sides' decisions on the same dice drives the whole path a model's
//! orders take -- intents through `callisto::tactics` into the engine -- with no
//! model and no network.
//!
//! The orders name the ships, sides and positions of the Thebus they were
//! given in, so the replay runs against `fixtures/thebus_as_recorded.json`,
//! a copy of that scenario: the live one under `callisto/scenarios/` is free
//! to change without taking the recording with it.
//!
//! The assertions are about what the orders do -- each side's get shots off,
//! and the Executor's hurt the pirates -- not the numbers, so a rules change
//! that shifts the dice does not break them. To
//! record afresh after a change to the intent format or the scenario, copy
//! the scenario over the fixture and run:
//!
//! ```text
//! cargo run --release --bin harness -- --scenario tests/fixtures/thebus_as_recorded.json \
//!   --side Blue=gemini --side Red=gemini --games 1 --seed 1 --rounds 8 \
//!   --log tests/fixtures/thebus_replay.jsonl
//! ```

use std::collections::HashMap;

use ai_ships::commander::{named, Commander, Replay};
use ai_ships::harness::{play_recorded, Outcome};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates};

const THEBUS: &str = "tests/fixtures/thebus_as_recorded.json";
const RECORDING: &str = "tests/fixtures/thebus_replay.jsonl";

/// The game, and its rounds' combat events as JSON lines.
async fn replayed() -> (Outcome, String) {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let commanders = |side: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    let name = format!("replay:{RECORDING}:{side}");
    named(&name, seed).map(|commander| (name, commander))
  };
  let mut events = Vec::new();
  let outcome = play_recorded(THEBUS, &commanders, 8, 1, None, Some(&mut events)).await.unwrap();
  (outcome, String::from_utf8(events).unwrap())
}

/// The attacks each side made over the game, from the rounds' fire summaries.
fn attacks_by_side(events: &str) -> HashMap<String, u64> {
  let mut attacks = HashMap::new();
  for line in events.lines() {
    let round: serde_json::Value = serde_json::from_str(line).unwrap();
    for fire in round["summary"]["fire"].as_array().into_iter().flatten() {
      let team = fire["team"].as_str().unwrap_or("none").to_string();
      *attacks.entry(team).or_default() += fire["attacks"].as_u64().unwrap_or(0);
    }
  }
  attacks
}

#[tokio::test]
async fn the_recording_covers_every_round_for_both_sides() {
  for side in ["Blue", "Red"] {
    let replay = Replay::from_log(RECORDING, side).unwrap();
    assert_eq!(replay.rounds(), (1..=8).collect::<Vec<u32>>(), "{side}");
  }
}

#[tokio::test]
async fn replayed_orders_fight_the_battle() {
  let (outcome, events) = replayed().await;
  let attacks = attacks_by_side(&events);
  let shots = |side: &str| attacks.get(side).copied().unwrap_or(0);
  assert!(
    shots("Blue") > 0,
    "the pirates' recorded orders get shots off at the Executor: {attacks:?}"
  );
  let pirates = &outcome.sides["Blue"];
  assert!(
    shots("Red") > 0 && pirates.hull_at_end < pirates.hull_at_start,
    "and the Executor's recorded orders fire back and hurt them: {attacks:?} {outcome:?}"
  );
}

#[tokio::test]
async fn a_replay_is_the_same_game_every_time() {
  assert_eq!(replayed().await.0, replayed().await.0);
}

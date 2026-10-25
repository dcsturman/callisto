//! A recorded model game, replayed: what CI runs instead of calling a model.
//!
//! `fixtures/thebus_replay.jsonl` is eight rounds of Thebus with Gemini
//! commanding both sides, recorded by the harness (`--log`, seed 1). Replaying
//! both sides' decisions on the same dice drives the whole path a model's
//! orders take -- intents through `callisto::tactics` into the engine -- with no
//! model and no network.
//!
//! The assertions are about what the orders do, not the exact numbers, so a
//! rules change that shifts a few points of damage does not break them. To
//! record afresh after a change to the intent format:
//!
//! ```text
//! cargo run --release --bin harness -- --scenario ../callisto/scenarios/Thebus.json \
//!   --side Blue=gemini --side Red=gemini --games 1 --seed 1 --rounds 8 \
//!   --log tests/fixtures/thebus_replay.jsonl
//! ```

use ai_ships::commander::{named, Commander, Replay};
use ai_ships::harness::{play, Outcome};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates};

const THEBUS: &str = "../callisto/scenarios/Thebus.json";
const RECORDING: &str = "tests/fixtures/thebus_replay.jsonl";

async fn replayed() -> Outcome {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let commanders = |side: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    let name = format!("replay:{RECORDING}:{side}");
    named(&name, seed).map(|commander| (name, commander))
  };
  play(THEBUS, &commanders, 8, 1, None).await.unwrap()
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
  let outcome = replayed().await;
  let pirates = &outcome.sides["Blue"];
  let executor = &outcome.sides["Red"];
  assert!(
    executor.hull_at_end < executor.hull_at_start,
    "the pirates' recorded orders close and hit the Executor: {outcome:?}"
  );
  assert!(
    pirates.hull_at_end < pirates.hull_at_start,
    "and the Executor's hit back: {outcome:?}"
  );
}

#[tokio::test]
async fn a_replay_is_the_same_game_every_time() {
  assert_eq!(replayed().await, replayed().await);
}

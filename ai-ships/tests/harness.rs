//! The harness plays real scenarios, reproducibly.

use ai_ships::commander::{named, Commander};
use ai_ships::harness::{play, Outcome};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates};

const THEBUS: &str = "../callisto/scenarios/Thebus.json";

async fn thebus(pirates: &str, executor: &str, seed: u64) -> Outcome {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let commanders = |side: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    let name = match side {
      "Blue" => pirates,
      "Red" => executor,
      _ => "idle",
    };
    named(name, seed).map(|commander| (name.to_string(), commander))
  };
  play(THEBUS, &commanders, 40, seed, None).await.unwrap()
}

#[tokio::test]
async fn a_seed_replays_the_same_game_and_another_does_not() {
  let first = thebus("close", "close", 7).await;
  assert_eq!(first, thebus("close", "close", 7).await);
  let others: Vec<Outcome> = futures_util::future::join_all((1..=4).map(|seed| thebus("close", "close", seed))).await;
  assert!(
    others
      .iter()
      .any(|other| other.sides != first.sides || other.rounds != first.rounds),
    "different seeds should not all roll the same fight"
  );
}

#[tokio::test]
async fn a_side_that_gives_no_orders_loses() {
  let outcome = thebus("idle", "close", 3).await;
  assert_eq!(outcome.winner.as_deref(), Some("Red"), "{outcome:?}");
  assert_eq!(outcome.sides["Blue"].ships_at_end, 0);
  assert_eq!(outcome.sides["Red"].commander, "close");
}

#[tokio::test]
async fn a_side_with_no_commander_is_an_error() {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let nobody = |_: &str, _: u64| -> Option<(String, Box<dyn Commander>)> { None };
  assert!(play(THEBUS, &nobody, 5, 1, None).await.is_err());
}

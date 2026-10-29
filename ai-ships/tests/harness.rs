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

#[tokio::test]
async fn the_built_in_network_plays_and_beats_a_side_that_sits_still() {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let commanders = |side: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    let name = if side == "Blue" { "network" } else { "idle" };
    ai_ships::network::commander::named(name, seed).map(|commander| (name.to_string(), commander))
  };
  let outcome = play("../callisto/scenarios/Tessaly Crossing.json", &commanders, 40, 5, None)
    .await
    .unwrap();
  assert_eq!(outcome.winner.as_deref(), Some("Blue"), "{outcome:?}");
}

/// Keelhaven, 196 ships with High Guard's capitals, plays its opening: the
/// attacker comes in from Distant and both sides have lost hull by round 12.
#[tokio::test]
async fn keelhaven_plays_its_opening_rounds() {
  replace_ship_templates(load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap());
  let commanders = |_: &str, seed: u64| -> Option<(String, Box<dyn Commander>)> {
    named("close", seed).map(|c| ("close".to_string(), c))
  };
  let outcome = play("../callisto/scenarios/Keelhaven.json", &commanders, 12, 1, None)
    .await
    .unwrap();
  let (red, blue) = (&outcome.sides["Red"], &outcome.sides["Blue"]);
  assert_eq!((red.ships_at_start, blue.ships_at_start), (107, 89), "{outcome:?}");
  assert!(red.hull_at_end < red.hull_at_start, "{outcome:?}");
  assert!(blue.hull_at_end < blue.hull_at_start, "{outcome:?}");
}

#[test]
fn a_side_with_a_post_holds_it_rather_than_its_course() {
  use ai_ships::network::side::hold_at_post;
  use callisto::entity::Vec3;
  use callisto::tactics::{Helm, Intent};
  let post = Vec3::new(0.0, 0.0, 6.71e8);
  let holding = |helm: Helm| {
    (
      "ship".to_string(),
      Intent {
        helm,
        ..Intent::default()
      },
    )
  };
  let mut intents = vec![
    holding(Helm::Hold),
    holding(Helm::Follow {
      guide: "lead".to_string(),
      guide_helm: Box::new(Helm::Hold),
      guide_cap: None,
    }),
    holding(Helm::OpenRange {
      target: "raider".to_string(),
    }),
  ];
  hold_at_post(&mut intents, Some((post, 25_000_000.0)));
  let at = [0.0, 0.0, 6.71e8];
  assert_eq!(intents[0].1.helm, Helm::Station { position: at });
  assert!(
    matches!(&intents[1].1.helm, Helm::Follow { guide_helm, .. } if **guide_helm == Helm::Station { position: at })
  );
  assert_eq!(
    intents[2].1.helm,
    Helm::OpenRange {
      target: "raider".to_string()
    },
    "other orders stand"
  );

  // A side with no post keeps its course when it holds.
  let mut free = vec![holding(Helm::Hold)];
  hold_at_post(&mut free, None);
  assert_eq!(free[0].1.helm, Helm::Hold);
}

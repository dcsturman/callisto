//! The reward for kills and for firepower taken out.

use std::sync::Arc;

use callisto_rl::game::{firepower, Control, End, Game, Scenario, Shaping};

const DESIGNS: &str = "../callisto/ship_templates";

fn tessaly() -> Arc<Scenario> {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  runtime.block_on(async {
    callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
    Arc::new(Scenario::load("../callisto/scenarios/Tessaly Crossing.json").await.unwrap())
  })
}

/// Play one game of Close against Close, returning each side's total reward
/// and how it ended.
fn play(scenario: &Arc<Scenario>, seed: u64, shaping: Shaping) -> ([f32; 2], End, Game) {
  let close = Control::Baseline("close".to_string());
  let mut game = Game::new(Arc::clone(scenario), &[close.clone(), close], seed, 40).with_shaping(shaping);
  let mut totals = [0.0; 2];
  loop {
    let step = game.step_sides([None, None]);
    totals[0] += step.rewards[0];
    totals[1] += step.rewards[1];
    if let Some(end) = step.end {
      return (totals, end, game);
    }
  }
}

#[test]
fn guns_knocked_out_take_away_firepower() {
  let scenario = tessaly();
  let game = Game::new(Arc::clone(&scenario), &[Control::Agent, Control::Agent], 1, 40);
  let ours = game.view(0).own.clone();
  let theirs = game.view(1).own.clone();
  let mut ship = ours[0].clone();
  let full = firepower(&ship, &theirs);
  assert!(full > 0.0, "an armed ship has firepower");
  ship.active_weapons[0] = false;
  let less = firepower(&ship, &theirs);
  assert!(less < full, "one gun out takes some away: {less} of {full}");
  ship.active_weapons.fill(false);
  assert!(firepower(&ship, &theirs).abs() < f32::EPSILON, "no guns, no firepower");
}

#[test]
fn a_fleet_destroyed_pays_its_ships_and_its_guns() {
  let scenario = tessaly();
  let (seed, plain, end) = (1..40)
    .find_map(|seed| {
      let (plain, end, game) = play(&scenario, seed, Shaping::default());
      println!("seed {seed}: {end:?} after {} rounds", game.rounds);
      matches!(end, End::Destroyed { .. }).then_some((seed, plain, end))
    })
    .expect("some game of Close against Close ends in a kill");
  let End::Destroyed { side: loser } = end else {
    unreachable!()
  };
  let (kills, end_kills, _) = play(
    &scenario,
    seed,
    Shaping {
      kills: 1.0,
      firepower: 0.0,
      ..Shaping::default()
    },
  );
  let (guns, end_guns, _) = play(
    &scenario,
    seed,
    Shaping {
      kills: 0.0,
      firepower: 1.0,
      ..Shaping::default()
    },
  );
  assert_eq!(end, end_kills, "the shaping changes the reward, not the game");
  assert_eq!(end, end_guns);
  let winner = 1 - loser;
  // The loser lost every ship and every gun; the winner pays only for what she lost.
  for (shaped, what) in [(kills, "kills"), (guns, "firepower")] {
    let gained = shaped[winner] - plain[winner];
    let paid = shaped[loser] - plain[loser];
    assert!(
      gained > 0.0 && gained <= 1.0 + 1e-4,
      "{what}: the winner gains up to the whole fleet: {gained}"
    );
    assert!(
      (gained + paid).abs() < 1e-4,
      "{what}: what one side gains the other pays: {gained} {paid}"
    );
  }
}

#[test]
fn holding_pays_every_round_it_holds() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let scenario = runtime.block_on(async {
    callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
    // Jupiter with the Aslan holding the point they start at rather than
    // Europa, which orbits away from ships that sit still.
    let jupiter = std::fs::read_to_string("../callisto/scenarios/jupiter.json")
      .unwrap()
      .replace(r#""post": "Europa""#, r#""post": [0, 0, 650000000.0]"#);
    let path = std::env::temp_dir().join(format!("hold-pays-{}.json", std::process::id()));
    std::fs::write(&path, jupiter).unwrap();
    Arc::new(Scenario::load(path.to_str().unwrap()).await.unwrap())
  });
  // Both sides sit still, so Jupiter's Aslan, whose goal is Hold, hold all game.
  let play = |hold: f32| {
    let still = Control::Baseline("hold".to_string());
    let mut game = Game::new(Arc::clone(&scenario), &[still.clone(), still], 3, 10).with_shaping(Shaping {
      hold,
      ..Shaping::default()
    });
    let (mut totals, mut rounds) = ([0.0_f32; 2], 0);
    loop {
      let step = game.step_sides([None, None]);
      totals[0] += step.rewards[0];
      totals[1] += step.rewards[1];
      rounds += 1;
      if step.end.is_some() {
        return (totals, rounds);
      }
    }
  };
  let (plain, rounds) = play(0.0);
  let (paid, _) = play(0.01);
  assert!(
    (paid[1] - plain[1] - 0.01 * rounds as f32).abs() < 1e-4,
    "the Aslan are paid every round they hold"
  );
  assert!(
    (paid[0] - plain[0]).abs() < 1e-6,
    "the side that is not holding is paid nothing"
  );
}

#[test]
fn a_side_that_sits_still_ends_at_its_post() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let scenario = runtime.block_on(async {
    callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
    Arc::new(
      Scenario::load("../callisto/scenarios/Picket at the Refuelling Point.json")
        .await
        .unwrap(),
    )
  });
  let still = Control::Baseline("hold".to_string());
  let mut game = Game::new(scenario, &[still.clone(), still], 3, 5);
  while game.step_sides([None, None]).end.is_none() {}
  assert_eq!(game.at_post(), [true, true], "neither side moved from where it began");
  let tally = game.tally(0);
  assert!(tally.holding);
  assert!((tally.held - 1.0).abs() < 1e-6, "the picket held every round: {}", tally.held);
}

#[test]
fn a_side_holds_its_scenario_post_not_where_it_started() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let load = |json: String| {
    runtime.block_on(async {
      callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
      let path = std::env::temp_dir().join(format!("post-{}.json", std::process::id()));
      std::fs::write(&path, json).unwrap();
      Arc::new(Scenario::load(path.to_str().unwrap()).await.unwrap())
    })
  };
  let jupiter = std::fs::read_to_string("../callisto/scenarios/jupiter.json").unwrap();
  let still = Control::Baseline("hold".to_string());
  let play = |scenario: Arc<Scenario>| {
    let mut game = Game::new(scenario, &[still.clone(), still.clone()], 3, 3);
    while game.step_sides([None, None]).end.is_none() {}
    game.at_post()
  };
  // Jupiter's Aslan (Red) hold Europa, which they start beside. (Europa
  // orbits, so over a long game ships that sit still are left behind; three
  // rounds is not long.)
  let red = |at: [bool; 2]| at[1];
  assert!(red(play(load(jupiter.clone()))), "sitting beside Europa holds it");
  // Moved 650,000 km away, to Jupiter itself, the same ships sitting still do not.
  assert!(
    !red(play(load(jupiter.replace(r#""post": "Europa""#, r#""post": "Jupiter""#)))),
    "a post far from where the side sits is not held by sitting still"
  );
}

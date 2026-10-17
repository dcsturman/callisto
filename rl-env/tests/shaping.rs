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
    },
  );
  let (guns, end_guns, _) = play(
    &scenario,
    seed,
    Shaping {
      kills: 0.0,
      firepower: 1.0,
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

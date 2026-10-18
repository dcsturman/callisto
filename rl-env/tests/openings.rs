//! Varied openings: each fleet keeps its formation, the fleets move.

use std::sync::Arc;

use callisto::entity::Entity;
use callisto_rl::game::{Control, Game, Scenario};
use cgmath::InnerSpace;

fn scenario() -> Arc<Scenario> {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  runtime.block_on(async {
    callisto::ship::replace_ship_templates(
      callisto::ship::load_ship_templates_from_dir("../callisto/ship_templates")
        .await
        .unwrap(),
    );
    Arc::new(Scenario::load("../callisto/scenarios/Tessaly Crossing.json").await.unwrap())
  })
}

/// Each side's ships' positions, by name.
fn layout(game: &Game) -> [Vec<(String, callisto::entity::Vec3)>; 2] {
  [0, 1].map(|side| {
    let mut ships: Vec<_> = game
      .view(side)
      .own
      .iter()
      .map(|ship| (ship.get_name().to_string(), ship.get_position()))
      .collect();
    ships.sort_by(|a, b| a.0.cmp(&b.0));
    ships
  })
}

fn spread(ships: &[(String, callisto::entity::Vec3)]) -> Vec<f64> {
  ships
    .iter()
    .flat_map(|a| ships.iter().map(move |b| (a.1 - b.1).magnitude()))
    .collect()
}

#[test]
fn a_varied_opening_keeps_formations_and_moves_fleets() {
  let scenario = scenario();
  let agents = [Control::Agent, Control::Agent];
  let plain = layout(&Game::new(Arc::clone(&scenario), &agents, 3, 40));
  let none = layout(&Game::varied(Arc::clone(&scenario), &agents, 3, 40, 0.0));
  assert_eq!(plain, none, "no variation is the scenario as written");
  let varied = layout(&Game::varied(Arc::clone(&scenario), &agents, 3, 40, 0.3));
  for side in 0..2 {
    for (a, b) in spread(&plain[side]).iter().zip(spread(&varied[side])) {
      assert!((a - b).abs() < 1.0, "a fleet's formation is kept: {a} against {b} metres");
    }
  }
  let gap = |l: &[Vec<(String, callisto::entity::Vec3)>; 2]| (l[0][0].1 - l[1][0].1).magnitude();
  assert!(
    (gap(&plain) - gap(&varied)).abs() > 1000.0,
    "the fleets are moved relative to each other"
  );
  let again = layout(&Game::varied(Arc::clone(&scenario), &agents, 3, 40, 0.3));
  assert_eq!(varied, again, "the same seed gives the same opening");
}

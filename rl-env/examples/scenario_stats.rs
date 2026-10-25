//! Each side's ships and firepower in a scenario, for balancing:
//! `cargo run --release --example scenario_stats -- <scenario.json>...`

use std::collections::BTreeMap;
use std::sync::Arc;

use callisto::entity::Entity;
use callisto_rl::game::{firepower, Control, Game, Scenario};

fn main() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  runtime.block_on(async {
    callisto::ship::replace_ship_templates(
      callisto::ship::load_ship_templates_from_dir("../callisto/ship_templates")
        .await
        .unwrap(),
    );
  });
  for path in std::env::args().skip(1) {
    let scenario = Arc::new(runtime.block_on(Scenario::load(&path)).unwrap());
    let names = [scenario.sides[0].0.clone(), scenario.sides[1].0.clone()];
    let game = Game::new(Arc::clone(&scenario), &[Control::Agent, Control::Agent], 1, 40);
    let fleets = [game.view(0).own.clone(), game.view(1).own.clone()];
    println!("## {path}");
    for side in 0..2 {
      let foes = &fleets[1 - side];
      let mut by_design: BTreeMap<String, (usize, f32, u32)> = BTreeMap::new();
      for ship in &fleets[side] {
        let entry = by_design.entry(ship.design.name.clone()).or_default();
        entry.0 += 1;
        entry.1 += firepower(ship, foes);
        entry.2 += ship.current_hull;
      }
      let total: f32 = by_design.values().map(|v| v.1).sum();
      let hull: u32 = by_design.values().map(|v| v.2).sum();
      println!(
        "{} | ships {} | hull {} | firepower {:.1} a round",
        names[side],
        fleets[side].len(),
        hull,
        total
      );
      for (design, (count, fire, _)) in &by_design {
        println!("    {count:>2} x {design:<40} firepower {:.1} each", fire / *count as f32);
      }
    }
    let centre = |side: usize| {
      let ships = &fleets[side];
      ships
        .iter()
        .map(Entity::get_position)
        .fold(callisto::entity::Vec3::new(0.0, 0.0, 0.0), |a, b| a + b)
        / ships.len().max(1) as f64
    };
    println!(
      "    centres {:.0} km apart",
      cgmath::InnerSpace::magnitude(centre(0) - centre(1)) / 1000.0
    );
  }
}

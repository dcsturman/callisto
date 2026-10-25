//! Trace one game with random choices, round by round: `cargo run --release
//! --example trace -- <scenario> <seed>`.

use std::sync::Arc;

use callisto::entity::Entity;
use callisto_rl::action::{Choice, Head, HEADS, MASK_LEN};
use callisto_rl::encode::OBS_LEN;
use callisto_rl::game::{Control, Game, Scenario};
use cgmath::InnerSpace;
use rand::{Rng, SeedableRng};

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let path = args
    .get(1)
    .cloned()
    .unwrap_or_else(|| "../callisto/scenarios/Thebus.json".to_string());
  let seed: u64 = args.get(2).and_then(|seed| seed.parse().ok()).unwrap_or(1);
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let scenario = runtime.block_on(async {
    callisto::ship::replace_ship_templates(
      callisto::ship::load_ship_templates_from_dir("../callisto/ship_templates")
        .await
        .unwrap(),
    );
    Arc::new(Scenario::load(&path).await.unwrap())
  });
  let mut game = Game::new(scenario, &[Control::Agent, Control::Agent], seed, 40);
  let mut rng = rand::rngs::SmallRng::seed_from_u64(seed);
  loop {
    let slots = game.slots().len();
    let mut obs = vec![0.0; slots * OBS_LEN];
    let mut masks = vec![false; slots * MASK_LEN];
    game.observe_into(&mut obs, &mut masks);
    let choices: Vec<Choice> = masks
      .chunks(MASK_LEN)
      .map(|mask| {
        let mut choice: Choice = [0; HEADS];
        for (index, head) in Head::ALL.iter().enumerate() {
          let open: Vec<usize> = (0..head.size()).filter(|c| mask[head.offset() + c]).collect();
          choice[index] = i64::try_from(open[rng.gen_range(0..open.len())]).unwrap();
        }
        choice
      })
      .collect();
    let step = game.step(&choices);
    let world = game.world();
    let line: Vec<String> = world
      .ships
      .values()
      .map(|ship| {
        let ship = ship.read().unwrap();
        format!(
          "{}:{} hull {} at {:.0}km v {:.0}",
          ship.team.map_or("-".to_string(), |team| format!("{team:?}")),
          ship.get_name(),
          ship.current_hull,
          ship.get_position().magnitude() / 1000.0,
          ship.get_velocity().magnitude()
        )
      })
      .collect();
    println!(
      "round {} rewards {:?} end {:?}\n  {}",
      game.rounds,
      step.rewards,
      step.end,
      line.join("\n  ")
    );
    if step.end.is_some() {
      break;
    }
  }
}

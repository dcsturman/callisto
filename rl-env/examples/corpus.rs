//! Record the courses real games plot, one JSON line each, for measuring the
//! solver: `cargo run --release --features corpus --example corpus -- <out>
//! [games per pairing]`.
//!
//! Plays the training pool with baselines against each other, and random
//! choices against `close`, so the corpus has both sensible and odd courses.

use std::io::Write;
use std::sync::Arc;

use callisto_rl::action::{Choice, Head, HEADS, MASK_LEN};
use callisto_rl::encode::OBS_LEN;
use callisto_rl::game::{Control, Game, Scenario};
use rand::{Rng, SeedableRng};

const POOL: [&str; 4] = [
  "../callisto/scenarios/Tessaly Crossing.json",
  "../callisto/scenarios/jupiter.json",
  "../callisto/scenarios/Marduk Encounter.json",
  "../callisto/scenarios/Thebus.json",
];

fn main() {
  let args: Vec<String> = std::env::args().collect();
  let out = args.get(1).cloned().unwrap_or_else(|| "corpus.jsonl".to_string());
  let games: u64 = args.get(2).and_then(|n| n.parse().ok()).unwrap_or(3);
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let scenarios: Vec<Arc<Scenario>> = runtime.block_on(async {
    callisto::ship::replace_ship_templates(
      callisto::ship::load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap(),
    );
    let mut scenarios = Vec::new();
    for path in POOL {
      scenarios.push(Arc::new(Scenario::load(path).await.unwrap()));
    }
    scenarios
  });
  let baseline = |name: &str| Control::Baseline(name.to_string());
  let pairings = [
    [baseline("close"), baseline("close")],
    [baseline("close"), baseline("hold")],
    [Control::Agent, baseline("close")],
    [baseline("close"), Control::Agent],
  ];
  let mut file = std::fs::File::create(&out).unwrap();
  let mut rng = rand::rngs::SmallRng::seed_from_u64(11);
  let mut total = 0;
  for scenario in &scenarios {
    for controls in &pairings {
      for seed in 1..=games {
        let mut game = Game::new(Arc::clone(scenario), controls, seed, 40);
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
          if game.step(&choices).end.is_some() {
            break;
          }
        }
        let courses = callisto::solver::take_corpus();
        total += courses.len();
        for course in courses {
          writeln!(file, "{}", serde_json::to_string(&course).unwrap()).unwrap();
        }
      }
    }
    println!("{}: {total} courses so far", scenario.name);
  }
}

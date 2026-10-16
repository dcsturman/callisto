//! The environment's two promises: any choice the masks allow is a legal
//! game, and a baseline's orders mean the same through the action space as
//! given straight.

use std::sync::Arc;

use ai_ships::commander::{Close, Commander};
use callisto::entity::Entity;
use callisto_rl::action::{choice_of, Choice, Head, HEADS, MASK_LEN};
use callisto_rl::batch::{Batch, Config, Outputs};
use callisto_rl::encode::OBS_LEN;
use callisto_rl::game::{Control, Game, Scenario, Step};
use rand::{Rng, SeedableRng};

const DESIGNS: &str = "../callisto/ship_templates";

/// The training pool.
const POOL: [&str; 4] = [
  "../callisto/scenarios/Tessaly Crossing.json",
  "../callisto/scenarios/jupiter.json",
  "../callisto/scenarios/Marduk Encounter.json",
  "../callisto/scenarios/Thebus.json",
];

/// A choice drawn at random from what the mask allows, head by head.
fn random_choice(mask: &[bool], rng: &mut impl Rng) -> Choice {
  let mut choice: Choice = [0; HEADS];
  for (index, head) in Head::ALL.iter().enumerate() {
    let open: Vec<usize> = (0..head.size()).filter(|choice| mask[head.offset() + choice]).collect();
    assert!(!open.is_empty(), "{head:?} has no open choice");
    choice[index] = i64::try_from(open[rng.gen_range(0..open.len())]).unwrap();
  }
  choice
}

#[test]
fn random_play_is_legal() {
  let config = Config {
    scenarios: POOL.iter().map(ToString::to_string).collect(),
    designs: DESIGNS.to_string(),
    games: 8,
    max_rounds: 30,
    opponent: None,
    seed: 1,
    threads: 4,
    random_goals: 0.5,
  };
  let mut batch = Batch::new(&config).expect("the pool loads");
  let slots = batch.slots();
  let mut obs = vec![0.0; slots * OBS_LEN];
  let mut masks = vec![false; slots * MASK_LEN];
  batch.observe(&mut obs, &mut masks);
  let mut rng = rand::rngs::SmallRng::seed_from_u64(7);
  let mut finished = Vec::new();
  for _ in 0..80 {
    let choices: Vec<i64> = masks.chunks(MASK_LEN).flat_map(|mask| random_choice(mask, &mut rng)).collect();
    let mut rewards = vec![0.0; slots];
    let mut dones = vec![false; slots];
    let mut terminal = vec![0.0; slots * OBS_LEN];
    let mut out = Outputs {
      obs: &mut obs,
      masks: &mut masks,
      rewards: &mut rewards,
      dones: &mut dones,
      terminal_obs: &mut terminal,
    };
    finished.extend(batch.step(&choices, &mut out));
    assert!(obs.iter().all(|x| x.is_finite() && x.abs() <= 1.5), "observations are scaled");
    assert!(rewards.iter().all(|r| r.is_finite() && r.abs() <= 1.5), "rewards are bounded");
  }
  assert!(!finished.is_empty(), "some games finish in 80 rounds");
  for game in &finished {
    let total: f32 = game.returns.iter().sum::<f32>();
    println!(
      "{} seed {}: {:?} in {} rounds, returns {:?}",
      game.scenario, game.seed, game.end, game.rounds, game.returns
    );
    assert!(
      game.returns.iter().all(|r| r.abs() <= 1.5),
      "a side's return is its goal's score and a little shaping: {:?} ({total})",
      game.returns
    );
  }
}

fn load(path: &str) -> Arc<Scenario> {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  runtime.block_on(async {
    callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
    Arc::new(Scenario::load(path).await.unwrap())
  })
}

/// Play a game with `close` on the first side, given either through the
/// action space or straight, against `close` on the second.
fn play_close(scenario: &Arc<Scenario>, seed: u64, through_choices: bool) -> Vec<Step> {
  let controls = [Control::Agent, Control::Baseline("close".to_string())];
  let mut game = Game::new(Arc::clone(scenario), &controls, seed, 40);
  let mut close = Close;
  let mut steps = Vec::new();
  loop {
    let slots = game.slots().len();
    let mut obs = vec![0.0; slots * OBS_LEN];
    let mut masks = vec![false; slots * MASK_LEN];
    game.observe_into(&mut obs, &mut masks);
    let intents = futures::executor::block_on(close.decide(game.view(0)));
    let step = if through_choices {
      let choices: Vec<Choice> = game
        .slots()
        .iter()
        .map(|slot| {
          let ship = game.view(0).own.iter().find(|ship| ship.get_name() == slot.ship);
          let intent = intents.iter().find(|(name, _)| *name == slot.ship);
          match (ship, intent) {
            (Some(ship), Some((_, intent))) => choice_of(intent, ship, slot.picture()),
            _ => [0; HEADS],
          }
        })
        .collect();
      game.step(&choices)
    } else {
      game.step_intents(&intents)
    };
    steps.push(step);
    if step.end.is_some() {
      return steps;
    }
  }
}

#[test]
fn close_plays_the_same_through_the_action_space() {
  for path in [POOL[0], POOL[2]] {
    let scenario = load(path);
    for seed in 1..=4 {
      let straight = play_close(&scenario, seed, false);
      let through = play_close(&scenario, seed, true);
      assert_eq!(straight, through, "{path} seed {seed}");
      let last = straight.last().unwrap();
      println!("{path} seed {seed}: {} rounds, {:?}", straight.len(), last.end);
    }
  }
}

#[test]
fn each_goal_scores_as_agreed() {
  use callisto::ship::Goal;
  use callisto_rl::game::{terminal, Ending, Tally};
  // Hull kept below half unless a test says otherwise, so cowardice is not in play.
  let tally = |dealt: f32, jumped: f32, holding: bool| Tally {
    dealt,
    jumped,
    holding,
    kept: 0.4,
  };
  let none = tally(0.0, 0.0, false);
  // Destroy: running is nearly as bad as dying; a draw is as bad.
  assert!((terminal(Goal::Destroy, Ending::EnemyFled, &none) - 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Destroy, Ending::WeFled, &none) + 0.9).abs() < 1e-6);
  assert!((terminal(Goal::Destroy, Ending::Draw, &none) + 1.0).abs() < 1e-6);
  // Hold: a draw is good only while still holding; running is failure.
  assert!((terminal(Goal::Hold, Ending::Draw, &tally(0.0, 0.0, true)) - 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Hold, Ending::Draw, &none) + 1.0).abs() < 1e-6);
  assert!((terminal(Goal::Hold, Ending::EnemyFled, &none) - 1.0).abs() < 1e-6);
  // Raid: the damage is the point, however the game ends.
  assert!((terminal(Goal::Raid, Ending::WeFled, &tally(0.3, 0.0, false)) - 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Raid, Ending::WeFled, &tally(0.2, 0.0, false)) + 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Raid, Ending::EnemyFled, &tally(0.2, 0.0, false)) + 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Raid, Ending::Draw, &tally(0.2, 0.0, false)) + 1.0).abs() < 1e-6);
  // Escape: only jumping out counts.
  assert!((terminal(Goal::Escape, Ending::WeFled, &tally(0.0, 1.0, false)) - 1.0).abs() < 1e-6);
  assert!((terminal(Goal::Escape, Ending::WeFled, &none) + 1.0).abs() < 1e-6);
  assert!(terminal(Goal::Escape, Ending::WeDestroyed, &tally(0.0, 0.5, false)).abs() < 1e-6);
  // Cowardice: running with more than half the hull left, unless leaving is the goal.
  let healthy = Tally { kept: 0.8, ..none };
  assert!((terminal(Goal::Destroy, Ending::WeFled, &healthy) + 1.1).abs() < 1e-6);
  assert!((terminal(Goal::Hold, Ending::WeFled, &healthy) + 1.1).abs() < 1e-6);
  assert!((terminal(Goal::Raid, Ending::WeFled, &healthy) + 1.1).abs() < 1e-6);
  assert!((terminal(Goal::Raid, Ending::WeFled, &Tally { dealt: 0.3, ..healthy }) - 0.6).abs() < 1e-6);
  assert!((terminal(Goal::Escape, Ending::WeFled, &Tally { jumped: 1.0, ..healthy }) - 1.0).abs() < 1e-6);
}

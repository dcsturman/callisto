//! The environment's two promises: any choice the masks allow is a legal
//! game, and a baseline's orders mean the same through the action space as
//! given straight.

use std::sync::Arc;

use ai_ships::commander::{Close, Commander};
use callisto::entity::Entity;
use callisto::tactics::Posture;
use callisto_rl::action::{choice_of, side_intents, Choice, Head, HEADS, MASK_LEN};
use callisto_rl::batch::{Batch, Config, Outputs};
use callisto_rl::encode::OBS_LEN;
use callisto_rl::game::{Control, Game, Scenario};
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
    random_goals: 0.3,
    destroy_goals: 0.4,
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

/// Every round of a game of `close` against `close`, `close`'s orders for the
/// first side put through the action heads and back must keep its decisions:
/// helm, fire, lock, posture, dodge and assist. The heads add what `close`
/// never orders (the captain's boosts, every engineer repairing), so whole
/// games are no longer identical; the decisions must be.
fn close_survives_the_heads(scenario: &Arc<Scenario>, seed: u64) -> usize {
  let controls = [Control::Agent, Control::Baseline("close".to_string())];
  let mut game = Game::new(Arc::clone(scenario), &controls, seed, 40);
  let mut close = Close;
  let mut checked = 0;
  loop {
    let slots = game.slots().len();
    let mut obs = vec![0.0; slots * OBS_LEN];
    let mut masks = vec![false; slots * MASK_LEN];
    game.observe_into(&mut obs, &mut masks);
    let view = game.view(0).clone();
    let intents = futures::executor::block_on(close.decide(&view));
    let decided: Vec<(&callisto::ship::Ship, Choice, callisto_rl::encode::Picture)> = game
      .slots()
      .iter()
      .filter_map(|slot| {
        let ship = view.own.iter().find(|ship| ship.get_name() == slot.ship)?;
        let (_, intent) = intents.iter().find(|(name, _)| *name == slot.ship)?;
        Some((ship, choice_of(intent, ship, slot.picture()), slot.picture().clone()))
      })
      .collect();
    let own: Vec<_> = decided.iter().map(|(ship, choice, picture)| (*ship, choice, picture)).collect();
    for (name, back) in side_intents(&own, &view) {
      let (_, given) = intents.iter().find(|(ship, _)| *ship == name).unwrap();
      let ship = view.own.iter().find(|ship| ship.get_name() == name).unwrap();
      assert_eq!(back.helm, given.helm, "{name}: helm");
      assert_eq!(back.fire, given.fire, "{name}: fire");
      assert_eq!(back.sensors, given.sensors, "{name}: lock");
      let posture_now = callisto_rl::encode::posture_of(ship);
      let posture = given.posture.map_or(posture_now, |p| {
        [Posture::Silent, Posture::Passive, Posture::Active]
          .iter()
          .position(|q| *q == p)
          .unwrap()
      });
      assert_eq!(
        back.posture,
        Some([Posture::Silent, Posture::Passive, Posture::Active][posture]),
        "{name}: posture"
      );
      assert_eq!(
        back.evade,
        Some(given.evade.unwrap_or_else(|| ship.get_dodge_thrust())),
        "{name}: dodge"
      );
      assert_eq!(
        back.assist_gunners,
        Some(given.assist_gunners.unwrap_or_else(|| ship.get_assist_gunners())),
        "{name}: assist"
      );
      checked += 1;
    }
    let step = game.step_intents(&intents);
    if step.end.is_some() {
      return checked;
    }
  }
}

#[test]
fn close_keeps_its_decisions_through_the_heads() {
  for path in [POOL[0], POOL[2]] {
    let scenario = load(path);
    for seed in 1..=4 {
      let checked = close_survives_the_heads(&scenario, seed);
      assert!(checked > 0, "{path} seed {seed}: nothing checked");
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

#[test]
fn random_whole_side_play_is_legal() {
  use callisto_rl::side::{HEAD_SIZES, MASK_LEN as SIDE_MASK, OWN};
  use callisto_rl::side_batch::{SideBatch, Views, WIDTHS};
  let config = Config {
    scenarios: POOL.iter().map(ToString::to_string).collect(),
    designs: DESIGNS.to_string(),
    games: 8,
    max_rounds: 30,
    opponent: None,
    seed: 3,
    threads: 4,
    random_goals: 0.3,
    destroy_goals: 0.4,
  };
  let mut batch = SideBatch::new(&config).expect("the pool loads");
  let entries = 2 * batch.games();
  let mut own = vec![0.0; entries * WIDTHS[0]];
  let mut foes = vec![0.0; entries * WIDTHS[1]];
  let mut side = vec![0.0; entries * WIDTHS[2]];
  let mut pairs = vec![0.0; entries * WIDTHS[3]];
  let mut masks = vec![false; entries * WIDTHS[4]];
  let mut views = Views {
    own: &mut own,
    foes: &mut foes,
    side: &mut side,
    pairs: &mut pairs,
    masks: &mut masks,
  };
  batch.observe(&mut views);
  let mut rng = rand::rngs::SmallRng::seed_from_u64(5);
  let mut finished = 0;
  for _ in 0..80 {
    // A random open choice per head, for every own-ship row of every entry.
    let mut choices = Vec::with_capacity(entries * OWN * HEADS);
    for mask in views.masks.chunks(SIDE_MASK) {
      let mut at = 0;
      for size in HEAD_SIZES {
        let open: Vec<usize> = (0..size).filter(|c| mask[at + c]).collect();
        assert!(!open.is_empty(), "every head has a choice");
        choices.push(i64::try_from(open[rng.gen_range(0..open.len())]).unwrap());
        at += size;
      }
    }
    let mut rewards = vec![0.0; entries];
    let mut dones = vec![false; entries];
    finished += batch.step(&choices, &mut views, &mut rewards, &mut dones).len();
    for buffer in [&*views.own, &*views.foes, &*views.side, &*views.pairs] {
      assert!(buffer.iter().all(|x| x.is_finite() && x.abs() <= 1.5), "views are scaled");
    }
  }
  assert!(finished > 0, "games finish");
}

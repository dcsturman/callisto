//! One game of a scenario, between two sides, a round at a time: the unit
//! the batch steps.
//!
//! A side is played either by the network (its ships are *slots*, which take
//! a [`Choice`] each round) or by one of the baseline commanders, run here in
//! Rust. Orders go through `callisto::tactics` and are applied as the backend
//! applies a console's, exactly as in the harness, so a game here is a game
//! there.
//!
//! The reward is the objective in `docs/rl_plan.md`: destroy the other side
//! (+1) or drive it off (+0.6, judged by [`Retreats`], as the harness judges
//! it); be destroyed (-1) or run (-0.6). A draw -- the round limit with both
//! still in it, or both running at once -- counts as a loss for both (-1), so
//! stalling is never a goal and a side outmatched does better to leave. A small
//! shaping term each round pays for hull taken and charges for hull lost and
//! for time.

use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use ai_ships::commander::{self, Commander};
use ai_ships::harness::{apply, hull_of, sides, Retreats};
use ai_ships::observation::{observe, Observation};
use callisto::authentication::MockAuthenticator;
use callisto::entity::{Entities, Entity, Vec3};
use callisto::player::PlayerManager;
use callisto::server::Server;
use callisto::ship::{Goal, Ship};
use callisto::tactics::{self, EngineerVerb, Intent};
use cgmath::InnerSpace;
use rand::{Rng, SeedableRng};

use crate::action::{side_intents, Choice, MASK_LEN};
use crate::encode::{encode_absent, encode_ship, Context, Memory, Picture, Seen, MAX_SHIPS, OBS_LEN};

/// A scenario loaded once and played many times.
pub struct Scenario {
  pub name: String,
  pub entities: Entities,
  /// The two sides, in name order, and their ships.
  pub sides: [(String, Vec<String>); 2],
}

impl Scenario {
  /// Load a scenario file. Designs must already be loaded.
  ///
  /// # Errors
  /// Returns an error if the file cannot be read, or the scenario does not
  /// have exactly two sides of at most [`MAX_SHIPS`] ships.
  pub async fn load(path: &str) -> Result<Self, String> {
    let entities = Entities::load_from_file_with_ship_templates(path, callisto::ship::get_ship_templates_snapshot())
      .await
      .map_err(|e| format!("{path}: {e}"))?;
    let name = std::path::Path::new(path)
      .file_stem()
      .map_or_else(|| path.to_string(), |stem| stem.to_string_lossy().into_owned());
    let found: Vec<(String, Vec<String>)> = sides(&entities).into_iter().collect();
    let [a, b]: [(String, Vec<String>); 2] = found
      .try_into()
      .map_err(|found: Vec<_>| format!("{path}: {} sides, not two", found.len()))?;
    if let Some((side, ships)) = [&a, &b].into_iter().find(|(_, ships)| ships.len() > MAX_SHIPS) {
      return Err(format!("{path}: side {side} has {} ships, more than {MAX_SHIPS}", ships.len()));
    }
    Ok(Scenario {
      name,
      entities,
      sides: [a, b],
    })
  }
}

/// Who plays a side.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
  /// The network: every ship is a slot.
  Agent,
  /// A baseline commander, by name (`close`, `hold`, `idle`, `random`).
  Baseline(String),
}

/// How a game ended, from the first side's point of view where it matters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum End {
  /// This side has no ship left in play, and none of them jumped.
  Destroyed { side: usize },
  /// This side ran: its ships jumped, or opened the range past Distant and
  /// kept going while the other side could still fight.
  Fled { side: usize },
  /// Both sides wiped out, or both ran, or the round limit: scored on hull.
  Draw,
}

/// How a game ended, for one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ending {
  EnemyDestroyed,
  EnemyFled,
  /// The round limit with both still in it, both running at once, or both
  /// wiped out.
  Draw,
  WeFled,
  WeDestroyed,
}

/// What a side did over a game, for the goals that need more than how it
/// ended.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tally {
  /// The share of the enemy's hull destroyed.
  pub dealt: f32,
  /// The share of the side's ships that jumped out.
  pub jumped: f32,
  /// At least one of the side's ships is within [`HOLD_RADIUS`] of where the
  /// side started.
  pub holding: bool,
  /// The share of its own hull the side has left, ships that jumped out
  /// counted at the hull they left with.
  pub kept: f32,
}

/// How far from where it started a side may be and still hold its position:
/// the edge of Long range.
pub const HOLD_RADIUS: f64 = 25_000_000.0;
/// The share of the enemy's hull a raid must destroy to have done its job.
pub const RAID_DAMAGE: f32 = 0.25;
/// Running while above this share of its hull, when the goal is not to run,
/// is cowardice: it scores [`COWARDICE`], worse than being destroyed.
pub const COWARD_HULL: f32 = 0.5;
pub const COWARDICE: f32 = -1.1;

/// The end-of-game reward for one side, by its goal.
///
/// | Outcome          | Destroy | Hold                    | Raid                          | Escape      |
/// |------------------|---------|-------------------------|-------------------------------|-------------|
/// | Enemy destroyed  | +1      | +1                      | +1                            | +1          |
/// | Enemy fled       | +0.6    | +1                      | +0.6 if 25% dealt, else -0.6  | share jumped |
/// | Draw             | -1      | +0.6 if holding, else -1 | +0.6 if 25% dealt, else -1   | share jumped |
/// | We fled          | -0.9    | -1                      | +0.6 if 25% dealt, else -0.6  | share jumped |
/// | ...above half hull | -1.1  | -1.1                    | +0.6 if 25% dealt, else -1.1  | share jumped |
/// | We were destroyed| -1      | -1                      | -1                            | share jumped |
///
/// "Share jumped" runs from -1 (none) to +1 (every ship jumped out). Under
/// Destroy, running is nearly as bad as dying: leaving is for goals that
/// allow it. Running with more than half its hull left, unless leaving is
/// the goal or the raid is done, is cowardice, and worse than dying.
#[must_use]
pub fn terminal(goal: Goal, ending: Ending, tally: &Tally) -> f32 {
  let coward = ending == Ending::WeFled && tally.kept > COWARD_HULL;
  match goal {
    Goal::Destroy => match ending {
      Ending::EnemyDestroyed => 1.0,
      Ending::EnemyFled => 0.6,
      Ending::WeFled if coward => COWARDICE,
      Ending::WeFled => -0.9,
      Ending::Draw | Ending::WeDestroyed => -1.0,
    },
    Goal::Hold => match ending {
      Ending::EnemyDestroyed | Ending::EnemyFled => 1.0,
      Ending::Draw if tally.holding => 0.6,
      Ending::WeFled if coward => COWARDICE,
      Ending::Draw | Ending::WeFled | Ending::WeDestroyed => -1.0,
    },
    Goal::Raid => {
      let done = tally.dealt >= RAID_DAMAGE;
      match ending {
        Ending::EnemyDestroyed => 1.0,
        Ending::WeDestroyed => -1.0,
        _ if done => 0.6,
        Ending::WeFled if coward => COWARDICE,
        Ending::EnemyFled | Ending::WeFled => -0.6,
        Ending::Draw => -1.0,
      }
    }
    Goal::Escape => match ending {
      Ending::EnemyDestroyed => 1.0,
      _ => 2.0 * tally.jumped - 1.0,
    },
  }
}

/// A side's goal: the one most of its ships carry, Destroy if none do.
pub fn side_goal<'a>(ships: impl IntoIterator<Item = &'a Ship>) -> Goal {
  let mut counts: HashMap<Goal, usize> = HashMap::new();
  for ship in ships {
    *counts.entry(ship.goal.unwrap_or_default()).or_default() += 1;
  }
  crate::encode::GOALS
    .into_iter()
    .max_by_key(|goal| counts.get(goal).copied().unwrap_or(0))
    .filter(|goal| counts.contains_key(goal))
    .unwrap_or_default()
}

/// The goals a side can be given at random: Escape only if every ship has a
/// jump drive.
fn goals_open_to(ships: &[&Ship]) -> Vec<Goal> {
  let mut goals = vec![Goal::Destroy, Goal::Hold, Goal::Raid];
  if ships.iter().all(|ship| ship.design.jump > 0) {
    goals.push(Goal::Escape);
  }
  goals
}

/// Shaping, per round: this times hull share taken less hull share lost.
pub const SHAPING: f32 = 0.1;
/// Shaping, per round: the cost of time.
pub const ROUND_COST: f32 = 0.002;

/// What a round brought each side.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Step {
  pub rewards: [f32; 2],
  /// Set when the game is over.
  pub end: Option<End>,
}

struct Side {
  name: String,
  ships: Vec<String>,
  control: Control,
  commander: Option<Box<dyn Commander>>,
  memory: Memory,
  hull_at_start: u32,
  hull_last: u32,
  /// What the side is trying to do.
  goal: Goal,
  /// The middle of the side's ships at the start, for Hold.
  start: Vec3,
}

/// One slot: an agent ship, and the picture her last observation was made
/// from.
#[derive(Debug, Clone)]
pub struct Slot {
  pub side: usize,
  pub ship: String,
  picture: Picture,
}

impl Slot {
  /// Which ship was in which slot when this one last looked.
  #[must_use]
  pub fn picture(&self) -> &Picture {
    &self.picture
  }
}

/// One game in progress.
pub struct Game {
  scenario: Arc<Scenario>,
  player: PlayerManager,
  sides: [Side; 2],
  slots: Vec<Slot>,
  /// Ships that jumped out, and the hull they left with.
  departed: HashMap<String, u32>,
  /// The world and each side's view as last observed: what this round's
  /// orders are given against.
  world: Entities,
  views: [Observation; 2],
  pub rounds: u32,
  pub max_rounds: u32,
  pub seed: u64,
  /// Baselines playing network sides; see [`Game::stand_in`].
  stand_ins: [Option<String>; 2],
  /// Who has run, as the harness judges it.
  retreats: Retreats,
  rosters: BTreeMap<String, Vec<String>>,
  /// The share of games where each side's goal is drawn at random.
  random_goals: f32,
}

fn player_for(scenario: &Scenario, seed: u64) -> PlayerManager {
  let server = Arc::new(Server::from_scenario(&format!("rl-{seed}"), &scenario.entities));
  PlayerManager::new(Some(server), Box::new(MockAuthenticator::new("")), true).with_seed(seed)
}

fn entities(player: &PlayerManager) -> Entities {
  player.get_entities().expect("the game's world can be read")
}

fn present(world: &Entities, ships: &[String]) -> Vec<String> {
  ships.iter().filter(|ship| world.ships.contains_key(*ship)).cloned().collect()
}

impl Game {
  /// A new game of `scenario`, with `controls` for its two sides in name order.
  ///
  /// # Panics
  /// Panics if a baseline is not one the commander module knows.
  #[must_use]
  pub fn new(scenario: Arc<Scenario>, controls: &[Control; 2], seed: u64, max_rounds: u32) -> Self {
    let player = player_for(&scenario, seed);
    let world = entities(&player);
    let sides = [0, 1].map(|index| {
      let (name, ships) = &scenario.sides[index];
      let control = controls[index].clone();
      let commander = match &control {
        Control::Agent => None,
        Control::Baseline(name) => Some(commander::named(name, seed).unwrap_or_else(|| panic!("no baseline {name}"))),
      };
      let hull = hull_of(&world, ships);
      let own: Vec<Ship> = ships
        .iter()
        .filter_map(|ship| world.ships.get(ship))
        .map(|ship| ship.read().unwrap().clone())
        .collect();
      let start = own
        .iter()
        .map(Entity::get_position)
        .fold(Vec3::new(0.0, 0.0, 0.0), |sum, at| sum + at)
        / own.len().max(1) as f64;
      Side {
        name: name.clone(),
        ships: ships.clone(),
        control,
        commander,
        memory: Memory::new(),
        hull_at_start: hull,
        hull_last: hull,
        goal: side_goal(&own),
        start,
      }
    });
    let slots = sides
      .iter()
      .enumerate()
      .filter(|(_, side)| side.control == Control::Agent)
      .flat_map(|(index, side)| {
        side.ships.iter().map(move |ship| Slot {
          side: index,
          ship: ship.clone(),
          picture: Picture::default(),
        })
      })
      .collect();
    let views = [0, 1].map(|index| observe(&world, &present(&world, &sides[index].ships)));
    let scenario_rosters: BTreeMap<String, Vec<String>> = scenario.sides.iter().cloned().collect();
    Game {
      scenario,
      player,
      sides,
      slots,
      departed: HashMap::new(),
      world,
      views,
      rounds: 0,
      max_rounds,
      seed,
      stand_ins: [None, None],
      retreats: Retreats::default(),
      rosters: scenario_rosters,
      random_goals: 0.0,
    }
  }

  /// In this share of games, from now on, each side gets a goal drawn at
  /// random instead of the scenario's, so the network learns what the goals
  /// mean rather than which one each scenario has.
  #[must_use]
  pub fn with_random_goals(mut self, share: f32) -> Self {
    self.random_goals = share;
    self.draw_goals();
    self
  }

  fn draw_goals(&mut self) {
    let mut rng = rand::rngs::SmallRng::seed_from_u64(self.seed ^ 0x6F41_5EED);
    if rng.gen::<f32>() >= self.random_goals {
      return;
    }
    for side in &mut self.sides {
      let ships: Vec<&Ship> = self
        .views
        .iter()
        .flat_map(|view| view.own.iter())
        .filter(|ship| side.ships.iter().any(|name| name == ship.get_name()))
        .collect();
      let open = goals_open_to(&ships);
      side.goal = open[rng.gen_range(0..open.len())];
    }
  }

  /// Each side's goal.
  #[must_use]
  pub fn goals(&self) -> [Goal; 2] {
    [self.sides[0].goal, self.sides[1].goal]
  }

  /// The same scenario and sides again, from the start, with a new seed.
  pub fn reset(&mut self, seed: u64) {
    let controls = [self.sides[0].control.clone(), self.sides[1].control.clone()];
    let stand_ins = self.stand_ins.clone();
    let random_goals = self.random_goals;
    *self = Game::new(Arc::clone(&self.scenario), &controls, seed, self.max_rounds).with_random_goals(random_goals);
    for (side, stand_in) in stand_ins.into_iter().enumerate() {
      self.stand_in(side, stand_in);
    }
  }

  /// Hand a network side to a baseline, by name, or back to the network
  /// with `None`. The side's slots stay, so the batch keeps its shape; their
  /// choices are ignored while a baseline plays. Kept across resets.
  ///
  /// # Panics
  /// Panics if the baseline is not one the commander module knows.
  pub fn stand_in(&mut self, side: usize, baseline: Option<String>) {
    let commander = match (&self.sides[side].control, &baseline) {
      (Control::Baseline(name), None) => {
        Some(commander::named(name, self.seed).unwrap_or_else(|| panic!("no baseline {name}")))
      }
      (_, Some(name)) => Some(commander::named(name, self.seed).unwrap_or_else(|| panic!("no baseline {name}"))),
      (Control::Agent, None) => None,
    };
    self.sides[side].commander = commander;
    self.stand_ins[side] = baseline;
  }

  /// The baseline standing in for each side, if any.
  #[must_use]
  pub fn stand_ins(&self) -> &[Option<String>; 2] {
    &self.stand_ins
  }

  #[must_use]
  pub fn slots(&self) -> &[Slot] {
    &self.slots
  }

  #[must_use]
  pub fn scenario(&self) -> &Scenario {
    &self.scenario
  }

  #[must_use]
  pub fn side_name(&self, side: usize) -> &str {
    &self.sides[side].name
  }

  /// The world as it stands.
  #[must_use]
  pub fn world(&self) -> &Entities {
    &self.world
  }

  /// One side's view, as last observed.
  #[must_use]
  pub fn view(&self, side: usize) -> &Observation {
    &self.views[side]
  }

  /// Hull left, as a share of the start, for each side: the true figure.
  #[must_use]
  pub fn hull_shares(&self) -> [f32; 2] {
    [0, 1].map(|side| {
      let side = &self.sides[side];
      side.hull_last as f32 / side.hull_at_start.max(1) as f32
    })
  }

  /// Write every slot's observation into `obs` (slots × [`OBS_LEN`]) and mask
  /// into `masks` (slots × [`MASK_LEN`]), and remember what each side has seen.
  ///
  /// # Panics
  /// Panics if the buffers are not the slots' size.
  pub fn observe_into(&mut self, obs: &mut [f32], masks: &mut [bool]) {
    assert_eq!(obs.len(), self.slots.len() * OBS_LEN);
    assert_eq!(masks.len(), self.slots.len() * MASK_LEN);
    for side in 0..2 {
      remember(&mut self.sides[side].memory, &self.views[side], self.rounds);
    }
    let shares = self.hull_shares();
    for (slot, (out, mask)) in self
      .slots
      .iter_mut()
      .zip(obs.chunks_mut(OBS_LEN).zip(masks.chunks_mut(MASK_LEN)))
    {
      let view = &self.views[slot.side];
      let Some(me) = view.own.iter().find(|ship| ship.get_name() == slot.ship) else {
        encode_absent(out, mask);
        slot.picture = Picture::default();
        continue;
      };
      let memory = &self.sides[slot.side].memory;
      let ctx = Context {
        view,
        memory,
        round: self.rounds,
        max_rounds: self.max_rounds,
        own_hull_share: shares[slot.side],
        enemy_hull_share: believed_hull(memory),
        goal: self.sides[slot.side].goal,
      };
      slot.picture = encode_ship(&ctx, me, out, mask);
    }
  }

  /// Play a round: the slots' `choices` (one per slot, in slot order) and the
  /// baselines' orders, all given against the last observation, then resolve.
  ///
  /// # Panics
  /// Panics if `choices` is not one per slot.
  pub fn step(&mut self, choices: &[Choice]) -> Step {
    assert_eq!(choices.len(), self.slots.len());
    let mut orders: Vec<(String, Intent)> = Vec::new();
    for side in 0..2 {
      let view = &self.views[side];
      let intents = if let Some(commander) = self.sides[side].commander.as_mut() {
        futures::executor::block_on(commander.decide(view))
      } else {
        let own: Vec<_> = self
          .slots
          .iter()
          .zip(choices)
          .filter(|(slot, _)| slot.side == side)
          .filter_map(|(slot, choice)| {
            view
              .own
              .iter()
              .find(|ship| ship.get_name() == slot.ship)
              .map(|ship| (ship, choice, &slot.picture))
          })
          .collect();
        side_intents(&own, view)
      };
      let ours = &view.own;
      orders.extend(
        intents
          .into_iter()
          .filter(|(ship, _)| ours.iter().any(|own| own.get_name() == ship)),
      );
    }
    self.play(&orders)
  }

  /// Give these orders and resolve the round.
  fn play(&mut self, orders: &[(String, Intent)]) -> Step {
    for (ship, intent) in orders {
      for request in tactics::orders(&self.world, ship, intent).unwrap_or_default() {
        apply(&self.player, request);
      }
    }
    let jumping: Vec<&str> = orders
      .iter()
      .filter(|(_, intent)| intent.engineering.iter().any(|verb| matches!(verb, EngineerVerb::Jump)))
      .map(|(ship, _)| ship.as_str())
      .collect();
    let before = std::mem::take(&mut self.world);
    let _ = self.player.update();
    self.rounds += 1;
    self.world = entities(&self.player);
    for ship in jumping {
      if !self.world.ships.contains_key(ship) {
        let hull = before.ships.get(ship).map_or(0, |ship| ship.read().unwrap().current_hull);
        self.departed.insert(ship.to_string(), hull);
      }
    }
    self.views = [0, 1].map(|side| observe(&self.world, &present(&self.world, &self.sides[side].ships)));
    self.score()
  }

  fn score(&mut self) -> Step {
    let hulls = [0, 1].map(|side| {
      let ships = &self.sides[side].ships;
      hull_of(&self.world, ships) + ships.iter().filter_map(|ship| self.departed.get(ship)).sum::<u32>()
    });
    let lost = [0, 1].map(|side| {
      let side_state = &self.sides[side];
      side_state.hull_last.saturating_sub(hulls[side]) as f32 / side_state.hull_at_start.max(1) as f32
    });
    let mut rewards = [0, 1].map(|side| SHAPING * (lost[1 - side] - lost[side]) - ROUND_COST);
    for (side, hull) in self.sides.iter_mut().zip(hulls) {
      side.hull_last = hull;
    }

    let end = self.end();
    if let Some(end) = end {
      for (side, reward) in rewards.iter_mut().enumerate() {
        let ending = match end {
          End::Destroyed { side: loser } if loser == side => Ending::WeDestroyed,
          End::Destroyed { .. } => Ending::EnemyDestroyed,
          End::Fled { side: runner } if runner == side => Ending::WeFled,
          End::Fled { .. } => Ending::EnemyFled,
          End::Draw => Ending::Draw,
        };
        *reward += terminal(self.sides[side].goal, ending, &self.tally(side));
      }
    }
    Step { rewards, end }
  }

  /// What a side has done so far, for its goal's score.
  ///
  /// # Panics
  /// Panics if a ship's lock is poisoned.
  #[must_use]
  pub fn tally(&self, side: usize) -> Tally {
    let enemy = &self.sides[1 - side];
    let ours = &self.sides[side];
    let jumped = ours.ships.iter().filter(|ship| self.departed.contains_key(*ship)).count();
    let holding = ours
      .ships
      .iter()
      .filter_map(|ship| self.world.ships.get(ship))
      .any(|ship| (ship.read().unwrap().get_position() - ours.start).magnitude() <= HOLD_RADIUS);
    Tally {
      dealt: 1.0 - enemy.hull_last as f32 / enemy.hull_at_start.max(1) as f32,
      jumped: jumped as f32 / ours.ships.len().max(1) as f32,
      holding,
      kept: ours.hull_last as f32 / ours.hull_at_start.max(1) as f32,
    }
  }

  /// Whether the game is over, and how.
  fn end(&mut self) -> Option<End> {
    let in_play = [0, 1].map(|side| present(&self.world, &self.sides[side].ships));
    let gone = in_play.each_ref().map(Vec::is_empty);
    if gone[0] || gone[1] {
      if gone[0] && gone[1] {
        return Some(End::Draw);
      }
      let side = usize::from(gone[1]);
      let jumped = self.sides[side].ships.iter().any(|ship| self.departed.contains_key(ship));
      return Some(if jumped {
        End::Fled { side }
      } else {
        End::Destroyed { side }
      });
    }

    // Who has run, by the rule the harness ends games by.
    let fled = self.retreats.update(&self.world, &self.rosters);
    let ran = [0, 1].map(|side| fled.contains(&self.sides[side].name));
    match ran {
      [true, true] => return Some(End::Draw),
      [true, false] => return Some(End::Fled { side: 0 }),
      [false, true] => return Some(End::Fled { side: 1 }),
      [false, false] => {}
    }
    (self.rounds >= self.max_rounds).then_some(End::Draw)
  }

  /// Play a round with the slots' orders given directly, as intents: for
  /// checking that a commander's orders mean the same through the action
  /// space as given straight.
  pub fn step_intents(&mut self, slot_orders: &[(String, Intent)]) -> Step {
    let mut orders: Vec<(String, Intent)> = slot_orders.to_vec();
    for side in 0..2 {
      if let Some(commander) = self.sides[side].commander.as_mut() {
        orders.extend(futures::executor::block_on(commander.decide(&self.views[side])));
      }
    }
    self.play(&orders)
  }
}

/// Update a side's memory from its view: contacts as they are now; ships no
/// longer on the plot at all are gone, so their hull is none.
pub fn remember(memory: &mut Memory, view: &Observation, round: u32) {
  for contact in view.enemies() {
    memory.insert(
      contact.name.clone(),
      Seen {
        round,
        hull_share: contact.current_hull as f32 / contact.design_hull.max(1) as f32,
        design_hull: contact.design_hull,
        thrust: contact.current_maneuver,
      },
    );
  }
  let on_plot = |name: &str| {
    view.contacts.iter().any(|contact| contact.name == name) || view.blips.iter().any(|blip| blip.name == name)
  };
  for (name, seen) in memory.iter_mut() {
    if !on_plot(name) {
      seen.hull_share = 0.0;
    }
  }
}

/// The enemy's hull left as a side believes it: over the enemies it has
/// ever held, their last-seen hull against their design. All of it if none.
#[must_use]
pub fn believed_hull(memory: &Memory) -> f32 {
  let (left, whole) = memory.values().fold((0.0, 0.0), |(left, whole), seen| {
    let design = seen.design_hull as f32;
    (left + seen.hull_share * design, whole + design)
  });
  if whole > 0.0 {
    left / whole
  } else {
    1.0
  }
}

/// A game's slots and their sides, by side name: for reporting.
#[must_use]
pub fn roster(game: &Game) -> BTreeMap<String, Vec<String>> {
  let mut roster = BTreeMap::<String, Vec<String>>::new();
  for slot in game.slots() {
    roster
      .entry(game.side_name(slot.side).to_string())
      .or_default()
      .push(slot.ship.clone());
  }
  roster
}

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
use callisto::entity::{Entities, Entity};
use callisto::player::PlayerManager;
use callisto::server::Server;
use callisto::tactics::{self, EngineerVerb, Intent};

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

/// The end-of-game reward for each side.
pub const DESTROYED: f32 = 1.0;
pub const FLED: f32 = 0.6;
/// What a draw costs each side: the round limit with both still in it, both
/// running at once, or both wiped out. As much as being destroyed, so the
/// only ways out of a fight are winning it or leaving it: a weaker side that
/// keeps out of reach until the clock runs out has gained nothing by it.
pub const DRAW_COST: f32 = 1.0;
/// The most the hull difference adds to or takes from a draw: nothing.
pub const DRAW_SCALE: f32 = 0.0;
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
      Side {
        name: name.clone(),
        ships: ships.clone(),
        control,
        commander,
        memory: Memory::new(),
        hull_at_start: hull,
        hull_last: hull,
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
    }
  }

  /// The same scenario and sides again, from the start, with a new seed.
  pub fn reset(&mut self, seed: u64) {
    let controls = [self.sides[0].control.clone(), self.sides[1].control.clone()];
    let stand_ins = self.stand_ins.clone();
    *self = Game::new(Arc::clone(&self.scenario), &controls, seed, self.max_rounds);
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
        side_intents(&own)
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
      let shares = self.hull_shares();
      let terminal = match end {
        End::Destroyed { side } => {
          let mut terminal = [DESTROYED; 2];
          terminal[side] = -DESTROYED;
          terminal
        }
        End::Fled { side } => {
          let mut terminal = [FLED; 2];
          terminal[side] = -FLED;
          terminal
        }
        // A cost to both, and hull share taken less hull share lost, onto
        // ±DRAW_SCALE.
        End::Draw => [0, 1].map(|side| -DRAW_COST + DRAW_SCALE * ((1.0 - shares[1 - side]) - (1.0 - shares[side]))),
      };
      for (reward, terminal) in rewards.iter_mut().zip(terminal) {
        *reward += terminal;
      }
    }
    Step { rewards, end }
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

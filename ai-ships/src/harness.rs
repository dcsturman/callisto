//! The headless harness: play a scenario with a commander on each side, many
//! times, and score it.
//!
//! No WebSocket and no service: the scenario is loaded into a backend server
//! in-process and each round goes exactly as it would for a crew -- every
//! side's commander sees its own [`observe`] view, its intents become orders
//! through `callisto::tactics`, the orders are applied as the backend would
//! apply them from a console, and the round is resolved. Dice come from a
//! seed, so a game is reproducible and no two seeds alike.
//!
//! What it is for: measuring one commander against another (a win rate over
//! many seeds, with sides swapped), tuning, and producing the per-round log
//! that later training reads. Victory is the side still in play at the end,
//! having destroyed the other or driven it off (see [`Retreats`]); hull points
//! remaining and taken are the finer score.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use callisto::authentication::MockAuthenticator;
use callisto::entity::{Entities, Entity};
use callisto::payloads::RequestMsg;
use callisto::player::PlayerManager;
use callisto::server::Server;
use callisto::tactics::{self, Intent};
use serde::{Deserialize, Serialize};

use crate::commander::Commander;
use crate::observation::observe;

/// Picks each side's commander: given a side and the game's seed, the
/// commander's name and the commander itself, or `None` if there is none.
pub type CommanderPicker<'a> = dyn Fn(&str, u64) -> Option<(String, Box<dyn Commander>)> + 'a;

/// Which side a ship is on, as the backend groups AI ships: its team, or the
/// ship alone if it has none.
#[must_use]
pub fn side_of(team: Option<callisto::ship::Team>, ship: &str) -> String {
  team.map_or_else(|| format!("ship:{ship}"), |team| format!("{team:?}"))
}

/// Every side in the world and its ships, by name.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn sides(world: &Entities) -> BTreeMap<String, Vec<String>> {
  let mut sides = BTreeMap::<String, Vec<String>>::new();
  for ship in world.ships.values() {
    let ship = ship.read().unwrap();
    sides
      .entry(side_of(ship.team, ship.get_name()))
      .or_default()
      .push(ship.get_name().to_string());
  }
  for ships in sides.values_mut() {
    ships.sort();
  }
  sides
}

/// The hull these ships have left, counting none for a ship no longer in play.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn hull_of(world: &Entities, ships: &[String]) -> u32 {
  ships
    .iter()
    .filter_map(|name| world.ships.get(name))
    .map(|ship| ship.read().unwrap().current_hull)
    .sum()
}

/// A ship this far from every enemy is out of the fight, if she keeps going:
/// the edge of Distant.
pub const FLEE_DISTANCE: f64 = 50_000_000.0;
/// Rounds a side must keep running past [`FLEE_DISTANCE`] to have fled.
pub const FLEE_ROUNDS: u32 = 3;

/// Who has run from the fight. The rule training is scored by, so the harness
/// and the training environment end games alike.
///
/// Judged on true positions, not on either side's view. A side has fled when
/// every one of its ships has been beyond [`FLEE_DISTANCE`] of every enemy,
/// moving away from her nearest enemy and not burning back toward her, for
/// [`FLEE_ROUNDS`] rounds running, while an enemy can still fight. Range
/// opens for both sides alike, so who is running is told by each ship's own
/// motion and burn, in the scenario's frame: a chaser falling behind is still
/// heading in. And only a side that has come inside [`FLEE_DISTANCE`] of an
/// enemy can flee: one that opens the game far off has not run, it has not
/// arrived.
#[derive(Debug, Default, Clone)]
pub struct Retreats {
  running: BTreeMap<String, u32>,
  engaged: std::collections::BTreeSet<String>,
}

impl Retreats {
  /// Look at the world after a round and say which sides have now fled.
  ///
  /// # Panics
  /// Panics if a ship's lock is poisoned.
  pub fn update(&mut self, world: &Entities, rosters: &BTreeMap<String, Vec<String>>) -> Vec<String> {
    use cgmath::InnerSpace;
    let in_play = |ships: &[String]| -> Vec<String> {
      ships.iter().filter(|ship| world.ships.contains_key(*ship)).cloned().collect()
    };
    let mut fled = Vec::new();
    for (side, ships) in rosters {
      let ours = in_play(ships);
      let enemies: Vec<String> = rosters
        .iter()
        .filter(|(other, _)| *other != side)
        .flat_map(|(_, ships)| in_play(ships))
        .collect();
      if ours.is_empty() || enemies.is_empty() {
        self.running.remove(side);
        continue;
      }
      let mut out = true;
      for ship in &ours {
        let (here, velocity, burn) = {
          let ship = world.ships[ship].read().unwrap();
          (ship.get_position(), ship.get_velocity(), ship.plan.0 .0)
        };
        let towards = enemies
          .iter()
          .map(|enemy| world.ships[enemy].read().unwrap().get_position() - here)
          .min_by(|a, b| a.magnitude().total_cmp(&b.magnitude()))
          .expect("there are enemies");
        if towards.magnitude() <= FLEE_DISTANCE {
          self.engaged.insert(side.clone());
        }
        out &= towards.magnitude() > FLEE_DISTANCE && velocity.dot(towards) < 0.0 && burn.dot(towards) <= 0.0;
      }
      let running = self.running.entry(side.clone()).or_default();
      *running = if out && self.engaged.contains(side) {
        *running + 1
      } else {
        0
      };
      let enemy_can_fight = enemies
        .iter()
        .any(|enemy| !world.ships[enemy].read().unwrap().weapons().is_empty());
      if *running >= FLEE_ROUNDS && enemy_can_fight {
        fled.push(side.clone());
      }
    }
    fled
  }
}

/// How one side came out of a game.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SideResult {
  pub commander: String,
  pub ships_at_start: usize,
  pub ships_at_end: usize,
  pub hull_at_start: u32,
  pub hull_at_end: u32,
  /// What the side's model calls cost, in US dollars, for a commander that
  /// makes them at a known price.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub model_cost_usd: Option<f64>,
}

/// One game's result.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct Outcome {
  pub seed: u64,
  pub rounds: u32,
  /// The one side still in play, if only one is.
  pub winner: Option<String>,
  /// Every side still in it ran at once (see [`Retreats`]): the sides broke
  /// off, and nobody won.
  #[serde(default)]
  pub disengaged: bool,
  /// The sides that ran from the fight, by [`Retreats`].
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub fled: Vec<String>,
  pub sides: BTreeMap<String, SideResult>,
}

/// One line of the round log: what one side decided in one round. What
/// [`crate::commander::Replay`] reads back.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Record {
  pub seed: u64,
  pub round: u32,
  pub side: String,
  /// The commander's standing plan after deciding, if it keeps one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub plan: Option<String>,
  pub intents: Vec<(String, Intent)>,
}

/// Apply a console's request the way the backend's processor would.
pub fn apply(player: &PlayerManager, request: RequestMsg) {
  let result = match request {
    RequestMsg::SetPlan(plan) => player.set_plan(&plan).map(|_| ()),
    RequestMsg::ModifyActions(actions) => {
      player.merge_actions(actions);
      Ok(())
    }
    RequestMsg::SetShipEmissions(emissions) => player.set_ship_emissions(&emissions).map(|_| ()),
    RequestMsg::SetPilotActions(pilot) => player.set_pilot_actions(&pilot).map(|_| ()),
    RequestMsg::SetSoftwareRunning(software) => player.set_software_running(&software).map(|_| ()),
    RequestMsg::CaptainAction(captain) => {
      let _ = player.captain_action(&captain);
      Ok(())
    }
    other => Err(format!("the harness does not apply {other:?}")),
  };
  if let Err(e) = result {
    tracing::debug!("(harness) Refused: {e}");
  }
}

/// The opening observation of one side of `scenario`, for seeing what a
/// commander is shown before any round is played.
///
/// # Errors
/// Returns an error if the side has no ships in the scenario.
///
/// # Panics
/// Panics if the scenario cannot be loaded, as the backend does.
pub async fn opening_view(scenario: &str, side: &str) -> Result<crate::observation::Observation, String> {
  let server = Arc::new(Server::new("harness-view", scenario).await);
  let player = PlayerManager::new(Some(server), Box::new(MockAuthenticator::new("")), true).with_seed(1);
  let world = player.get_entities().map_err(|e| format!("{e:?}"))?;
  let ships = sides(&world)
    .remove(side)
    .ok_or_else(|| format!("no side {side} in {scenario}"))?;
  Ok(observe(&world, &ships))
}

/// Play one game of `scenario` (a scenario file path), with the commander
/// `commanders` names for each side, for at most `max_rounds`.
///
/// # Errors
/// Returns an error if a side's commander cannot be made.
///
/// # Panics
/// Panics if the scenario cannot be loaded, as the backend does.
pub async fn play(
  scenario: &str, commanders: &CommanderPicker<'_>, max_rounds: u32, seed: u64, mut log: Option<&mut dyn Write>,
) -> Result<Outcome, String> {
  let server = Arc::new(Server::new(&format!("harness-{seed}"), scenario).await);
  let player = PlayerManager::new(Some(server), Box::new(MockAuthenticator::new("")), true).with_seed(seed);

  let start = player.get_entities().map_err(|e| format!("{e:?}"))?;
  let rosters = sides(&start);
  let mut deciders = BTreeMap::new();
  for side in rosters.keys() {
    let (name, commander) = commanders(side, seed).ok_or_else(|| format!("no commander for side {side}"))?;
    deciders.insert(side.clone(), (name, commander));
  }

  let mut rounds = 0;
  let mut retreats = Retreats::default();
  let mut fled: Vec<String> = Vec::new();
  let mut disengaged = false;
  loop {
    let world = player.get_entities().map_err(|e| format!("{e:?}"))?;
    for side in retreats.update(&world, &rosters) {
      if !fled.contains(&side) {
        fled.push(side);
      }
    }
    let with_ships: Vec<&String> = rosters
      .iter()
      .filter(|(_, ships)| ships.iter().any(|ship| world.ships.contains_key(ship)))
      .map(|(side, _)| side)
      .collect();
    let alive: Vec<&String> = with_ships.iter().copied().filter(|side| !fled.contains(side)).collect();
    if alive.is_empty() && !with_ships.is_empty() {
      disengaged = true;
    }
    if alive.len() <= 1 || rounds >= max_rounds {
      break;
    }

    for side in alive {
      let ships: Vec<String> = rosters[side]
        .iter()
        .filter(|ship| world.ships.contains_key(*ship))
        .cloned()
        .collect();
      let (_, commander) = deciders.get_mut(side).expect("every side has a commander");
      let intents = commander.decide(&observe(&world, &ships)).await;
      if let Some(log) = log.as_mut() {
        let line = serde_json::to_string(&Record {
          seed,
          round: world.round,
          side: side.clone(),
          plan: commander.plan().map(str::to_string),
          intents: intents.clone(),
        })
        .map_err(|e| e.to_string())?;
        writeln!(log, "{line}").map_err(|e| e.to_string())?;
      }
      for (ship, intent) in intents.iter().filter(|(ship, _)| ships.contains(ship)) {
        for request in tactics::orders(&world, ship, intent).unwrap_or_default() {
          apply(&player, request);
        }
      }
    }
    let _ = player.update();
    rounds += 1;
  }

  let end = player.get_entities().map_err(|e| format!("{e:?}"))?;
  let results: BTreeMap<String, SideResult> = rosters
    .iter()
    .map(|(side, ships)| {
      (
        side.clone(),
        SideResult {
          commander: deciders[side].0.clone(),
          ships_at_start: ships.len(),
          ships_at_end: ships.iter().filter(|ship| end.ships.contains_key(*ship)).count(),
          hull_at_start: hull_of(&start, ships),
          hull_at_end: hull_of(&end, ships),
          model_cost_usd: deciders[side].1.spent(),
        },
      )
    })
    .collect();
  let standing: Vec<&String> = results
    .iter()
    .filter(|(side, result)| result.ships_at_end > 0 && !fled.contains(side))
    .map(|(side, _)| side)
    .collect();
  Ok(Outcome {
    seed,
    rounds,
    winner: (standing.len() == 1).then(|| standing[0].clone()),
    disengaged,
    fled,
    sides: results,
  })
}

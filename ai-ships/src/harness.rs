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
//! that later training reads. Victory is the side still in play at the end;
//! hull points remaining and taken are the finer score.

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

fn sides(world: &Entities) -> BTreeMap<String, Vec<String>> {
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

fn hull_of(world: &Entities, ships: &[String]) -> u32 {
  ships
    .iter()
    .filter_map(|name| world.ships.get(name))
    .map(|ship| ship.read().unwrap().current_hull)
    .sum()
}

/// Rounds with no ship holding an enemy before a game is called off as
/// broken off: long enough to come back, short enough not to pay for two
/// sides flying apart.
pub const DISENGAGED_ROUNDS: u32 = 3;

/// Whether any ship holds a contact on a ship of another side.
fn in_contact(world: &Entities) -> bool {
  world.ships.values().any(|ship| {
    let ship = ship.read().unwrap();
    let side = side_of(ship.team, ship.get_name());
    ship.contacts.iter().any(|name| {
      world.ships.get(name).is_some_and(|other| {
        let other = other.read().unwrap();
        side_of(other.team, other.get_name()) != side
      })
    })
  })
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
  /// The game ended because no ship had held an enemy for
  /// [`DISENGAGED_ROUNDS`] rounds: the sides broke off.
  #[serde(default)]
  pub disengaged: bool,
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
fn apply(player: &PlayerManager, request: RequestMsg) {
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
  let mut out_of_contact = 0;
  let mut engaged = false;
  let mut disengaged = false;
  loop {
    let world = player.get_entities().map_err(|e| format!("{e:?}"))?;
    // Only a fight that has started can be broken off: a scenario that
    // opens with everyone out of contact (Marduk, at Distant) is an approach.
    if in_contact(&world) {
      engaged = true;
      out_of_contact = 0;
    } else {
      out_of_contact += 1;
    }
    if engaged && out_of_contact >= DISENGAGED_ROUNDS {
      disengaged = true;
      break;
    }
    let alive: Vec<&String> = rosters
      .iter()
      .filter(|(_, ships)| ships.iter().any(|ship| world.ships.contains_key(ship)))
      .map(|(side, _)| side)
      .collect();
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
    .filter(|(_, result)| result.ships_at_end > 0)
    .map(|(side, _)| side)
    .collect();
  Ok(Outcome {
    seed,
    rounds,
    winner: (standing.len() == 1).then(|| standing[0].clone()),
    disengaged,
    sides: results,
  })
}

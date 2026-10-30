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
//! having destroyed the other or driven it off (see [`Retreats`]); at the
//! round limit it is judged as the game's own victory screen judges it (see
//! [`verdict_at_the_limit`]); hull points remaining and taken are the finer
//! score.

use std::collections::BTreeMap;
use std::io::Write;
use std::sync::Arc;

use callisto::authentication::MockAuthenticator;
use callisto::entity::{Entities, Entity, Vec3};
use callisto::payloads::RequestMsg;
use callisto::player::PlayerManager;
use callisto::server::Server;
use callisto::ship::{Goal, DEFAULT_POST_RADIUS_KM};
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
  // The craft aboard a carrier are the side's ships too: off the plot
  // until launched, and crewed by the side once they are.
  for ship in world.ships.values() {
    let ship = ship.read().unwrap();
    for ship in std::iter::once(&*ship).chain(&ship.embarked) {
      sides
        .entry(side_of(ship.team, ship.get_name()))
        .or_default()
        .push(ship.get_name().to_string());
    }
  }
  for ships in sides.values_mut() {
    ships.sort();
  }
  sides
}

/// The hull these ships have left, counting none for a ship no longer in
/// play. A craft aboard a carrier is not on the plot but is still the side's
/// hull: she counts at what she has, until the carrier goes down with her.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn hull_of(world: &Entities, ships: &[String]) -> u32 {
  let aboard: BTreeMap<String, u32> = world
    .ships
    .values()
    .flat_map(|ship| {
      ship
        .read()
        .unwrap()
        .embarked
        .iter()
        .map(|craft| (craft.get_name().to_string(), craft.current_hull))
        .collect::<Vec<_>>()
    })
    .collect();
  ships
    .iter()
    .map(|name| {
      world.ships.get(name).map_or_else(
        || aboard.get(name).copied().unwrap_or(0),
        |ship| ship.read().unwrap().current_hull,
      )
    })
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

/// Where a side holds and how close counts, in metres: its first ship's
/// post, or the middle of where it started with the default radius.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn post_of(start: &Entities, now: &Entities, ships: &[String]) -> (Vec3, f64) {
  let posted = ships
    .iter()
    .filter_map(|name| start.ships.get(name))
    .map(|ship| ship.read().unwrap())
    .find(|ship| ship.post.is_some());
  let radius = posted
    .as_ref()
    .and_then(|ship| ship.post_radius)
    .unwrap_or(DEFAULT_POST_RADIUS_KM)
    * 1000.0;
  let at = posted
    .and_then(|ship| now.post_position(ship.post.as_ref()?))
    .unwrap_or_else(|| {
      let placed: Vec<Vec3> = ships
        .iter()
        .filter_map(|name| start.ships.get(name))
        .map(|ship| ship.read().unwrap().get_position())
        .collect();
      #[allow(clippy::cast_precision_loss)]
      let count = placed.len().max(1) as f64;
      placed.iter().fold(Vec3::new(0.0, 0.0, 0.0), |sum, at| sum + at) / count
    });
  (at, radius)
}

/// The verdict when the round limit ends a game with more than one side
/// still in it, as the game's own victory screen gives it (`victory.ts`,
/// `timeUp`): a side whose goal is Hold with a ship still at its post has
/// held, and wins; if a Hold side is in the game and none has held, the post
/// fell, and the attackers win; with no Hold side it is nobody's game.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn verdict_at_the_limit(
  start: &Entities, end: &Entities, rosters: &BTreeMap<String, Vec<String>>, standing: &[&String],
) -> Option<String> {
  use cgmath::InnerSpace;
  let holds = |ships: &[String]| {
    ships
      .iter()
      .filter_map(|name| start.ships.get(name))
      .any(|ship| ship.read().unwrap().goal == Some(Goal::Hold))
  };
  let holders: Vec<&String> = rosters.iter().filter(|(_, ships)| holds(ships)).map(|(side, _)| side).collect();
  if holders.is_empty() {
    return None;
  }
  let held = holders.iter().copied().find(|side| {
    let (post, radius) = post_of(start, end, &rosters[*side]);
    rosters[*side]
      .iter()
      .filter_map(|name| end.ships.get(name))
      .any(|ship| (ship.read().unwrap().get_position() - post).magnitude() <= radius)
  });
  held
    .or_else(|| standing.iter().copied().find(|side| !holders.contains(side)))
    .cloned()
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
  /// The one side still in play, if only one is; or, at the round limit,
  /// the side [`verdict_at_the_limit`] names.
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
  /// Every ship in play as the round began, true state: what the orders are
  /// judged against.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub ships: Vec<ShipState>,
  /// The commander's own account of the decision, if it gives one.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub explain: Option<serde_json::Value>,
}

/// One ship as a round began, for the log.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ShipState {
  pub name: String,
  pub side: String,
  pub design: String,
  pub position: [f64; 3],
  pub velocity: [f64; 3],
  pub hull: u32,
  pub design_hull: u32,
  pub armour: u32,
  /// Thrust she can make now, in G.
  pub thrust: u8,
  pub dodge: u8,
  pub active_sensors: bool,
  pub transmitting: bool,
  pub contacts: Vec<String>,
  pub locks: Vec<String>,
  /// Systems powered down.
  pub offline: Vec<String>,
}

/// Every ship in the world as the log records it.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn ship_states(world: &Entities) -> Vec<ShipState> {
  let mut states: Vec<ShipState> = world
    .ships
    .values()
    .map(|ship| {
      let ship = ship.read().unwrap();
      let (p, v) = (ship.get_position(), ship.get_velocity());
      ShipState {
        name: ship.get_name().to_string(),
        side: side_of(ship.team, ship.get_name()),
        design: ship.design.name.clone(),
        position: [p.x, p.y, p.z],
        velocity: [v.x, v.y, v.z],
        hull: ship.current_hull,
        design_hull: ship.design.hull,
        armour: ship.get_current_armor(),
        thrust: ship.max_acceleration(),
        dodge: ship.get_dodge_thrust(),
        active_sensors: ship.active_sensors,
        transmitting: ship.transmitting,
        contacts: ship.contacts.clone(),
        locks: ship.sensor_locks.clone(),
        offline: ship.offline.iter().map(|system| format!("{system:?}")).collect(),
      }
    })
    .collect();
  states.sort_by(|a, b| a.name.cmp(&b.name));
  states
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

/// The view `side` would have once every ordinary ship in reach had been
/// found: the opening view of an engagement already under way.
///
/// # Errors
/// Returns an error if the side is not in the scenario.
pub async fn engaged_view(scenario: &str, side: &str) -> Result<crate::observation::Observation, String> {
  let server = Arc::new(Server::new("harness-view", scenario).await);
  let player = PlayerManager::new(Some(server), Box::new(MockAuthenticator::new("")), true).with_seed(1);
  let world = player.get_entities().map_err(|e| format!("{e:?}"))?;
  world.establish_initial_contacts();
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
  scenario: &str, commanders: &CommanderPicker<'_>, max_rounds: u32, seed: u64, log: Option<&mut dyn Write>,
) -> Result<Outcome, String> {
  play_recorded(scenario, commanders, max_rounds, seed, log, None).await
}

/// One round's combat events, as the engine reports them to the players:
/// who fired at whom, hits and misses, damage, interceptions, ships lost.
#[derive(Debug, Clone, Serialize)]
pub struct RoundEvents {
  pub seed: u64,
  pub round: u32,
  pub effects: Vec<callisto::payloads::EffectMsg>,
}

/// [`play`], also writing every round's combat events to `events`, one JSON
/// line per round.
///
/// # Errors
/// As [`play`], and if an event line cannot be written.
///
/// # Panics
/// As [`play`].
pub async fn play_recorded(
  scenario: &str, commanders: &CommanderPicker<'_>, max_rounds: u32, seed: u64, mut log: Option<&mut dyn Write>,
  mut events: Option<&mut dyn Write>,
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
          ships: ship_states(&world),
          explain: commander.explain(),
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
    let round = player.get_entities().map(|world| world.round).unwrap_or_default();
    let effects = player.update();
    if let Some(events) = events.as_mut() {
      let line = serde_json::to_string(&RoundEvents { seed, round, effects }).map_err(|e| e.to_string())?;
      writeln!(events, "{line}").map_err(|e| e.to_string())?;
    }
    rounds += 1;
  }

  let end = player.get_entities().map_err(|e| format!("{e:?}"))?;
  Ok(judge(
    &start, &end, &rosters, &deciders, seed, rounds, max_rounds, fled, disengaged,
  ))
}

/// The game's outcome once it has stopped: each side's result and the
/// winner -- the one side still in it, or at the round limit the side the
/// verdict there gives.
#[allow(clippy::too_many_arguments)]
fn judge(
  start: &Entities, end: &Entities, rosters: &BTreeMap<String, Vec<String>>,
  deciders: &BTreeMap<String, (String, Box<dyn Commander>)>, seed: u64, rounds: u32, max_rounds: u32,
  fled: Vec<String>, disengaged: bool,
) -> Outcome {
  let results: BTreeMap<String, SideResult> = rosters
    .iter()
    .map(|(side, ships)| {
      (
        side.clone(),
        SideResult {
          commander: deciders[side].0.clone(),
          ships_at_start: ships.len(),
          ships_at_end: ships.iter().filter(|ship| end.ships.contains_key(*ship)).count(),
          hull_at_start: hull_of(start, ships),
          hull_at_end: hull_of(end, ships),
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
  let winner = match standing.as_slice() {
    [one] => Some((*one).clone()),
    _ if rounds >= max_rounds => verdict_at_the_limit(start, end, rosters, &standing),
    _ => None,
  };
  Outcome {
    seed,
    rounds,
    winner,
    disengaged,
    fled,
    sides: results,
  }
}

#[cfg(test)]
mod limit_tests {
  use super::*;
  use callisto::ship::{FeatureKind, Post, ShipDesignTemplate, ShipFeature, Team};
  use cgmath::Zero;

  fn design(name: &str, displacement: u32, hull: u32, features: Vec<ShipFeature>) -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: name.to_string(),
      displacement,
      hull,
      features,
      ..ShipDesignTemplate::default()
    })
  }

  /// Blue holds a post at the origin (radius 1,000 km) and Red attacks; Blue
  /// `away` metres from the post at the end.
  fn world(away: f64, blue_holds: bool) -> Entities {
    let hull = design("Hull", 400, 160, vec![]);
    let mut world = Entities::default();
    world.add_ship("Picket".to_string(), Vec3::new(away, 0.0, 0.0), Vec3::zero(), &hull, None, None);
    world.add_ship(
      "Raider".to_string(),
      Vec3::new(3.0e7, 0.0, 0.0),
      Vec3::zero(),
      &hull,
      None,
      None,
    );
    {
      let mut picket = world.ships["Picket"].write().unwrap();
      picket.team = Some(Team::Blue);
      picket.goal = blue_holds.then_some(Goal::Hold);
      picket.post = blue_holds.then_some(Post::At([0.0, 0.0, 0.0]));
      picket.post_radius = Some(1_000.0);
    }
    world.ships["Raider"].write().unwrap().team = Some(Team::Red);
    world
  }

  fn verdict(away: f64, blue_holds: bool) -> Option<String> {
    let start = world(0.0, blue_holds);
    let end = world(away, blue_holds);
    let rosters = sides(&start);
    let standing: Vec<&String> = rosters.keys().collect();
    verdict_at_the_limit(&start, &end, &rosters, &standing)
  }

  #[test]
  fn a_holder_still_at_its_post_has_held() {
    assert_eq!(verdict(5.0e5, true).as_deref(), Some("Blue"));
  }

  #[test]
  fn a_post_left_falls_to_the_attackers() {
    assert_eq!(verdict(5.0e6, true).as_deref(), Some("Red"));
  }

  #[test]
  fn with_no_hold_side_the_limit_is_a_draw() {
    assert_eq!(verdict(5.0e6, false), None);
  }

  /// Without a post the side holds where it started, within the default
  /// radius.
  #[test]
  fn a_side_with_no_post_holds_where_it_started() {
    let start = world(0.0, true);
    start.ships["Picket"].write().unwrap().post = None;
    let (post, radius) = post_of(&start, &start, &["Picket".to_string()]);
    assert_eq!(post, Vec3::zero());
    assert_eq!(radius, DEFAULT_POST_RADIUS_KM * 1000.0);
  }

  #[test]
  fn hull_counts_the_craft_aboard() {
    let berths = vec![ShipFeature {
      name: "Docking spaces".to_string(),
      power: 0,
      default_on: true,
      kind: Some(FeatureKind::DockingSpace {
        craft_tons: 50,
        count: 2,
      }),
    }];
    let carrier = design("Carrier", 50_000, 27_500, berths);
    let fighter = design("Fighter", 50, 20, vec![]);
    let mut world = Entities::default();
    world.add_ship("Mother".to_string(), Vec3::zero(), Vec3::zero(), &carrier, None, None);
    world.add_ship("Chick".to_string(), Vec3::zero(), Vec3::zero(), &fighter, None, None);
    world.ships["Chick"].write().unwrap().current_hull = 15;
    world.embark_ship("Chick", "Mother").unwrap();
    assert!(!world.ships.contains_key("Chick"), "aboard, not on the plot");
    let roster = ["Mother".to_string(), "Chick".to_string(), "Lost".to_string()];
    assert_eq!(hull_of(&world, &roster), 27_500 + 15);
    assert_eq!(
      sides(&world)["ship:Mother"].len(),
      1,
      "a craft is her own side when nobody has a team"
    );
  }
}

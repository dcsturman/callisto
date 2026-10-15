//! What decides a side's orders each round.
//!
//! A commander sees only an [`Observation`] -- never the world -- and answers
//! with an [`Intent`] for each of its ships, in tactical verbs. Turning those
//! into a console's messages is `callisto::tactics`, not the commander.
//!
//! Three commanders so far, none of them clever:
//!
//! * [`Idle`] gives no orders. Ships keep their plans.
//! * [`Hold`] keeps its plans and fights whatever is in reach: a fixed
//!   opponent to measure smarter commanders against.
//! * [`Close`] heads for the nearest enemy at the range its guns like best and
//!   fights. What the service uses until there is a model.

use callisto::entity::Entity;
use callisto::ship::{Range, ShipSystem, WeaponType};
use callisto::tactics::{weapons_that_bear, Engage, EngineerVerb, Helm, Intent, SensorVerb};

use crate::observation::{Contact, Observation};

/// One side's decision-maker.
pub trait Commander: Send {
  /// Each of the side's ships' orders for this round.
  fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)>;
}

/// Gives no orders. Ships hold their current plans.
#[derive(Debug, Default)]
pub struct Idle;

impl Commander for Idle {
  fn decide(&mut self, _view: &Observation) -> Vec<(String, Intent)> {
    vec![]
  }
}

/// The order an engineer repairs in: what keeps a ship fighting and moving
/// first.
const REPAIR_PRIORITY: [ShipSystem; 9] = [
  ShipSystem::Maneuver,
  ShipSystem::Powerplant,
  ShipSystem::Weapon,
  ShipSystem::Sensors,
  ShipSystem::Bridge,
  ShipSystem::Fuel,
  ShipSystem::Jump,
  ShipSystem::Crew,
  ShipSystem::Cargo,
];

/// Fight the nearest enemy with everything that reaches, lock it, defend, and
/// repair what is broken.
fn fight(target: Option<&Contact>, helm: Helm) -> Intent {
  Intent {
    helm,
    fire: target
      .map(|target| Engage {
        target: target.name.clone(),
        ..Engage::default()
      })
      .into_iter()
      .collect(),
    defend: true,
    sensors: target
      .map(|target| SensorVerb::Lock {
        target: target.name.clone(),
      })
      .into_iter()
      .collect(),
    engineering: vec![EngineerVerb::Repair {
      priority: REPAIR_PRIORITY.to_vec(),
    }],
    rationale: target.map_or_else(
      || "No enemy in contact; holding.".to_string(),
      |target| format!("Engaging {}, the nearest enemy.", target.name),
    ),
    ..Intent::default()
  }
}

/// Keeps its plans and fights whatever is in reach.
#[derive(Debug, Default)]
pub struct Hold;

impl Commander for Hold {
  fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let target = view.enemies().into_iter().next();
    view
      .own
      .iter()
      .map(|ship| (ship.get_name().to_string(), fight(target, Helm::Hold)))
      .collect()
  }
}

/// The band a ship's guns like best: the furthest every one of its direct-fire
/// guns still reaches, so none is wasted. A ship with only launchers stays at
/// Long, out of most guns' way.
fn preferred_band(ship: &callisto::ship::Ship) -> Range {
  [Range::VeryLong, Range::Long, Range::Medium, Range::Short]
    .into_iter()
    .find(|band| {
      let direct = ship
        .weapons()
        .iter()
        .filter(|weapon| {
          weapon
            .kinds()
            .iter()
            .any(|kind| !matches!(kind, WeaponType::Sand | WeaponType::PointDefense | WeaponType::Repulsor))
        })
        .count();
      weapons_that_bear(ship, *band).len() >= direct
    })
    .unwrap_or(Range::Long)
}

/// Heads for the nearest enemy at the range its guns like best, and fights.
#[derive(Debug, Default)]
pub struct Close;

impl Commander for Close {
  fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let target = view.enemies().into_iter().next();
    view
      .own
      .iter()
      .map(|ship| {
        let helm = target.map_or(Helm::Hold, |target| Helm::Intercept {
          target: target.name.clone(),
          band: preferred_band(ship),
        });
        (ship.get_name().to_string(), fight(target, helm))
      })
      .collect()
  }
}

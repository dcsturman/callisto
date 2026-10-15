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
//! * [`Close`] heads for the nearest enemy, to the range band where it does
//!   the most harm for the least taken, and fights. What the service uses
//!   until there is a model.

use callisto::entity::Entity;
use callisto::ship::{Range, ShipSystem};
use callisto::tactics::{BandOdds, Engage, EngineerVerb, Helm, Intent, SensorVerb};

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

/// The band to fight a contact in, read off the engagement table.
///
/// A fight is a race to the other ship's hull, so each band is scored as the
/// share of their hull we expect to take in a round, less the share of ours
/// they expect to take. That is why a small ship that out-reaches a big one
/// stands off where it cannot be answered, rather than closing to where it
/// does the most raw damage and takes the most back. Their hull is their
/// class's; ours is what we have left.
///
/// Bands where we cannot hurt them are never chosen: this commander is here
/// to fight. On a tie, the band where we do more. With nothing that reaches,
/// Long.
fn best_band(view: &Observation, ours: &str, contact: &Contact) -> Range {
  let our_hull = view
    .own
    .iter()
    .find(|ship| ship.get_name() == ours)
    .map_or(1.0, |ship| f64::from(ship.current_hull.max(1)));
  let their_hull = f64::from(contact.design_hull.max(1));
  view.engagement(ours, &contact.name).map_or(Range::Long, |table| {
    let edge = |we: &BandOdds, they: &BandOdds| we.expected_damage / their_hull - they.expected_damage / our_hull;
    table
      .we_hit
      .iter()
      .zip(&table.they_hit)
      .filter(|(we, _)| we.expected_damage > 0.0)
      .max_by(|(we_a, they_a), (we_b, they_b)| {
        edge(we_a, they_a)
          .total_cmp(&edge(we_b, they_b))
          .then(we_a.expected_damage.total_cmp(&we_b.expected_damage))
      })
      .map_or(Range::Long, |(we, _)| we.band)
  })
}

/// Heads for the nearest enemy, to the band where it does the most harm for
/// the least taken, and fights.
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
          band: best_band(view, ship.get_name(), target),
        });
        (ship.get_name().to_string(), fight(target, helm))
      })
      .collect()
  }
}

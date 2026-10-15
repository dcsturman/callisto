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
//!   the most harm for the least taken, and fights. A fixed opponent that
//!   fights back.
//! * [`Replay`] plays back a recorded game's decisions, round by round, so
//!   the tests never call a model.
//!
//! The one that asks a model is [`crate::llm::Llm`].

use std::collections::BTreeMap;
use std::io::BufRead;

use async_trait::async_trait;
use callisto::entity::Entity;
use callisto::ship::{Range, ShipSystem};
use callisto::tactics::{BandOdds, Engage, EngineerVerb, Helm, Intent, SensorVerb};

use crate::harness::Record;
use crate::observation::{Contact, Observation};

/// One side's decision-maker.
#[async_trait]
pub trait Commander: Send {
  /// Each of the side's ships' orders for this round.
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)>;

  /// The standing plan the side is working to, for the log. Only a commander
  /// that thinks ahead has one.
  fn plan(&self) -> Option<&str> {
    None
  }

  /// Whether the last call to `decide` produced a decision. A commander that
  /// asks a model can fail to; the rest always decide, even if to do nothing.
  fn answered(&self) -> bool {
    true
  }
}

/// Gives no orders. Ships hold their current plans.
#[derive(Debug, Default)]
pub struct Idle;

#[async_trait]
impl Commander for Idle {
  async fn decide(&mut self, _view: &Observation) -> Vec<(String, Intent)> {
    vec![]
  }
}

/// The order an engineer repairs in: what keeps a ship fighting and moving
/// first.
pub const REPAIR_PRIORITY: [ShipSystem; 9] = [
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
/// repair what is broken. With no enemy in contact, sweep with active sensors
/// to find one: holding quiet with nothing to hide from finds nobody.
fn fight(target: Option<&Contact>, helm: Helm) -> Intent {
  Intent {
    posture: target.is_none().then_some(callisto::tactics::Posture::Active),
    helm,
    fire: target
      .map(|target| Engage {
        target: target.name.clone(),
        ..Engage::default()
      })
      .into_iter()
      .collect(),
    defend: vec![],
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
      || "No enemy in contact; sweeping for one.".to_string(),
      |target| format!("Engaging {}, the nearest enemy.", target.name),
    ),
    ..Intent::default()
  }
}

/// Keeps its plans and fights whatever is in reach.
#[derive(Debug, Default)]
pub struct Hold;

#[async_trait]
impl Commander for Hold {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let target = target(view);
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

/// The enemy to fight: among those nearly as near as the nearest (within a
/// tenth), the one the side can hurt most, as a share of its hull, at the
/// best band each of our ships has against it. Ties between equally near
/// ships go to the softest target rather than to whichever name sorts first.
fn target(view: &Observation) -> Option<&Contact> {
  let enemies = view.enemies();
  let nearest = enemies.first()?.distance;
  let harm = |contact: &Contact| -> f64 {
    let hull = f64::from(contact.design_hull.max(1));
    view
      .own
      .iter()
      .filter_map(|ship| view.engagement(ship.get_name(), &contact.name))
      .map(|table| table.we_hit.iter().map(|odds| odds.expected_damage).fold(0.0, f64::max) / hull)
      .sum()
  };
  enemies
    .into_iter()
    .filter(|contact| contact.distance <= nearest * 1.1)
    .max_by(|a, b| harm(a).total_cmp(&harm(b)).then(b.name.cmp(&a.name)))
}

/// Heads for the nearest enemy, to the band where it does the most harm for
/// the least taken, and fights.
#[derive(Debug, Default)]
pub struct Close;

#[async_trait]
impl Commander for Close {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let target = target(view);
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

/// Any legal verb at each station, by chance. The floor every other commander
/// should clear: a commander that cannot beat this is not deciding anything.
pub struct Random {
  rng: rand::rngs::SmallRng,
}

impl Random {
  #[must_use]
  pub fn seeded(seed: u64) -> Self {
    use rand::SeedableRng;
    Random {
      rng: rand::rngs::SmallRng::seed_from_u64(seed),
    }
  }
}

#[async_trait]
impl Commander for Random {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    use callisto::tactics::{Posture, BANDS};
    use rand::seq::SliceRandom;
    use rand::Rng;
    let enemies = view.enemies();
    view
      .own
      .iter()
      .map(|ship| {
        let target = enemies.choose(&mut self.rng).copied();
        let helm = match (target, self.rng.gen_range(0..3)) {
          (Some(target), 0) => Helm::Intercept {
            target: target.name.clone(),
            band: *BANDS.choose(&mut self.rng).unwrap_or(&Range::Long),
          },
          (Some(target), 1) => Helm::OpenRange {
            target: target.name.clone(),
          },
          _ => Helm::Hold,
        };
        let mut intent = fight(target, helm);
        intent.posture = [Posture::Silent, Posture::Passive, Posture::Active]
          .choose(&mut self.rng)
          .copied();
        intent.rationale = "Chosen at random.".to_string();
        (ship.get_name().to_string(), intent)
      })
      .collect()
  }
}

/// Plays back one side's decisions from a harness log, by round.
///
/// A round the log has nothing for gets no orders. Which game's decisions is
/// up to the file: a log of one game replays that game, which is what a test
/// wants.
pub struct Replay {
  by_round: BTreeMap<u32, Record>,
}

impl Replay {
  /// Read `side`'s records from a harness log (JSON lines of [`Record`]).
  ///
  /// # Errors
  /// Returns an error if the file cannot be read or a line is not a record.
  pub fn from_log(path: &str, side: &str) -> Result<Self, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("{path}: {e}"))?;
    let mut by_round = BTreeMap::new();
    for line in std::io::BufReader::new(file).lines() {
      let line = line.map_err(|e| format!("{path}: {e}"))?;
      if line.trim().is_empty() {
        continue;
      }
      let record: Record = serde_json::from_str(&line).map_err(|e| format!("{path}: {e}"))?;
      if record.side == side {
        by_round.insert(record.round, record);
      }
    }
    Ok(Replay { by_round })
  }

  /// The rounds the log has decisions for.
  #[must_use]
  pub fn rounds(&self) -> Vec<u32> {
    self.by_round.keys().copied().collect()
  }
}

#[async_trait]
impl Commander for Replay {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    self
      .by_round
      .get(&view.round)
      .map(|record| record.intents.clone())
      .unwrap_or_default()
  }

  fn plan(&self) -> Option<&str> {
    self.by_round.values().last().and_then(|record| record.plan.as_deref())
  }
}

/// A commander by name, for the harness and configuration: `idle`, `hold`,
/// `close`, `random`, or `replay:<log file>:<side>`. `seed` is used by those
/// that roll dice of their own. Models are named through
/// [`crate::llm::named`].
#[must_use]
pub fn named(name: &str, seed: u64) -> Option<Box<dyn Commander>> {
  if let Some(rest) = name.strip_prefix("replay:") {
    let (path, side) = rest.rsplit_once(':')?;
    return Replay::from_log(path, side)
      .ok()
      .map(|replay| Box::new(replay) as Box<dyn Commander>);
  }
  match name {
    "idle" => Some(Box::new(Idle)),
    "hold" => Some(Box::new(Hold)),
    "close" => Some(Box::new(Close)),
    "random" => Some(Box::new(Random::seeded(seed))),
    _ => None,
  }
}

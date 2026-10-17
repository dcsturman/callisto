//! What a side may know: the one place fog of war is applied.
//!
//! The backend sends every connection the whole world, and the browser hides
//! what a crew has not detected. A commander must be held to the same, so it
//! never sees the world -- only an [`Observation`], built here, of what the
//! side's own consoles would show:
//!
//! * its own ships, in full;
//! * ships on the same team, in full, since teams always know where each
//!   other are;
//! * each contact any of its ships holds, but only what the sensor station
//!   shows for a contact: its class, where it is and how it is moving, its
//!   rated drive and sensors, its weapons, and its critical hits. Not its hull
//!   points, power, crew or orders;
//! * planets, and missiles in flight at or from its ships;
//! * for each of its ships and each contact, an [`Engagement`]: the odds
//!   each way at every range band, worked out from the rules' own to-hit and
//!   damage arithmetic. A commander reads "at Very Long they cannot touch us"
//!   off the table rather than having to know why.
//!
//! * for each of its ships and each contact, an [`Exposure`]: by posture and
//!   whether it fires, the chance that contact finds it this round, and for a
//!   stealthy ship the chance of slipping contact by opening the range.
//!
//! An enemy's crew is not visible, so their side of the table assumes
//! [`ASSUMED_ENEMY_GUNNERY`] at every mount.

use std::collections::BTreeSet;

use callisto::entity::{Entities, Entity, Vec3};
use callisto::missile::Missile;
use callisto::planet::Planet;
use callisto::ship::{CounterMeasures, Range, Sensors, Ship, Stealth, Team, Weapon};
use callisto::tactics::{band_between, detection, engagement, BandOdds, DetectionOdds};
use cgmath::InnerSpace;
use serde::Serialize;

/// A ship this side has detected, as its sensor station shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Contact {
  pub name: String,
  pub design: String,
  pub displacement: u32,
  /// The hull and armour its class is built with.
  pub design_hull: u32,
  /// The hull it has left. A player's ship list shows this for any ship one
  /// of their side holds as a contact, so the AI is shown it too.
  pub current_hull: u32,
  pub design_armor: u32,
  pub stealth: Option<Stealth>,
  pub tl: u8,
  pub countermeasures: Option<CounterMeasures>,
  pub team: Option<Team>,
  pub position: Vec3,
  pub velocity: Vec3,
  /// The thrust it can make now.
  pub current_maneuver: u8,
  /// The burn it is making this round, m/s². A contact's course is on the
  /// plot, as a console shows it; a blip's is not.
  pub burn: Vec3,
  /// The sensors its class is built with.
  pub sensors: Sensors,
  pub weapons: Vec<Weapon>,
  /// Critical hits by system, as the contact detail shows them.
  pub crit_level: [u8; 11],
  /// Which of our ships hold this contact.
  pub held_by: Vec<String>,
  /// The nearest of our ships to it, and the band between them.
  pub nearest: String,
  pub band: Range,
  pub distance: f64,
}

/// A ship on the plot that no ship of ours has detected: the dimmed blip a
/// console shows. Where it is and how it is moving, and nothing else -- not
/// its class, not its guns. A course can be laid toward or away from it;
/// nothing else can be done to it.
#[derive(Debug, Clone, Serialize)]
pub struct Blip {
  pub name: String,
  pub position: Vec3,
  pub velocity: Vec3,
  /// The nearest of our ships to it, and how far.
  pub nearest: String,
  pub distance: f64,
}

/// An enemy ship that has found one of ours, as the sensor station's "Eyes on
/// us" panel shows it: who, and whether it holds a lock as well.
#[derive(Debug, Clone, Serialize)]
pub struct Watcher {
  /// Our ship being watched.
  pub ours: String,
  /// The enemy ship watching.
  pub by: String,
  pub lock: bool,
}

/// The gunner skill a side assumes of an enemy whose crew it cannot see: a
/// trained crew.
pub const ASSUMED_ENEMY_GUNNERY: i32 = 2;

/// What one of our ships and one contact can do to each other, by band.
#[derive(Debug, Clone, Serialize)]
pub struct Engagement {
  pub ours: String,
  pub contact: String,
  /// Our ship's guns against the contact.
  pub we_hit: Vec<BandOdds>,
  /// The contact's guns against our ship.
  pub they_hit: Vec<BandOdds>,
}

/// How visible one of our ships is to one contact.
#[derive(Debug, Clone, Serialize)]
pub struct Exposure {
  pub ours: String,
  pub contact: String,
  pub odds: Vec<DetectionOdds>,
}

/// Everything one side may decide from.
#[derive(Debug, Clone, Serialize)]
pub struct Observation {
  pub round: u32,
  pub team: Option<Team>,
  /// The side's own ships, in full.
  pub own: Vec<Ship>,
  /// Other ships on the side's team, in full.
  pub allies: Vec<Ship>,
  pub contacts: Vec<Contact>,
  /// Ships of other sides on the plot that nobody of ours has detected.
  pub blips: Vec<Blip>,
  /// Which enemies hold a contact on each of our ships.
  pub watchers: Vec<Watcher>,
  pub planets: Vec<Planet>,
  /// Missiles aimed at our ships, or fired by them.
  pub missiles: Vec<Missile>,
  /// The odds each way, for every pairing of our ships with a contact.
  pub engagements: Vec<Engagement>,
  /// How visible each of our ships is to each contact.
  pub exposures: Vec<Exposure>,
}

impl Observation {
  /// The odds between one of our ships and a contact.
  #[must_use]
  pub fn engagement(&self, ours: &str, contact: &str) -> Option<&Engagement> {
    self
      .engagements
      .iter()
      .find(|engagement| engagement.ours == ours && engagement.contact == contact)
  }

  /// The contacts that are not on our team, nearest first.
  #[must_use]
  pub fn enemies(&self) -> Vec<&Contact> {
    let mut enemies: Vec<&Contact> = self
      .contacts
      .iter()
      .filter(|contact| self.team.is_none() || contact.team != self.team)
      .collect();
    enemies.sort_by(|a, b| a.distance.total_cmp(&b.distance));
    enemies
  }
}

/// The odds and exposure tables for every pairing of our ships with a contact.
fn tables(own: &[Ship], ships: &[Ship], contacts: &[Contact]) -> (Vec<Engagement>, Vec<Exposure>) {
  let known = |ship: &&Ship| contacts.iter().any(|contact| contact.name == ship.get_name());
  let engagements = own
    .iter()
    .flat_map(|ship| {
      ships.iter().filter(known).map(move |contact| Engagement {
        ours: ship.get_name().to_string(),
        contact: contact.get_name().to_string(),
        // Our gunners we know; their armour is what their class carries.
        we_hit: engagement(
          ship,
          contact,
          &|mount| i32::from(ship.get_crew().get_gunnery(mount)),
          contact.design.armor,
        ),
        // Their gunners we can only guess at; our armour we know.
        they_hit: engagement(contact, ship, &|_| ASSUMED_ENEMY_GUNNERY, ship.get_current_armor()),
      })
    })
    .collect();
  let exposures = own
    .iter()
    .flat_map(|ship| {
      ships.iter().filter(known).map(move |contact| Exposure {
        ours: ship.get_name().to_string(),
        contact: contact.get_name().to_string(),
        odds: detection(contact, ship),
      })
    })
    .collect();
  (engagements, exposures)
}

/// "Eyes on us": every enemy holding a contact on one of our ships.
fn watchers_of(own: &[Ship], enemies: &[&Ship]) -> Vec<Watcher> {
  own
    .iter()
    .flat_map(|mine| {
      enemies
        .iter()
        .filter(|ship| ship.contacts.iter().any(|name| name == mine.get_name()))
        .map(|ship| Watcher {
          ours: mine.get_name().to_string(),
          by: ship.get_name().to_string(),
          lock: ship.sensor_locks.iter().any(|name| name == mine.get_name()),
        })
    })
    .collect()
}

fn snapshot(world: &Entities) -> Vec<Ship> {
  let mut ships: Vec<Ship> = world.ships.values().map(|ship| ship.read().unwrap().clone()).collect();
  ships.sort_by(|a, b| a.get_name().cmp(b.get_name()));
  ships
}

/// What the side made of these ships may know of this world.
///
/// # Panics
/// Panics if an entity's lock is poisoned.
#[must_use]
pub fn observe(world: &Entities, side: &[String]) -> Observation {
  let ships = snapshot(world);
  let own: Vec<Ship> = ships
    .iter()
    .filter(|ship| side.contains(&ship.get_name().to_string()))
    .cloned()
    .collect();
  let team = own.iter().find_map(|ship| ship.team);
  let ours = |ship: &Ship| side.contains(&ship.get_name().to_string());
  let allies: Vec<Ship> = ships
    .iter()
    .filter(|ship| !ours(ship) && team.is_some() && ship.team == team)
    .cloned()
    .collect();

  let held: BTreeSet<&String> = own.iter().flat_map(|ship| ship.contacts.iter()).collect();
  let contacts: Vec<Contact> = ships
    .iter()
    .filter(|ship| held.contains(&ship.get_name().to_string()) && !ours(ship))
    .filter(|ship| team.is_none() || ship.team != team)
    .filter_map(|contact| {
      let nearest = own.iter().min_by(|a, b| {
        let da = (a.get_position() - contact.get_position()).magnitude();
        let db = (b.get_position() - contact.get_position()).magnitude();
        da.total_cmp(&db)
      })?;
      Some(Contact {
        name: contact.get_name().to_string(),
        design: contact.design.name.clone(),
        displacement: contact.design.displacement,
        design_hull: contact.design.hull,
        current_hull: contact.current_hull,
        design_armor: contact.design.armor,
        stealth: contact.design.stealth,
        tl: contact.design.tl,
        countermeasures: contact.design.countermeasures,
        team: contact.team,
        position: contact.get_position(),
        velocity: contact.get_velocity(),
        current_maneuver: contact.current_maneuver,
        burn: contact.plan.0 .0,
        sensors: contact.design.sensors,
        weapons: contact.weapons().to_vec(),
        crit_level: contact.crit_level,
        held_by: own
          .iter()
          .filter(|ship| ship.contacts.iter().any(|name| name == contact.get_name()))
          .map(|ship| ship.get_name().to_string())
          .collect(),
        nearest: nearest.get_name().to_string(),
        band: band_between(nearest, contact),
        distance: (nearest.get_position() - contact.get_position()).magnitude(),
      })
    })
    .collect();

  let blips: Vec<Blip> = ships
    .iter()
    .filter(|ship| !ours(ship) && (team.is_none() || ship.team != team))
    .filter(|ship| !contacts.iter().any(|contact| contact.name == ship.get_name()))
    .filter_map(|blip| {
      let (nearest, distance) = own
        .iter()
        .map(|ship| (ship.get_name(), (ship.get_position() - blip.get_position()).magnitude()))
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
      Some(Blip {
        name: blip.get_name().to_string(),
        position: blip.get_position(),
        velocity: blip.get_velocity(),
        nearest: nearest.to_string(),
        distance,
      })
    })
    .collect();
  let enemy_ships: Vec<&Ship> = ships
    .iter()
    .filter(|ship| !ours(ship) && (team.is_none() || ship.team != team))
    .collect();
  let watchers = watchers_of(&own, &enemy_ships);
  let (engagements, exposures) = tables(&own, &ships, &contacts);

  let side_names: BTreeSet<&str> = own.iter().map(Entity::get_name).collect();
  let mut missiles: Vec<Missile> = world
    .missiles
    .values()
    .map(|missile| missile.read().unwrap().clone())
    .filter(|missile| side_names.contains(missile.target.as_str()) || side_names.contains(missile.source.as_str()))
    .collect();
  missiles.sort_by(|a, b| a.get_name().cmp(b.get_name()));
  let mut planets: Vec<Planet> = world.planets.values().map(|planet| planet.read().unwrap().clone()).collect();
  planets.sort_by(|a, b| a.get_name().cmp(b.get_name()));

  Observation {
    round: world.round,
    team,
    own,
    allies,
    contacts,
    blips,
    watchers,
    planets,
    missiles,
    engagements,
    exposures,
  }
}

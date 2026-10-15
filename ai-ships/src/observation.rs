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
//! An enemy's crew is not visible, so their side of the table assumes
//! [`ASSUMED_ENEMY_GUNNERY`] at every mount.

use std::collections::BTreeSet;

use callisto::entity::{Entities, Entity, Vec3};
use callisto::missile::Missile;
use callisto::planet::Planet;
use callisto::ship::{Range, Sensors, Ship, Team, Weapon};
use callisto::tactics::{band_between, engagement, BandOdds};
use cgmath::InnerSpace;
use serde::Serialize;

/// A ship this side has detected, as its sensor station shows it.
#[derive(Debug, Clone, Serialize)]
pub struct Contact {
  pub name: String,
  pub design: String,
  /// The hull its class is built with. Not what it has left: that the
  /// sensor station does not show.
  pub design_hull: u32,
  pub team: Option<Team>,
  pub position: Vec3,
  pub velocity: Vec3,
  /// The thrust it can make now.
  pub current_maneuver: u8,
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

/// The gunner skill a side assumes of an enemy whose crew it cannot see.
pub const ASSUMED_ENEMY_GUNNERY: i32 = 1;

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
  pub planets: Vec<Planet>,
  /// Missiles aimed at our ships, or fired by them.
  pub missiles: Vec<Missile>,
  /// The odds each way, for every pairing of our ships with a contact.
  pub engagements: Vec<Engagement>,
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
        design_hull: contact.design.hull,
        team: contact.team,
        position: contact.get_position(),
        velocity: contact.get_velocity(),
        current_maneuver: contact.current_maneuver,
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

  let engagements = own
    .iter()
    .flat_map(|ship| {
      ships
        .iter()
        .filter(|contact| contacts.iter().any(|known: &Contact| known.name == contact.get_name()))
        .map(move |contact| Engagement {
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
    planets,
    missiles,
    engagements,
  }
}

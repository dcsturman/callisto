use cgmath::{InnerSpace, Vector3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::payloads::{EffectMsg, EngineerActionResult, MessageCategory};
use rand::seq::SliceRandom;
use rand::RngCore;

use serde_with::serde_as;
use std::collections::{HashMap, HashSet};
use std::fmt::Debug;
use std::sync::{Arc, RwLock};
use tracing::{event, Level};

use crate::action::{
  boost_for_detection, boost_for_engineer, boost_for_sensor, BoostMap, BoostTarget, ShipAction, ShipActionList,
};
use crate::combat::{
  apply_crit, attack, build_point_defense_tallies, create_sand_counts, do_fire_actions, find_range_band,
  interception_cost, roll_battery_pool, roll_dice, roll_point_defense_pool, roll_screen_pool, HitMods,
  STANDARD_ROLL_THRESHOLD,
};
use crate::crew::Crew;
use crate::missile::Missile;
use crate::planet::{Planet, PlanetVisualEffect};
use crate::read_local_or_cloud_file;
use crate::rules_tables::{
  countermeasures_mod, detection_modifier_terms, detection_modifiers, Emissions, SENSOR_QUALITY_MOD,
};
use crate::ship::get_ship_templates_snapshot;
use crate::ship::Weapon;
use crate::ship::{
  with_ship_templates_for_deserialization, BridgeStation, FlightPlan, Range, Ship, ShipDesignTemplate, ShipSystem,
};

#[allow(unused_imports)]
use crate::{debug, error, info, warn, LOG_FILE_USE};

pub const DELTA_TIME: u64 = 360;
pub const DELTA_TIME_F64: f64 = 360.0;

pub const DEFAULT_ACCEL_DURATION: u64 = 50000;
// We will use 4 sig figs for every physics constant we import.
// This is the value of 1 (earth) gravity in m/s^2
pub const G: f64 = 9.807_000_000;
pub type Vec3 = Vector3<f64>;

pub trait Entity: Debug + PartialEq + Serialize + Send + Sync {
  fn get_name(&self) -> &str;
  fn set_name(&mut self, name: String);
  fn get_position(&self) -> Vec3;
  fn set_position(&mut self, position: Vec3);
  fn get_velocity(&self) -> Vec3;
  fn set_velocity(&mut self, velocity: Vec3);
  fn update(&mut self) -> Option<UpdateAction>;
}

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateAction {
  ShipImpact { ship: String, missile: String },
  ExhaustedMissile { name: String },
  ShipDestroyed,
}

#[serde_as]
#[derive(Default)]
pub struct Entities {
  pub ships: HashMap<String, Arc<RwLock<Ship>>>,
  pub missiles: HashMap<String, Arc<RwLock<Missile>>>,
  pub planets: HashMap<String, Arc<RwLock<Planet>>>,
  pub next_missile_id: u32,

  // Actions queued up for when the turn ends.
  // They are more ephemeral than the objects above, but are global state
  // so we store them here so that Entities the single global-state object for a server.
  pub actions: ShipActionList,
  pub metadata: MetaData,

  // Basename of the scenario file this Entities was loaded from (e.g.
  // "planetfun.json"). Empty for scenarios created from scratch in the builder.
  // Stored separately from MetaData because the file name is the disk identity,
  // distinct from the human-readable display name in `metadata.name`. Not
  // serialized into scenario files (the file knows its own name) but is sent
  // over the WS so the client can pre-populate save dialogs.
  pub filename: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct MetaData {
  // All fields default to empty so legacy scenario files (e.g. ones written
  // before `owner` existed) still deserialize. Without these defaults a single
  // missing field drops the whole scenario from the picker — saw exactly that
  // on canary where existing GCS scenarios predated the owner field.
  #[serde(default)]
  pub name: String,
  #[serde(default)]
  pub description: String,
  #[serde(default)]
  pub owner: String,
}

impl PartialEq for Entities {
  fn eq(&self, other: &Self) -> bool {
    self.ships.len() == other.ships.len()
      && self.missiles.len() == other.missiles.len()
      && self.planets.len() == other.planets.len()
      && self.ships.keys().all(|k| other.ships.contains_key(k))
      && self.missiles.keys().all(|k| other.missiles.contains_key(k))
      && self.planets.keys().all(|k| other.planets.contains_key(k))
      && self
        .ships
        .keys()
        .all(|k| self.ships[k].read().unwrap().eq(&other.ships[k].read().unwrap()))
      && self
        .missiles
        .keys()
        .all(|k| self.missiles[k].read().unwrap().eq(&other.missiles[k].read().unwrap()))
      && self
        .planets
        .keys()
        .all(|k| self.planets[k].read().unwrap().eq(&other.planets[k].read().unwrap()))
  }
}

/// The effect reported when a ship is ordered to act on something it cannot see.
///
/// Phrased for the referee log rather than as an error: the order was given,
/// the crew simply has nothing to aim at.
/// One line describing a sensor check: what was rolled, what modified it, and
/// how it came out.
///
/// Emitted for every check actually made, hit or miss. A referee watching a
/// stealth ship stay hidden for six rounds wants to know whether the rolls were
/// close or whether the target was never findable at all, and that is not
/// something you can infer from silence.
fn detection_roll_effect(
  observer: &str, target: &str, roll: u8, dm: i16, total: i32, outcome: &str, terms: &[(&str, i16)],
) -> EffectMsg {
  // Only the terms that did something; a quiet target keeps the line short.
  let breakdown = terms
    .iter()
    .filter(|(_, value)| *value != 0)
    .map(|(name, value)| format!("{name} {value:+}"))
    .collect::<Vec<_>>()
    .join(", ");
  let breakdown = if breakdown.is_empty() {
    String::new()
  } else {
    format!(" ({breakdown})")
  };
  // Opens like the other checks Callisto reports — "with roll N and DM N" —
  // but ends on the total against the target number rather than on Effect.
  // Detection is pass or fail: the margin buys nothing here, unlike jamming,
  // where Effect decides how many missiles die.
  EffectMsg::about(
    observer,
    MessageCategory::Detection,
    format!(
      "{observer} sensor check on {target} with roll {roll} and DM {dm:+}{breakdown} for a total of {total} against 8: {outcome}."
    ),
  )
}

/// An engineer's check, reading like the sensop's: the roll, the DM with each
/// term that did something named, and the total against the target number.
/// Returns the total, floored at zero, with that text.
fn engineer_check(roll: u8, terms: &[(&str, i16)], target: u8) -> (u8, String) {
  let dm: i16 = terms.iter().map(|(_, value)| value).sum();
  let total = u8::try_from((i16::from(roll) + dm).max(0)).unwrap_or(u8::MAX);
  let breakdown = terms
    .iter()
    .filter(|(_, value)| *value != 0)
    .map(|(name, value)| format!("{name} {value:+}"))
    .collect::<Vec<_>>()
    .join(", ");
  let breakdown = if breakdown.is_empty() {
    String::new()
  } else {
    format!(" ({breakdown})")
  };
  (
    total,
    format!("with roll {roll} and DM {dm:+}{breakdown} for a total of {total} against {target}"),
  )
}

/// One sensor-operator check against a fixed target number.
///
/// Same shape as `detection_roll_effect` deliberately: every check a sensop
/// makes should read the same way, so a referee can see why one failed without
/// reaching for the debug log.
fn sensor_check_effect(
  actor: &str, action: &str, other: Option<&str>, roll: u8, dm: i16, target_number: i16, outcome: &str,
) -> EffectMsg {
  let total = i16::from(roll) + dm;
  // Jamming inbound missiles is the one sensop check with no second ship in it.
  let subject = other.map_or_else(|| action.to_string(), |other| format!("{action} {other}"));
  EffectMsg::about(
    actor,
    MessageCategory::Detection,
    format!(
      "{actor} {subject} with roll {roll} and DM {dm:+} for a total of {total} against {target_number}: {outcome}."
    ),
  )
}

/// An opposed sensor check, where there is no target number -- only the other
/// ship's roll. Both sides are shown, because "failed" against a 12 and against
/// a 4 are very different pieces of information.
fn opposed_check_effect(
  actor: &str, action: &str, other: &str, acting: (u8, i16), opposing: (u8, i16), outcome: &str,
) -> EffectMsg {
  let (roll, dm) = acting;
  let (other_roll, other_dm) = opposing;
  let mine = i16::from(roll) + dm;
  let theirs = i16::from(other_roll) + other_dm;
  EffectMsg::about(
    actor,
    MessageCategory::Detection,
    format!(
      "{actor} {action} {other} with roll {roll} and DM {dm:+} for {mine}, against roll {other_roll} and DM {other_dm:+} for {theirs}: {outcome}."
    ),
  )
}

pub(crate) fn no_contact_effect(ship_name: &str, target: &str, verb: &str) -> EffectMsg {
  EffectMsg::about(
    ship_name,
    MessageCategory::Detection,
    format!("{ship_name} has no sensor contact on {target} and cannot {verb} it."),
  )
}

impl Entities {
  #[must_use]
  pub fn new() -> Self {
    Entities {
      ships: HashMap::new(),
      missiles: HashMap::new(),
      planets: HashMap::new(),
      next_missile_id: 0,
      actions: vec![],
      metadata: MetaData::default(),
      filename: String::new(),
    }
  }

  #[must_use]
  pub fn len(&self) -> usize {
    self.ships.len() + self.missiles.len() + self.planets.len()
  }

  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.ships.is_empty() && self.missiles.is_empty() && self.planets.is_empty()
  }

  /// Do a deep copy and create and return the copy.
  ///
  /// # Errors
  /// Returns an error if [`fixup_pointers`](Self::fixup_pointers) fails on
  /// the copy because a missile target or planet primary references a name
  /// that no longer exists in the source. This signals an inconsistent
  /// `Entities` state — `Player::remove` is responsible for keeping these
  /// references intact when removing ships or planets.
  pub fn deep_copy(&self) -> Result<Self, String> {
    let mut entities = Entities::new();
    self.deep_copy_into(&mut entities)?;
    Ok(entities)
  }

  /// Do a deep copy from one `Entities` to another.
  ///
  /// # Errors
  /// Returns an error if `fixup_pointers` fails on the destination. Prior
  /// to this guard the code unwrapped here and a single dangling reference
  /// would take down the whole tokio worker (and cascade through
  /// `main.rs::try_send` to crash the container). Callers in the wire
  /// path should propagate the error into a `ResponseMsg::Error` instead
  /// of letting it bubble up.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship, missile, or planet.
  pub fn deep_copy_into(&self, dest: &mut Self) -> Result<(), String> {
    dest.ships.clear();
    dest.missiles.clear();
    dest.planets.clear();

    for ship in self.ships.values() {
      let ship = ship.read().unwrap();
      dest
        .ships
        .insert(ship.get_name().to_string(), Arc::new(RwLock::new(ship.clone())));
    }

    for missile in self.missiles.values() {
      let missile = missile.read().unwrap();
      dest
        .missiles
        .insert(missile.get_name().to_string(), Arc::new(RwLock::new(missile.clone())));
    }

    for planet in self.planets.values() {
      let planet = planet.read().unwrap();
      dest
        .planets
        .insert(planet.get_name().to_string(), Arc::new(RwLock::new(planet.clone())));
    }

    dest.next_missile_id = self.next_missile_id;
    dest.actions.clone_from(&self.actions);

    // Drop anything whose target has left play before resolving pointers. The
    // live state should never contain an orphan, but if it does, failing here
    // makes every request for entities fail with it: the client stops getting
    // updates entirely and sits on stale state, still showing ships that are
    // gone. Losing a missile is a far better outcome than losing the session.
    // Scenario *files* are still validated strictly, in `parse_bytes_...`.
    dest.prune_orphaned_missiles();
    dest.fixup_pointers()?;
    dest.reset_gravity_wells();
    Ok(())
  }

  // Build a deep clone of the ships. It does not need to be thread safe so we can drop the use of Arc
  pub(crate) fn ship_deep_copy(&self) -> HashMap<String, Ship> {
    self
      .ships
      .iter()
      .map(|(name, ship)| (name.clone(), ship.read().unwrap().clone()))
      .collect()
  }

  /// Load a scenario file.  A scenario file is just a JSON encoding of a set of entities.
  /// After loading the file, the pointers are fixed up and the gravity wells are reset.
  /// # Arguments
  /// * `file_name` - The name of the file to load.
  ///
  /// # Errors
  /// Returns an error if the file cannot be read or the file cannot be parsed (e.g. bad JSON)
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship, missile, or planet.
  pub async fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
    Self::load_from_file_with_ship_templates(file_name, get_ship_templates_snapshot()).await
  }

  /// Parse a scenario from in-memory bytes using the global ship-template
  /// snapshot. Caller provides the path-or-basename for diagnostics and so
  /// the loaded `Entities` can be stamped with its filename.
  ///
  /// Splitting this out from `load_from_file` lets the scenario watcher
  /// salvage `metadata.owner` from a file whose full parse failed — it
  /// reads the bytes once and tries both attempts.
  ///
  /// # Errors
  /// Returns an error if the JSON cannot be parsed (e.g. an unknown
  /// design name or malformed body) or pointer fix-up fails.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship, missile, or planet.
  pub fn load_from_bytes(scenario_contents: &[u8], file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
    Self::parse_bytes_with_ship_templates(scenario_contents, file_name, get_ship_templates_snapshot())
  }

  /// Load a scenario file using the provided ship-template snapshot.
  ///
  /// This ensures all ships deserialized from the scenario point at the same
  /// template snapshot that the caller intends to associate with the scenario.
  ///
  /// # Errors
  /// Returns an error if the file cannot be read or the file cannot be parsed (e.g. bad JSON)
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship, missile, or planet.
  pub async fn load_from_file_with_ship_templates(
    file_name: &str, ship_templates: Arc<HashMap<String, Arc<ShipDesignTemplate>>>,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    event!(target: LOG_FILE_USE, Level::INFO, file_name, use = "Load scenario.");

    let scenario_contents = read_local_or_cloud_file(file_name).await?;
    Self::parse_bytes_with_ship_templates(&scenario_contents, file_name, ship_templates)
  }

  fn parse_bytes_with_ship_templates(
    scenario_contents: &[u8], file_name: &str, ship_templates: Arc<HashMap<String, Arc<ShipDesignTemplate>>>,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    let mut entities: Entities =
      with_ship_templates_for_deserialization(ship_templates, || serde_json::from_slice(scenario_contents))?;

    // Stamp the basename of the scenario file we loaded from. `file_name` may
    // be a full path ("./scenarios/sol.json" or "gs://bucket/sol.json") — we
    // only want the basename so the save dialog defaults match the picker.
    entities.filename = file_name.rsplit('/').next().unwrap_or(file_name).to_string();

    entities.fixup_pointers()?;
    entities.reset_gravity_wells();

    // Fix all the initial current values in the ship based on the design.
    // This does limit our ability to load wounded ships into a scenario.  If we need
    // that we can add it later.
    for ship in entities.ships.values_mut() {
      ship.write().unwrap().fixup_current_values();
    }

    #[cfg(not(coverage))]
    for ship in entities.ships.values() {
      debug!("Loaded entity {:?}", ship.read().unwrap());
    }

    #[cfg(not(coverage))]
    for planet in entities.planets.values() {
      debug!("Loaded entity {:?}", planet.read().unwrap());
    }

    #[cfg(not(coverage))]
    for missile in entities.missiles.values() {
      debug!("Loaded entity {:?}", missile.read().unwrap());
    }
    assert!(entities.validate(), "Scenario file failed validation");
    Ok(entities)
  }

  /// Serialize this `Entities` value into the on-disk scenario JSON format.
  ///
  /// The wire-level [`Serialize`] impl (used by `EntityResponse` over the WebSocket)
  /// intentionally omits `metadata` and `actions`, since neither is meaningful to a
  /// connected client. Scenario files on disk include `metadata` and follow the
  /// shape `{ metadata, ships, planets, missiles? }` — this helper emits exactly
  /// that, with empty arrays elided.
  ///
  /// # Errors
  /// Returns a `serde_json::Error` if any contained ship/missile/planet fails to serialize.
  ///
  /// # Panics
  /// Panics if a ship/missile/planet `RwLock` is poisoned.
  pub fn to_scenario_file_json(&self) -> Result<Vec<u8>, serde_json::Error> {
    #[derive(Serialize)]
    struct ScenarioFile<'a> {
      metadata: &'a MetaData,
      #[serde(skip_serializing_if = "Vec::is_empty")]
      ships: Vec<crate::ship::Ship>,
      #[serde(skip_serializing_if = "Vec::is_empty")]
      planets: Vec<Planet>,
      #[serde(skip_serializing_if = "Vec::is_empty")]
      missiles: Vec<Missile>,
    }

    let mut ships: Vec<_> = self.ships.values().map(|s| s.read().unwrap().clone()).collect();
    let mut planets: Vec<_> = self.planets.values().map(|p| p.read().unwrap().clone()).collect();
    let mut missiles: Vec<_> = self.missiles.values().map(|m| m.read().unwrap().clone()).collect();
    // Stable ordering matches the existing wire-level Serialize impl, so file diffs are clean.
    ships.sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
    planets.sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
    missiles.sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());

    let payload = ScenarioFile {
      metadata: &self.metadata,
      ships,
      planets,
      missiles,
    };
    serde_json::to_vec_pretty(&payload)
  }

  /// Add a ship to the entities.
  ///
  /// # Arguments
  /// * `name` - The name of the ship.
  /// * `position` - The position of the ship.
  /// * `velocity` - The velocity of the ship.
  /// * `design` - The design of the ship.
  /// * `crew` - The crew of the ship.
  /// * `weapons` - The ship's armament.  `None` means it uses its design's weapons.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read an existing ship that is being modified.
  pub fn add_ship(
    &mut self, name: String, position: Vec3, velocity: Vec3, design: &Arc<ShipDesignTemplate>, crew: Option<Crew>,
    weapons: Option<Vec<Weapon>>,
  ) {
    // Cloning the Arc (not the ship) releases the borrow on `self.ships`, which
    // `clear_weapon_actions` needs back as `&mut self` below.
    let Some(existing) = self.ships.get(&name).cloned() else {
      // Create a new ship and add it to the ship table
      let ship = Arc::new(RwLock::new(Ship::new(name.clone(), position, velocity, design, crew, weapons)));
      self.ships.insert(name, ship);
      return;
    };

    let armament_changed = {
      // If the ship already exists, then just update appropriate values.
      let mut ship = existing.write().unwrap();
      ship.set_position(position);
      ship.set_velocity(velocity);
      // A ship pointed at a different design is a different ship, so its
      // current values come from the new design rather than being carried over.
      // `fixup_current_values` only ever raises them, so without this a swap to
      // a smaller hull kept the larger one's hull, thrust and sensors.
      let design_changed = ship.design.name != design.name;
      ship.design = design.clone();
      // `None` means the scenario named no crew, so the design's stands -- the
      // same rule the load path uses. Only an explicit crew replaces it.
      if let Some(crew) = crew {
        ship.set_crew(crew);
      }
      // Owned because `set_weapons` is about to replace what `weapons()` borrows.
      let before = ship.weapons().to_vec();
      // Set the armament before the fixup so `active_weapons` is sized to it.
      ship.set_weapons(weapons);
      if design_changed {
        ship.reset_current_values_to_design();
      } else {
        ship.fixup_current_values();
      }
      ship.weapons() != before
    };

    // Queued actions address weapons by index, so re-arming a ship leaves any
    // of its queued fire orders pointing at a different weapon — or past the
    // end of the list.  Editing armament mid-turn should be rare (this is a
    // design-phase dialog), but the stale ids are silently wrong when it
    // happens, so drop them.
    if armament_changed {
      self.clear_weapon_actions(&name);
    }
  }

  /// Drop every queued action that addresses a weapon on `ship_name`.
  ///
  /// That is the ship's own fire and point-defense orders, plus any boost a
  /// captain queued against one of its weapons — those live under the
  /// *captain's* ship, so every action list has to be swept, not just this
  /// ship's.  Non-weapon actions (pilot, sensor, engineer) are untouched.
  fn clear_weapon_actions(&mut self, ship_name: &str) {
    for (owner, actions) in &mut self.actions {
      actions.retain_mut(|action| match action {
        ShipAction::FireAction { .. } | ShipAction::PointDefenseAction { .. } | ShipAction::DeleteFireAction { .. } => {
          owner != ship_name
        }
        ShipAction::LeadershipCheck { boosts } => {
          boosts.retain(|boost| {
            !matches!(boost,
            BoostTarget::Fire { ship, .. } | BoostTarget::PointDefense { ship, .. } if ship == ship_name)
          });
          !boosts.is_empty()
        }
        _ => true,
      });
    }
    // A ship with nothing left queued should not linger as an empty entry.
    self.actions.retain(|(_, actions)| !actions.is_empty());
  }

  /// Add a planet to the entities.
  ///
  /// # Arguments
  /// * `name` - The name of the planet.
  /// * `position` - The position of the planet.
  /// * `color` - The color of the planet.
  /// * `primary` - The name of the primary planet.  If None, the planet is a star.
  /// * `radius` - The radius of the planet.
  /// * `mass` - The mass of the planet.
  ///
  /// # Errors
  /// Returns an error if the primary planet is not found or if for some reason a pointer to the primary planet cannot be created.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a planet.
  #[allow(clippy::too_many_arguments)]
  pub fn add_planet(
    &mut self, name: String, position: Vec3, color: String, primary: Option<String>, radius: f64, mass: f64,
    visual_effects: Vec<PlanetVisualEffect>,
  ) -> Result<(), String> {
    debug!(
      "Add planet {} with position {:?},  color {:?}, primary {}, radius {:?}, mass {:?}, ",
      name,
      position,
      color,
      primary.as_ref().unwrap_or(&String::from("None")),
      radius,
      mass
    );

    let (primary_ptr, dependency) = if let Some(primary_name) = &primary {
      let primary = self
        .planets
        .get(primary_name)
        .ok_or_else(|| format!("Primary planet {primary_name} not found for planet {name}."))?;

      (Some(primary.clone()), primary.read().unwrap().dependency + 1)
    } else {
      (None, 0)
    };

    // A safety check to ensure we never have a pointer without a name of a primary or vis versa.
    if primary_ptr.is_some() ^ primary.is_some() {
      return Err(format!(
        "Planet {name} has a primary pointer but no primary name or vice versa."
      ));
    }

    if let Some(existing_planet) = self.planets.get(&name) {
      let mut planet = existing_planet.write().unwrap();
      planet.set_position(position);
      planet.color = color;
      planet.primary = primary;
      planet.primary_ptr = primary_ptr;
      planet.radius = radius;
      planet.mass = mass;
      planet.dependency = dependency;
      planet.visual_effects = visual_effects;
      planet.reset_gravity_wells();
      let updated_velocity = if planet.primary_ptr.is_some() {
        planet.calculate_rotational_velocity()?
      } else {
        Vec3::new(0.0, 0.0, 0.0)
      };
      planet.set_velocity(updated_velocity);

      debug!("Updated existing planet {:?}", planet);
    } else {
      let mut entity = Planet::new(name.clone(), position, color, radius, mass, primary, &primary_ptr, dependency);
      entity.visual_effects = visual_effects;

      debug!("Added planet with fixed gravity wells {:?}", entity);
      self.planets.insert(name, Arc::new(RwLock::new(entity)));
    }

    Ok(())
  }

  /// Rename a ship or planet in place, preserving the underlying entity
  /// (and any references to its Arc). Missile renames are not supported.
  ///
  /// On a planet rename, any other planet whose `primary` field points at
  /// the old name is also updated so the parent-child chain stays
  /// consistent.
  ///
  /// # Errors
  ///
  /// Returns `Err` if:
  /// * `new_name` is empty after trimming,
  /// * `new_name` is already in use by any ship, planet, or missile,
  /// * `current` does not match any ship or planet,
  /// * `current` matches a missile (rename not supported for missiles).
  ///
  /// Renaming an entity to its current name is a no-op and returns `Ok`.
  ///
  /// # Panics
  /// Panics if a ship or planet `RwLock` is poisoned.
  pub fn rename(&mut self, current: &str, new_name: &str) -> Result<String, String> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
      return Err("New name cannot be empty.".to_string());
    }
    if current == trimmed {
      return Ok(format!("Entity {current} renamed (no change)."));
    }
    if self.ships.contains_key(trimmed) || self.planets.contains_key(trimmed) || self.missiles.contains_key(trimmed) {
      return Err(format!("Name '{trimmed}' is already in use."));
    }
    if let Some(ship_arc) = self.ships.remove(current) {
      ship_arc.write().unwrap().set_name(trimmed.to_string());
      self.ships.insert(trimmed.to_string(), ship_arc);
      // Everyone tracking the old name has to follow it, or the rename
      // silently drops their contact and sensor lock.
      self.rename_ship_references(current, trimmed);
      return Ok(format!("Renamed ship {current} to {trimmed}."));
    }
    if let Some(planet_arc) = self.planets.remove(current) {
      planet_arc.write().unwrap().set_name(trimmed.to_string());
      self.planets.insert(trimmed.to_string(), planet_arc);
      // Re-parent any planets that referenced the old name.
      for other in self.planets.values() {
        let mut other = other.write().unwrap();
        if other.primary.as_deref() == Some(current) {
          other.primary = Some(trimmed.to_string());
        }
      }
      return Ok(format!("Renamed planet {current} to {trimmed}."));
    }
    if self.missiles.contains_key(current) {
      return Err(format!("Cannot rename missile {current}."));
    }
    Err(format!("Entity {current} not found."))
  }

  /// Launch a missile from a ship at a ship.
  ///
  /// # Arguments
  /// * `source` - The ship that is launching the missile.
  /// * `target` - The ship that is the target of the missile.
  ///
  /// # Errors
  /// Returns an error if the source ship is not found.
  /// Returns an error if the target ship is not found.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn launch_missile(&mut self, source: &str, target: &str, weapon: Weapon) -> Result<(), String> {
    // Could use a random number generator here for the name but that makes tests flakey (random)
    // So this counter used to distinguish missiles between the same source and target
    let id = self.next_missile_id;
    self.next_missile_id += 1;

    let name = format!("{source}::{target}::{id:X}");
    let source_ptr = self
      .ships
      .get(source)
      .ok_or_else(|| format!("Missile source {source} not found for missile {name}."))?
      .clone();

    let target_ptr = self
      .ships
      .get(target)
      .ok_or_else(|| format!("Target {target} not found for missile {name}."))?
      .clone();

    let source_ship = source_ptr.read().unwrap();
    let target_ship = target_ptr.read().unwrap();
    let direction = (target_ship.get_position() - source_ship.get_position()).normalize();
    let offset = 10000.0 * direction;

    let target_ptr = target_ptr.clone();

    let position = source_ship.get_position() + offset;
    let velocity = source_ship.get_velocity();

    let entity = Missile::new(
      name.clone(),
      source.to_string(),
      target.to_string(),
      target_ptr,
      position,
      velocity,
      crate::missile::DEFAULT_BURN,
      weapon,
    );

    debug!("(Entities.launch_missile) Added missile {}", &name);
    self.missiles.insert(name, Arc::new(RwLock::new(entity)));
    Ok(())
  }

  /// Set the flight plan.
  ///
  /// # Returns
  /// `Ok(())` if the flight plan was set successfully.
  ///
  /// # Arguments
  /// * `name` - The name of the ship to set the flight plan for.
  /// * `plan` - The flight plan to set.
  ///
  /// # Errors
  /// Returns an error if the ship is not found.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn set_flight_plan(&mut self, name: &str, plan: &FlightPlan) -> Result<(), String> {
    if let Some(entity) = self.ships.get_mut(name) {
      entity.write().unwrap().set_flight_plan(plan)
    } else {
      Err(format!("Could not set acceleration for non-existent entity {name}"))
    }
  }

  /// Process all fire actions and turn them into either missile launches or attacks.
  ///
  /// # Arguments
  /// * `fire_actions` - The fire actions to process.
  /// * `ship_snapshot` - A snapshot of all ships state at the start of the round.  Having this snapshot avoid trying to lookup
  ///   a ship that was destroyed earlier in the round.
  /// * `rng` - The random number generator to use.
  ///
  /// # Returns
  /// A list of all the effects resulting from the fire actions.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn fire_actions(
    &mut self, fire_actions: &[(String, Vec<ShipAction>)], point_defense_actions: &[(String, Vec<ShipAction>)],
    ship_snapshot: &HashMap<String, Ship>, boost_map: &BoostMap, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    // Nothing fires without fire control: not the guns, not point defence, not
    // sand. Judged on the snapshot, since everyone fires at once.
    let fire_control = |name: &String| {
      ship_snapshot
        .get(name)
        .is_none_or(|ship| ship.station_working(BridgeStation::FireControl))
    };
    // One message per ship, in name order so seeded runs read the same.
    let grounded: std::collections::BTreeSet<&String> = fire_actions
      .iter()
      .chain(point_defense_actions)
      .filter(|(name, actions)| !actions.is_empty() && !fire_control(name))
      .map(|(name, _)| name)
      .collect();
    let mut battery_effects: Vec<EffectMsg> = grounded
      .into_iter()
      .map(|name| {
        EffectMsg::about(
          name,
          MessageCategory::Critical,
          format!("{name} cannot fire: its fire control station is out."),
        )
      })
      .collect();
    let fire_actions: Vec<_> = fire_actions.iter().filter(|(name, _)| fire_control(name)).cloned().collect();
    let point_defense_actions: Vec<_> = point_defense_actions
      .iter()
      .filter(|(name, _)| fire_control(name))
      .cloned()
      .collect();
    let point_defense_actions = point_defense_actions.as_slice();

    // Create a snapshot of all the sand capabilities of each ship.
    let mut sand_counts = create_sand_counts(ship_snapshot, point_defense_actions);

    // Point-defence batteries are automatic: they need no action, no gunner and
    // no decision, so every ship that has one gets a pool whether or not its
    // crew queued anything.  That is why this is a separate pass over all ships
    // rather than part of the loop above.
    //
    // Iterate in name order.  `self.ships` is a HashMap, and rolling in map
    // order would make the seeded integration tests non-reproducible -- the
    // same reason missiles are sorted before resolution below.
    let mut battery_ships: Vec<String> = self.ships.keys().cloned().collect();
    battery_ships.sort_unstable();
    for name in battery_ships {
      let Some(ship) = self.ships.get(&name) else {
        continue;
      };
      let mut ship = ship.write().unwrap();
      // Screens are rolled in the same pass and for the same reason: they are
      // per-round, need no queued action, and every ship that has one gets them.
      let screens = roll_screen_pool(&ship, rng);
      ship.set_screen_pool(screens);

      let pool = if ship.station_working(BridgeStation::FireControl) {
        roll_battery_pool(&ship, rng)
      } else {
        0
      };
      // A set rather than an add: this pass runs first, covers every ship, and
      // so is also what clears any value left over from the previous round.
      ship.set_point_defense_pool(pool);
      if pool > 0 {
        debug!("(Entities.fire_actions) {name}'s point defence batteries will intercept {pool} missile(s).");
        battery_effects.push(EffectMsg::message(format!(
          "{name}'s point defence batteries will intercept up to {pool} missile(s) this round."
        )));
      }
    }

    // From our list of point defense actions, go into each ship and build up a proper list of usable point defense actions.
    // These then get used and cleared in `Entities::update_all` after all missiles have been updated.
    for (defender, actions) in point_defense_actions {
      let Some(ship) = self.ships.get(defender) else {
        warn!(
          "(Entities.fire_actions) Cannot find attacker {} for point defense actions.",
          defender
        );
        continue;
      };

      let mut ship = ship.write().unwrap();
      let tallies = build_point_defense_tallies(&ship, actions, boost_map, defender);

      // Every gunner makes one check per round and their Effects add up
      // (Core Rulebook p. 171), so roll the whole list now rather than one
      // weapon per incoming missile.  Nothing pairs a gunner with a particular
      // missile, so there is no reason to hold any of them back.
      let pool = roll_point_defense_pool(&tallies, rng);
      debug!("(Entities.fire_actions) {defender}'s gunners contribute {pool} point(s) of point defence this round.");
      ship.add_point_defense_pool(pool);
      ship.set_point_defense_list(tallies);
    }

    let effects = fire_actions
      .iter()
      .flat_map(|(attacker, actions)| {
        let Some(attack_ship) = ship_snapshot.get(attacker) else {
          warn!("Cannot find attacker {} for fire actions.", attacker);
          return vec![];
        };

        let (missiles, effects) =
          do_fire_actions(attack_ship, &mut self.ships, &mut sand_counts, actions, boost_map, rng);
        for missile in missiles {
          if let Err(msg) = self.launch_missile(&missile.source, &missile.target, missile.weapon) {
            warn!("Could not launch missile: {}", msg);
          }
        }
        effects
      })
      .collect::<Vec<EffectMsg>>();
    battery_effects.extend(effects);
    battery_effects
  }

  /// Check which ships are jump enabled.  This is done at the end of each round.  It is done
  /// by checking if the ship is more than 100 diameters (200 radii) away from every planet.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write a ship or read a planet.
  pub fn check_jump_enabled(&mut self) {
    // Check which ships are jump enabled.
    for ship in self.ships.values() {
      let readable_ship = ship.read().unwrap();
      // Is every planet more than 100 diameters (200 radii) away?
      let can_jump = self.planets.values().all(|planet| {
        let planet = planet.read().unwrap();
        (readable_ship.get_position() - planet.get_position()).magnitude() > planet.radius * 200.0
      });

      // If ship can jump, note it in entities.
      if !can_jump || readable_ship.current_jump == 0 {
        debug!(
          "(Entity.check_jump_enabled) Ship {} is NOT jump enabled.",
          readable_ship.get_name()
        );
        continue;
      }

      drop(readable_ship);

      let mut ship = ship.write().unwrap();
      ship.enable_jump();
      debug!("(Entity.check_jump_enabled) Ship {} jump enabled.", ship.get_name());
    }
  }

  /// Update all entities.  This is typically done at the end of a round to advance a turn.
  /// It returns all the effects resulting from the actions of the update.  This happens after `fire_actions`
  /// so all missiles should already be launched and will get to move here.  Update in the order:
  /// 1. Planets
  /// 2. Missiles
  /// 3. Ships
  ///
  /// # Arguments
  /// * `ship_snapshot` - A snapshot of the ships at the start of the round.  This is used to ensure that
  ///   any damage applied is simultaneous.  The snapshot is worked off of and real damage or other effects
  ///   are applied to the actual entities.
  /// * `rng` - A random number generator.
  ///
  /// # Panics
  /// Panics if the lock (read or write) cannot be obtained when reading any specific entity.
  #[allow(clippy::too_many_lines)]
  pub fn update_all(
    &mut self, ship_snapshot: &HashMap<String, Ship>, boost_map: &BoostMap, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    let mut planets = self.planets.values_mut().collect::<Vec<_>>();
    planets.sort_by(|a, b| {
      let a_ent = a.read().unwrap();
      let b_ent = b.read().unwrap();
      a_ent.dependency.cmp(&b_ent.dependency)
    });

    // If we have effects from planet updates this has to change and get a bit more complex (like missiles below)
    for planet in planets {
      planet.write().unwrap().update();
    }

    let mut cleanup_missile_list = Vec::<String>::new();

    // Creating this sorted list is necessary ONLY to ensure unit tests run consistently
    // If it ends up being slow we should take it out.
    let mut sorted_missiles = self.missiles.values().collect::<Vec<_>>();
    sorted_missiles.sort_by(|a, b| {
      let a_ent = a.read().unwrap();
      let b_ent = b.read().unwrap();
      a_ent.get_name().partial_cmp(b_ent.get_name()).unwrap()
    });

    // Now update all (remaining) missiles.
    let mut effects = sorted_missiles
      .into_iter()
      .filter_map(|missile| {
        let mut missile = missile.write().unwrap();
        let update = missile.update();
        let missile_name = missile.get_name();
        let missile_pos = missile.get_position();
        // Captured before the match, whose arm shadows `missile` with its name.
        let launcher = missile.weapon.clone();
        let Some(missile_source) = ship_snapshot.get(&missile.source) else {
          warn!(
            "(Entity.update_all) Cannot find source {} for missile. It may have been destroyed.",
            &missile.source
          );
          return None;
        };

        // We use UpdateAction vs just returning the effect so that the call to attack() stays at this level rather
        // than being embedded in the missile update code.  Also enables elimination of missiles.
        match update? {
          UpdateAction::ShipImpact { ship: target_name, missile } => {
            // Resolve the impact as an attack by the weapon that launched this
            // object, so a torpedo does a torpedo's 6D rather than a missile's 4D.
            debug!("(Entity.update_all) Missile impact on {} by missile {}.", target_name, missile);
            let target = self.ships.get(&target_name).map_or_else(
              || {
                warn!("Cannot find target {} for missile. It may have been destroyed.", target_name);
                None
              },
              |ship| Some(ship.clone()),
            );

            if let Some(target) = target {
              // For now assume all missiles are smart missiles.
              let smart_missile_bonus =
                i32::from(missile_source.design.tl.saturating_sub(target.read().unwrap().design.tl)).clamp(1, 6);

              debug!(
                "(Entity.update_all) Missile {} impacted target {} with smart missile bonus {} (attacker TL {}, target TL {}).",
                missile,
                target_name,
                smart_missile_bonus,
                missile_source.design.tl,
                target.read().unwrap().design.tl
              );
              let mut target = target.write().unwrap();

              // A torpedo costs two points where a missile costs one, so a
              // ship's point defence stops half as many of them.
              let cost = interception_cost(launcher.primary_kind());
              let stopped = target.take_interception(cost);

              // This stops the attack
              if stopped {
                debug!(
                  "(Entity.update_all) Missile {} destroyed by point defense by {}.",
                  missile, target_name
                );
                cleanup_missile_list.push(missile.clone());
                let what = String::from(&launcher.primary_kind());
                Some(vec![EffectMsg::ExhaustedMissile { position: target.get_position() }, EffectMsg::message(format!("{what} {missile} destroyed by {target_name}'s point defence"))])
              } else {
                // The attack gets through point defense
                // A launched object resolves as the gun that threw it.
                let Some(firing) = launcher.firing_default() else {
                  warn!("(Entity.update_all) Missile {missile} has no launching gun.");
                  return None;
                };
                let effects = attack(
                  HitMods {
                    smart: smart_missile_bonus,
                    ..HitMods::default()
                  },
                  0,
                  missile_source,
                  &mut target,
                  &firing,
                  // Missiles cannot do called shots
                  None,
                  boost_map,
                  rng,
                );
                cleanup_missile_list.push(missile);

                Some(effects)
              }
            } else {
              debug!(
                "(Entity.update_all) Missile {} exhausted at position {:?}.",
                missile, missile_pos
              );
              cleanup_missile_list.push(missile);
              Some(vec![EffectMsg::ExhaustedMissile { position: missile_pos }])
            }
          }
          UpdateAction::ExhaustedMissile { name } => {
            assert_eq!(name, missile_name);
            debug!("(Entity.update_all) Removing missile {}", name);
            cleanup_missile_list.push(name.clone());
            Some(vec![EffectMsg::ExhaustedMissile { position: missile_pos }])
          }
          UpdateAction::ShipDestroyed => {
            panic!("(Entity.update_all) Unexpected ShipDestroyed update during missile updates.")
          }
        }
      })
      .flatten()
      .collect::<Vec<_>>();

    let mut cleanup_ships_list = Vec::<String>::new();

    effects.append(
      &mut self
        .ships
        .values_mut()
        .filter_map(|ship| {
          let mut ship = ship.write().unwrap();
          let update = ship.update();
          // Missile attacks are done by this point so clear this up for the next round.
          ship.clear_point_defense();
          // Ion suppression is measured in rounds and the ship has now had its
          // actions, so run the clock down and give the Power back when it
          // lapses (High Guard p. 30).
          ship.tick_ion_recovery();
          let name = ship.get_name();
          let pos = ship.get_position();

          match update? {
            UpdateAction::ShipDestroyed => {
              debug!("(Entity.update_all) Ship {} destroyed at position {:?}.", name, pos);
              cleanup_ships_list.push(name.to_string());
              Some(vec![
                EffectMsg::ShipDestroyed { position: pos },
                EffectMsg::about(name, MessageCategory::Destruction, format!("{name} destroyed.")),
              ])
            }
            update => panic!("(Entity.update_all) Unexpected update {update:?} during ship updates."),
          }
        })
        .flatten()
        .collect::<Vec<_>>(),
    );

    for name in &cleanup_missile_list {
      debug!("(Entity.update_all) Removing missile {}", name);
      self.missiles.remove(name);
    }

    for name in &cleanup_ships_list {
      debug!("(Entity.update_all) Removing ship {}", name);
      self.ships.remove(name);
    }
    if !cleanup_ships_list.is_empty() {
      self.prune_ship_references();
      // Anything still flying at a ship that just died has nothing to hit.
      effects.append(&mut self.prune_orphaned_missiles());
    }

    // Update which ships are jump enabled
    self.check_jump_enabled();
    effects
  }

  /// Do all sensor actions.  These activities are done before any combat in a round
  /// as they impact combat in that round (remove missiles, etc).
  ///
  /// # Arguments
  /// * `actions` - The actions to perform, already reduced to just the sensor actions.
  /// * `rng` - The random number generator to use.
  ///
  /// # Returns
  /// A list of all the effects resulting from the sensor actions.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn sensor_actions(
    &mut self, actions: &[(String, Vec<ShipAction>)], boost_map: &BoostMap, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    // First build a table that, for each ship, notes all the ships that have senor locks on it (i.e. we're reversing
    // the structure).  That is for any ship name (entry), provide a list of every ship that has a sensor lock on the entry.
    // We do this once, up front, to avoid rebuilding on each ShipAction::BreakSensorLock action.
    let mut reverse_sensor_locks = HashMap::<String, Vec<String>>::new();
    for ship in self.ships.values() {
      for sensor_lock in &ship.read().unwrap().sensor_locks {
        reverse_sensor_locks
          .entry(sensor_lock.clone())
          .or_default()
          .push(ship.read().unwrap().get_name().to_string());
      }
    }

    let mut effects = Vec::<EffectMsg>::new();

    for (ship_name, actions) in actions {
      if actions.is_empty() {
        continue;
      }
      if let Some(effect) = self.station_down_effect(ship_name, BridgeStation::Sensors, "take sensor actions") {
        effects.push(effect);
        continue;
      }
      let boost = boost_for_sensor(boost_map, ship_name);
      // Process the actions for each ship.
      for action in actions {
        effects.append(&mut match action {
          ShipAction::JamMissiles => self.jam_missiles(ship_name, boost, rng),
          ShipAction::BreakSensorLock { target } => {
            self.break_sensor_lock(ship_name, target, &reverse_sensor_locks, boost, rng)
          }

          ShipAction::SensorLock { target } => {
            if !self.ships.contains_key(target) {
              warn!("(Entity.do_sensor_actions) Cannot find target {} for sensor lock.", target);
              continue;
            }
            // A lock is deliberate, continuous illumination, which a ship
            // running dark is by definition not doing. High Guard p. 77:
            // pinpointing a ship "requires the use of active sensors".
            if !self.has_active_sensors(ship_name) {
              effects.push(EffectMsg::about(
                ship_name,
                MessageCategory::Detection,
                format!("{ship_name} is running dark and cannot lock onto {target}."),
              ));
              continue;
            }
            if !self.has_contact(ship_name, target) {
              effects.push(no_contact_effect(ship_name, target, "lock onto"));
              continue;
            }
            self.sensor_lock(ship_name, target, boost, rng)
          }
          ShipAction::JamComms { target } => {
            if !self.ships.contains_key(target) {
              warn!("(Entity.do_sensor_actions) Cannot find target {} for jamming comms.", target);
              continue;
            }
            if !self.has_contact(ship_name, target) {
              effects.push(no_contact_effect(ship_name, target, "jam"));
              continue;
            }
            self.jam_comms(ship_name, target, boost, rng)
          }
          ShipAction::PointDefenseAction { .. }
          | ShipAction::FireAction { .. }
          | ShipAction::DeleteFireAction { .. }
          | ShipAction::Jump
          | ShipAction::OverloadDrive
          | ShipAction::OverloadPlant
          | ShipAction::Repair { .. }
          | ShipAction::LeadershipCheck { .. }
          | ShipAction::ClearSensorAction
          | ShipAction::ClearEngineerAction
          | ShipAction::ClearLeadershipCheck => {
            error!("(Entity.do_sensor_actions) Unexpected sensor action {action:?}");
            Vec::default()
          }
        });
      }
    }
    effects
  }

  /// DM applied to a sensor check made by `observer_name` against `target_name`.
  ///
  /// Covers both the tech-level difference and the target's stealth; see
  /// `rules_tables::detection_modifiers` for how High Guard separates them.
  fn sensor_detection_modifiers(&self, observer_name: &str, target_name: &str) -> i16 {
    let observer = self.ships.get(observer_name).unwrap().read().unwrap();
    let target = self.ships.get(target_name).unwrap().read().unwrap();

    detection_modifiers(observer.design.tl, target.design.tl, target.design.stealth)
  }

  // Quality modifiers are the level of sensors as well as skill of the crew
  fn sensor_quality_modifiers(&self, ship_name: &str) -> i16 {
    let ship = self.ships.get(ship_name).unwrap().read().unwrap();
    SENSOR_QUALITY_MOD[ship.current_sensors as usize] + i16::from(ship.get_crew().get_sensors())
  }

  fn sensor_lock(&mut self, ship_name: &String, target: &str, boost: i16, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    // First check if there is already a sensor lock and if so just return.
    if self
      .ships
      .get(ship_name)
      .is_some_and(|ship| ship.read().unwrap().sensor_locks.contains(&target.to_string()))
    {
      return Vec::default();
    }

    // Check if sensor lock is achieved.
    let roll = roll_dice(2, rng);
    let dm = self.sensor_quality_modifiers(ship_name) + self.sensor_detection_modifiers(ship_name, target) + boost;
    let check = i16::from(roll) + dm - 8;

    if check > 0 {
      // If there is sensor lock, record it.
      // Scope the write lock so we don't hold it - its the only place we need to write.
      {
        // The unwrap after the get is safe as if the ship didn't exist the `continue` up
        // above would have triggered.
        self
          .ships
          .get(ship_name)
          .unwrap()
          .write()
          .unwrap()
          .sensor_locks
          .push(target.to_string());
      }
      vec![sensor_check_effect(
        ship_name,
        "attempts a sensor lock on",
        Some(target),
        roll,
        dm,
        8,
        "lock established",
      )]
    } else {
      vec![sensor_check_effect(
        ship_name,
        "attempts a sensor lock on",
        Some(target),
        roll,
        dm,
        8,
        "no lock",
      )]
    }
  }

  fn jam_comms(&self, ship_name: &String, target: &str, boost: i16, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    // Rolled jammer-first, then target, to consume the seeded stream in the
    // order this always did.
    let roll = roll_dice(2, rng);
    let dm = self.sensor_quality_modifiers(ship_name)
      + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
      + boost;
    let other_roll = roll_dice(2, rng);
    let other_dm = self.sensor_quality_modifiers(target)
      + countermeasures_mod(self.ships.get(target).unwrap().read().unwrap().design.countermeasures);
    let check = i16::from(roll) + dm - i16::from(other_roll) - other_dm;

    if check >= 0 {
      // Jamming stops communication, and a sensor hand-off is communication:
      // for the rest of the round this ship can neither share its contacts with
      // its team nor receive theirs.
      if let Some(target_ship) = self.ships.get(target) {
        target_ship.write().unwrap().comms_jammed = true;
      }
      vec![opposed_check_effect(
        ship_name,
        "jams comms on",
        target,
        (roll, dm),
        (other_roll, other_dm),
        "comms jammed",
      )]
    } else {
      vec![opposed_check_effect(
        ship_name,
        "jams comms on",
        target,
        (roll, dm),
        (other_roll, other_dm),
        "jamming failed",
      )]
    }
  }
  fn jam_missiles(&mut self, ship_name: &String, boost: i16, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    let mut effects = Vec::<EffectMsg>::new();
    // Find all missiles targeting this ship.
    let targeting_missiles = self
      .missiles
      .iter()
      .filter(|(_, missile)| missile.read().unwrap().target == *ship_name)
      .map(|(missile_name, _missile)| missile_name.clone())
      .collect::<Vec<_>>();

    let dice = roll_dice(2, rng);
    let dm = self.sensor_quality_modifiers(ship_name)
      + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
      + boost;
    let check = i16::from(dice) + dm - 10;

    debug!(
      "(Entity.jam_missiles) Missile jamming attempt by {ship_name} rolled {dice}, sensor_quality mod {}, countermeasures mod {} gives an effect of {check}.",
      self.sensor_quality_modifiers(ship_name),
      countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures),
    );

    if check >= 0 {
      effects.append(&mut vec![sensor_check_effect(
        ship_name,
        "jams inbound missiles",
        None,
        dice,
        dm,
        10,
        &format!("effect {check}"),
      )]);
      // Deal with effect needing to allow one missile impact when the roll is made exactly.
      // Cast is safe because from above check >= 0.
      #[allow(clippy::cast_sign_loss)]
      let num_missiles = (check as usize).max(1);
      // Randomly pick the missiles that are destroyed.
      let destroyed = targeting_missiles.choose_multiple(rng, num_missiles).collect::<Vec<_>>();
      // Create for each destroyed missile an effect (exhaustion) and message.
      effects.append(
        &mut destroyed
          .iter()
          .flat_map(|missile_name| {
            let dead_missile = self.missiles.remove(missile_name.as_str()).unwrap();
            let missile = dead_missile.read().unwrap();
            [
              EffectMsg::ExhaustedMissile {
                position: missile.get_position(),
              },
              EffectMsg::tagged(
                MessageCategory::Destruction,
                format!("Missile {} destroyed by jamming.", missile.get_name()),
              ),
            ]
          })
          .collect::<Vec<_>>(),
      );
      // Remove the destroyed missiles from the list of all missiles.
    } else {
      // If the EW check failed, just let the users know.
      effects.push(sensor_check_effect(
        ship_name,
        "jams inbound missiles",
        None,
        dice,
        dm,
        10,
        "jamming failed",
      ));
    }
    effects
  }

  fn break_sensor_lock(
    &self, ship_name: &String, target: &str, reverse_sensor_locks: &HashMap<String, Vec<String>>, boost: i16,
    rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    // Check if the target of the BreakSensorLock has a sensor lock on this ship.
    // Get the list of every ship with a sensor lock on current ship; make sure the target of the BreakSensorLock is in that list.
    let valid_lock = reverse_sensor_locks
      .get(ship_name)
      .and_then(|ships_with_locks| ships_with_locks.iter().find(|&s| *s == target));
    if valid_lock.is_some() {
      // Make an opposed check - this ship vs the one with the lock..
      // Rolled breaker-first, then holder, to consume the seeded stream in the
      // order this always did.
      let roll = roll_dice(2, rng);
      let dm = self.sensor_quality_modifiers(ship_name)
        + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
        + boost;
      let other_dm = self.sensor_quality_modifiers(target)
        // The ship shaking off the lock benefits from ITS OWN stealth, so the
        // observer here is `target` (which holds the lock) and the quarry is
        // `ship_name`. Negating turns the detection penalty into a bonus for
        // the ship breaking free.
        + self.sensor_detection_modifiers(target, ship_name)
        + countermeasures_mod(self.ships.get(target).unwrap().read().unwrap().design.countermeasures);
      let other_roll = roll_dice(2, rng);
      let check = i16::from(roll) + dm - other_dm - i16::from(other_roll);
      if check >= 0 {
        self
          .ships
          .get(target)
          .unwrap()
          .write()
          .unwrap()
          .sensor_locks
          .retain(|s| s != ship_name);
        vec![opposed_check_effect(
          ship_name,
          "breaks the sensor lock held by",
          target,
          (roll, dm),
          (other_roll, other_dm),
          "lock broken",
        )]
      } else {
        vec![opposed_check_effect(
          ship_name,
          "breaks the sensor lock held by",
          target,
          (roll, dm),
          (other_roll, other_dm),
          "lock holds",
        )]
      }
    } else {
      Vec::default()
    }
  }

  /// Validate the entity data structure, performing some important post-load checks.
  /// These checks include:
  /// * A planet has a named primary iff it has a pointer to that planet.
  /// * Every missile has a point to its target and the names match.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a planet or missile.
  #[must_use]
  pub fn validate(&self) -> bool {
    for planet in self.planets.values() {
      let planet = planet.read().unwrap();

      // Clearer if we spell out each branch
      #[allow(clippy::match_same_arms)]
      match (&planet.primary, planet.primary_ptr.as_ref()) {
        (Some(_), None) => return false,
        (None, Some(_)) => return false,
        (Some(primary), Some(primary_ptr)) if primary_ptr.read().unwrap().get_name() != primary => {
          return false;
        }
        _ => {}
      }
    }

    for missile in self.missiles.values() {
      let missile = missile.read().unwrap();
      if missile.target_ptr.is_none()
        || missile.target_ptr.as_ref().unwrap().read().unwrap().get_name() != missile.target
      {
        return false;
      }
    }
    true
  }

  /// Fix secondary pointers in entities. For planets this is ensuring a link to the named primary for a planet.
  /// For missiles this is ensuring a link to the named target for a missile.
  ///
  /// # Errors
  /// Returns an error if a named planet entity is not found when building a primary pointer.
  /// Returns an error if a named ship entity is not found when building a target pointer.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a planet or missile.
  pub fn fixup_pointers(&mut self) -> Result<(), String> {
    for planet in self.planets.values() {
      let mut planet = planet.write().unwrap();
      let name = planet.get_name().to_string();
      if let Some(primary) = &mut planet.primary {
        let looked_up = self
          .planets
          .get(primary)
          .ok_or_else(|| format!("Unable to find entity named {primary} as primary for {name}"))?;
        planet.primary_ptr.replace(looked_up.clone());
      }
    }

    for missile in self.missiles.values() {
      let mut missile = missile.write().unwrap();
      let name = missile.get_name();
      let looked_up = self
        .ships
        .get(&missile.target)
        .ok_or_else(|| format!("Unable to find entity named {} as target for {name}", missile.target))?;
      missile.target_ptr.replace(looked_up.clone());
    }

    Ok(())
  }

  /// Give every ship a contact on every other ship that is not stealthed and is
  /// within Distant.
  ///
  /// **Not** the default opening state. Scenarios start with whatever contacts
  /// their file specifies, which is normally none: ships have to find each
  /// other, and the first detection pass runs at the end of the opening round.
  /// This exists so a scenario can be *authored* as already-engaged rather than
  /// as an approach, and for tests that are about something other than
  /// acquisition.
  ///
  /// Stealthed hulls are excluded even here, and nothing is seeded past
  /// Distant, where everything is an undifferentiated blip regardless.
  ///
  /// Takes `&self` because the ships are behind `RwLock`s; the map itself is
  /// only read.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  pub fn establish_initial_contacts(&self) {
    // Which ships are loud enough to be taken as already seen. A stealthed hull
    // is not: it has to be found by a detection pass like anything else.
    let mut visible: Vec<(String, Vec3)> = self
      .ships
      .iter()
      .filter(|(_, ship)| ship.read().unwrap().design.stealth.is_none())
      .map(|(name, ship)| (name.clone(), ship.read().unwrap().get_position()))
      .collect();
    // Sorted so the wire payload and the test fixtures do not depend on the
    // map's iteration order.
    visible.sort_by(|a, b| a.0.cmp(&b.0));

    for (name, ship) in &self.ships {
      let mut ship = ship.write().unwrap();
      let here = ship.get_position();
      ship.contacts = visible
        .iter()
        .filter(|(other, _)| other != name)
        .filter(|(_, there)| {
          // Nothing is in contact across more than Distant, so a scenario that
          // opens with ships that far apart opens with them unaware of each
          // other. They acquire normally once they close.
          #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
          let distance = (*there - here).magnitude() as u32;
          find_range_band(distance) != Range::Distant
        })
        .map(|(other, _)| other.clone())
        .collect();
    }
  }

  /// Whether `observer` currently detects `target`.
  ///
  /// The gate on every action one ship takes against another: an undetected
  /// ship is not there as far as the observer is concerned, so it cannot be
  /// fired at, locked or jammed.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  #[must_use]
  pub fn has_contact(&self, observer: &str, target: &str) -> bool {
    let Some(observer) = self.ships.get(observer) else {
      return false;
    };
    let Some(target) = self.ships.get(target) else {
      return false;
    };
    observer.read().unwrap().detects(&target.read().unwrap())
  }

  /// Whether `ship_name` is running its active sensors.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  #[must_use]
  pub fn has_active_sensors(&self, ship_name: &str) -> bool {
    self
      .ships
      .get(ship_name)
      .is_some_and(|ship| ship.read().unwrap().active_sensors)
  }

  /// Resolve detection for every pair of ships.
  ///
  /// Runs once at the end of each round, after movement, because the trigger
  /// for losing a stealthed ship is the range opening — which needs both the
  /// start-of-round positions (from `ship_snapshot`) and the end-of-round ones.
  /// Placing it here rather than at the top of the round also means a player
  /// sees a new contact before queueing the orders that would use it.
  ///
  /// Each ordered pair gets **at most one roll**: a stealthed ship that shakes
  /// off its pursuer does not then get reacquired by an acquisition roll in the
  /// same round.
  ///
  /// `fired` names the ships that took a fire action this round, which is worth
  /// DM+2 to anyone hunting a stealthed one.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write a ship.
  pub fn detection_pass(
    &mut self, ship_snapshot: &HashMap<String, Ship>, fired: &HashSet<String>, boost_map: &BoostMap,
    rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    let mut effects = Vec::new();
    // Sorted so a seeded run is reproducible; `ships` is a HashMap.
    let mut names: Vec<&String> = self.ships.keys().collect();
    names.sort();

    // Decisions are collected first and applied after, so no ship is being
    // written while another pair is still reading it.
    let mut acquired = Vec::<(String, String)>::new();
    // The bool is whether a roll was reported for this pair. A check that was
    // rolled already says how it came out, so repeating the outcome only makes
    // the results longer. Contact dropped for range is not rolled, so it has
    // nothing else to announce it.
    let mut lost = Vec::<(String, String, bool)>::new();
    // Every check that was actually rolled, reported so a referee can see why
    // a ship stayed hidden rather than having to infer it.
    let mut rolls = Vec::<EffectMsg>::new();
    // Range bands that moved, reported to whoever holds the contact.
    //
    // Only to them: you cannot judge the range to a ship you cannot see, and
    // reporting every pair would hand a player the position of ships they have
    // no contact on. Worth saying even when nothing is rolled -- the band sets
    // the to-hit modifier, and for a stealthed target an opening band is what
    // puts the contact at risk.
    let mut band_changes = Vec::<EffectMsg>::new();

    for observer_name in &names {
      for target_name in &names {
        if observer_name == target_name {
          continue;
        }

        let observer = self.ships.get(*observer_name).unwrap().read().unwrap();
        let target = self.ships.get(*target_name).unwrap().read().unwrap();

        // Team-mates always know where each other are, so there is nothing to
        // acquire and nothing that can be lost.
        if observer.team.is_some() && observer.team == target.team {
          continue;
        }

        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let band_now = find_range_band((target.get_position() - observer.get_position()).magnitude() as u32);

        let holds_contact = observer.contacts.iter().any(|name| name == *target_name);

        if holds_contact {
          if let Some(band_start) = Self::snapshot_band(ship_snapshot, observer_name, target_name) {
            if band_now != band_start {
              band_changes.push(EffectMsg::about(
                observer_name,
                MessageCategory::Detection,
                format!("{observer_name}: {target_name} now at {band_now} range (was {band_start})."),
              ));
            }
          }

          // Beyond Distant everything is an undifferentiated blip (p. 76), so
          // contact cannot be held at all.
          if band_now == Range::Distant {
            lost.push(((*observer_name).clone(), (*target_name).clone(), false));
            continue;
          }

          // Only a stealthed ship can be lost, and only by opening the range:
          // "sensor contact ... may be lost if the range between ships extends
          // by one or more bands during an encounter" (p. 77).
          if target.design.stealth.is_none() {
            continue;
          }
          let Some(band_start) = Self::snapshot_band(ship_snapshot, observer_name, target_name) else {
            continue;
          };
          if band_now <= band_start {
            continue;
          }

          let terms = self.detection_dm_terms(observer_name, target_name, &target, fired);
          let dm: i16 = terms.iter().map(|(_, value)| value).sum();
          let roll = roll_dice(2, rng);
          let total = i32::from(roll) + i32::from(dm);
          if total < STANDARD_ROLL_THRESHOLD {
            lost.push(((*observer_name).clone(), (*target_name).clone(), true));
          }
          rolls.push(detection_roll_effect(
            observer_name,
            target_name,
            roll,
            dm,
            total,
            if total < STANDARD_ROLL_THRESHOLD {
              "contact lost"
            } else {
              "contact held"
            },
            &terms,
          ));
        } else {
          // Acquisition needs active sensors: pinpointing a ship "requires the
          // use of active sensors" (p. 77).
          if !observer.active_sensors || band_now == Range::Distant {
            continue;
          }

          // A captain can put the sensop's attention on one particular check.
          // Detection happens either way — searching is free, not an action —
          // so this is purely the leadership boost.
          let boost = boost_for_detection(boost_map, observer_name, target_name);

          let terms = self.detection_dm_terms(observer_name, target_name, &target, fired);
          let dm: i16 = terms.iter().map(|(_, value)| value).sum::<i16>() + boost;
          let roll = roll_dice(2, rng);
          let total = i32::from(roll) + i32::from(dm);
          if total >= STANDARD_ROLL_THRESHOLD {
            acquired.push(((*observer_name).clone(), (*target_name).clone()));
          }
          rolls.push(detection_roll_effect(
            observer_name,
            target_name,
            roll,
            dm,
            total,
            if total >= STANDARD_ROLL_THRESHOLD {
              "contact"
            } else {
              "no contact"
            },
            &terms,
          ));
        }
      }
    }

    effects.append(&mut band_changes);
    effects.append(&mut rolls);
    effects.append(&mut self.apply_detection_changes(&lost, &acquired));
    effects
  }

  /// Add and remove the contacts a detection pass decided on.
  ///
  /// Applied after every pair has been resolved rather than as they are
  /// decided, so no ship is written while another pair is still reading it.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  fn apply_detection_changes(&self, lost: &[(String, String, bool)], acquired: &[(String, String)]) -> Vec<EffectMsg> {
    let mut effects = Vec::new();

    for (observer_name, target_name, rolled) in lost {
      let mut observer = self.ships.get(observer_name).unwrap().write().unwrap();
      observer.contacts.retain(|name| name != target_name);
      // A lock cannot outlive the contact it was built on.
      observer.sensor_locks.retain(|name| name != target_name);
      // Only announce a loss nothing else has reported. A failed reacquisition
      // already printed its roll, ending in "contact lost".
      if !rolled {
        effects.push(EffectMsg::about(
          observer_name,
          MessageCategory::Detection,
          format!("{observer_name} has lost sensor contact with {target_name}: out of range."),
        ));
      }
    }

    // Acquisitions are never announced separately: every one of them came from
    // a check, and that check's line already ends in "contact".
    for (observer_name, target_name) in acquired {
      let mut observer = self.ships.get(observer_name).unwrap().write().unwrap();
      observer.contacts.push(target_name.clone());
      observer.contacts.sort();
    }

    effects
  }

  /// The DM on `observer`'s sensor check against `target`.
  ///
  /// The same sum whether this is a first acquisition or a reacquisition after
  /// the range opened: how hard a ship is to see does not depend on whether the
  /// looker has seen it before. See `rules_tables::emissions_mod` for why High
  /// Guard's two tables are treated as one.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  /// The net DM. Production reads [`Self::detection_dm_terms`] so it can report
  /// the breakdown; this stays for tests that only assert the total.
  #[cfg(test)]
  fn detection_dm(&self, observer_name: &str, target_name: &str, target: &Ship, fired: &HashSet<String>) -> i16 {
    self
      .detection_dm_terms(observer_name, target_name, target, fired)
      .iter()
      .map(|(_, value)| value)
      .sum()
  }

  /// The detection DM broken into named terms, in the order they are reasoned
  /// about: what the observer brings, then what the target gives away.
  ///
  /// A detection DM is eight or so numbers summed into one, and as one number
  /// it cannot be checked. A net DM+9 against an Advanced-stealth hull three
  /// TLs above the observer looks wrong until the itemised form shows the
  /// target was under 5G thrust, running active sensors, shooting, and hot
  /// from its criticals -- at which point it is obviously right.
  fn detection_dm_terms(
    &self, observer_name: &str, target_name: &str, target: &Ship, fired: &HashSet<String>,
  ) -> Vec<(&'static str, i16)> {
    let observer = self.ships.get(observer_name).unwrap().read().unwrap();
    let mut terms = vec![
      ("sensor grade", SENSOR_QUALITY_MOD[observer.current_sensors as usize]),
      ("sensor skill", i16::from(observer.get_crew().get_sensors())),
    ];
    terms.extend(detection_modifier_terms(
      observer.design.tl,
      target.design.tl,
      target.design.stealth,
    ));
    drop(observer);

    terms.extend(
      Emissions {
        active_sensors: target.active_sensors,
        thrust_g: target.thrust_in_g(),
        power_plant: target.current_power > 0,
        fired_weapons: fired.contains(target_name),
        crit_severity: target.total_crit_severity(),
        transmitting: target.is_transmitting(),
      }
      .detection_terms(),
    );
    terms
  }

  /// A message saying `ship_name` cannot `what` because `station` is out, or
  /// `None` when the station is working (or the ship is gone).
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  fn station_down_effect(&self, ship_name: &str, station: BridgeStation, what: &str) -> Option<EffectMsg> {
    let working = self.ships.get(ship_name)?.read().unwrap().station_working(station);
    (!working).then(|| {
      EffectMsg::about(
        ship_name,
        MessageCategory::Critical,
        format!("{ship_name} cannot {what}: its {station} station is out."),
      )
    })
  }

  /// Count disabled bridge stations down at the end of the round.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  pub fn tick_bridge_stations(&self) {
    for ship in self.ships.values() {
      ship.write().unwrap().tick_bridge_stations();
    }
  }

  /// Clear the per-round comms jamming flags.
  ///
  /// Called at the end of the round, after hand-offs have been resolved, so a
  /// jam lasts exactly the round it was made in.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  pub fn clear_comms_jamming(&self) {
    for ship in self.ships.values() {
      ship.write().unwrap().comms_jammed = false;
    }
  }

  /// Share contacts along the team's hand-off links.
  ///
  /// High Guard p. 77: ships in a squadron can pass their sensor picture to
  /// each other over comms, so a sensop that succeeds covers those that failed.
  /// It needs no check and no action — only a point of computer Bandwidth at
  /// each end — so this runs automatically after the detection pass for any
  /// ship whose crew has switched hand-off on.
  ///
  /// Three limits from the text:
  ///
  /// * the link costs one Bandwidth from **both** host and recipient, so a
  ///   ship with none available can neither send nor receive;
  /// * it breaks beyond Distant, which is why the range is checked per pair;
  /// * "ships that receive hand-off sensory data cannot then hand-off that data
  ///   to additional ships", so this is a single hop — sharing reads from the
  ///   picture each host acquired itself, not from what it was given.
  ///
  /// A hand-off can convey a contact the recipient could never have acquired
  /// alone, which is the whole point: a squadron can post one picket with
  /// excellent sensors running fully active while everyone else stays quiet and
  /// still shoots at what the picket sees.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write a ship.
  pub fn sensor_handoff_pass(&mut self) -> Vec<EffectMsg> {
    let mut effects = Vec::new();

    // Snapshot what each host acquired on its own, before anything is shared,
    // so a hand-off cannot be relayed onward within the same pass.
    let mut hosts: Vec<(String, Vec3, Vec<String>)> = Vec::new();
    for (name, ship) in &self.ships {
      let ship = ship.read().unwrap();
      // A jammed ship cannot send: jamming stops communication, and a
      // hand-off is communication. Jamming already announces itself, so it
      // needs no message here.
      if !ship.handoff_sensors || ship.team.is_none() || ship.comms_jammed {
        continue;
      }
      // Sending takes the comms station, and the computer to package the
      // picture. Said only when there was a picture to send.
      if let Some(station) = ship.handoff_station_down() {
        if !ship.contacts.is_empty() {
          effects.push(EffectMsg::about(
            name,
            MessageCategory::Handoff,
            format!("{name} cannot hand off its sensor picture: its {station} station is out."),
          ));
        }
        continue;
      }
      if ship.current_computer == 0 {
        // Say so rather than failing quietly. A ship with no Bandwidth left --
        // a bad enough bridge crit will take it -- looks exactly like one that
        // is sharing fine, since nothing on screen shows the rating. Only worth
        // saying when there is a picture to share.
        if !ship.contacts.is_empty() {
          effects.push(EffectMsg::about(
            name,
            MessageCategory::Handoff,
            format!("{name} cannot hand off its sensor picture: no computer Bandwidth available."),
          ));
        }
        continue;
      }
      hosts.push((name.clone(), ship.get_position(), ship.contacts.clone()));
    }
    if hosts.is_empty() {
      return effects;
    }

    let mut shared = Vec::<(String, String, String)>::new();
    for (recipient_name, recipient) in &self.ships {
      let recipient_guard = recipient.read().unwrap();
      let Some(team) = recipient_guard.team else { continue };
      if recipient_guard.comms_jammed || recipient_guard.handoff_station_down().is_some() {
        continue;
      }
      // Same reasoning as the host side: a recipient with no Bandwidth is told
      // so, but only once something was actually held out to it.
      let no_bandwidth = recipient_guard.current_computer == 0;
      let mut missed_a_handoff = false;
      let here = recipient_guard.get_position();

      for (host_name, host_pos, host_contacts) in &hosts {
        if host_name == recipient_name {
          continue;
        }
        // Same side, and close enough for the link to hold.
        if self.ships.get(host_name).unwrap().read().unwrap().team != Some(team) {
          continue;
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let distance = (*host_pos - here).magnitude() as u32;
        if find_range_band(distance) == Range::Distant {
          continue;
        }

        for contact in host_contacts {
          if contact == recipient_name || recipient_guard.contacts.contains(contact) {
            continue;
          }
          // A hand-off cannot give the recipient a contact it could not hold.
          // Beyond Distant everything is an undifferentiated blip (p. 76), and
          // the detection pass drops any contact held that far out -- so a
          // hand-off that ignored the recipient's own range to the contact
          // re-created it every round, one pass dropping what the other had
          // just shared, and the contact never went away. The link's own
          // range is checked above; this is the contact's.
          let Some(contact_ship) = self.ships.get(contact) else {
            continue;
          };
          #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
          let to_contact = (contact_ship.read().unwrap().get_position() - here).magnitude() as u32;
          if find_range_band(to_contact) == Range::Distant {
            continue;
          }
          if no_bandwidth {
            missed_a_handoff = true;
          } else {
            shared.push((recipient_name.clone(), contact.clone(), host_name.clone()));
          }
        }
      }

      if missed_a_handoff {
        effects.push(EffectMsg::about(
          recipient_name,
          MessageCategory::Handoff,
          format!("{recipient_name} cannot receive a sensor hand-off: no computer Bandwidth available."),
        ));
      }
    }

    for (recipient_name, contact, host_name) in shared {
      let mut recipient = self.ships.get(&recipient_name).unwrap().write().unwrap();
      if recipient.contacts.contains(&contact) {
        continue;
      }
      recipient.contacts.push(contact.clone());
      recipient.contacts.sort();
      effects.push(EffectMsg::about(
        &recipient_name,
        MessageCategory::Handoff,
        format!("{recipient_name} receives contact on {contact} from {host_name}."),
      ));
    }

    effects
  }

  /// The range band between two ships as it was at the start of the round.
  ///
  /// `None` when either ship was not in the snapshot, which means it joined
  /// mid-round and so cannot have opened the range during it.
  fn snapshot_band(snapshot: &HashMap<String, Ship>, observer: &str, target: &str) -> Option<Range> {
    let observer = snapshot.get(observer)?;
    let target = snapshot.get(target)?;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let distance = (target.get_position() - observer.get_position()).magnitude() as u32;
    Some(find_range_band(distance))
  }

  /// Drop missiles whose target no longer exists, reporting each as exhausted.
  ///
  /// A missile holds a resolved pointer to its target, rebuilt by
  /// `fixup_pointers` on every deep copy. If the target has left play the
  /// rebuild fails, and because the live state is deep-copied to answer any
  /// request for entities, one orphaned missile makes the whole scenario
  /// unreadable: the client stops receiving updates and sits on stale state,
  /// still offering the dead ship as a target.
  ///
  /// The referee's Remove already dropped them. Ships destroyed in combat and
  /// ships that jumped out did not, which is the more common way for a target
  /// to disappear while something is still flying at it.
  ///
  /// Reported as exhausted rather than deleted silently, so the salvo visibly
  /// goes away instead of vanishing between frames.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a missile.
  pub fn prune_orphaned_missiles(&mut self) -> Vec<EffectMsg> {
    let orphaned: Vec<(String, Vec3)> = self
      .missiles
      .iter()
      .filter_map(|(name, missile)| {
        let missile = missile.read().unwrap();
        (!self.ships.contains_key(&missile.target)).then(|| (name.clone(), missile.get_position()))
      })
      .collect();

    orphaned
      .into_iter()
      .map(|(name, position)| {
        debug!("(Entity.prune_orphaned_missiles) Missile {name} lost its target.");
        self.missiles.remove(&name);
        EffectMsg::ExhaustedMissile { position }
      })
      .collect()
  }

  /// Drop contacts and sensor locks naming ships that no longer exist.
  ///
  /// Called after ships leave play - destroyed, jumped out or removed by the
  /// referee - so nothing keeps tracking a name that is gone.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  pub fn prune_ship_references(&self) {
    for ship in self.ships.values() {
      let mut ship = ship.write().unwrap();
      ship.contacts.retain(|other| self.ships.contains_key(other));
      ship.sensor_locks.retain(|other| self.ships.contains_key(other));
    }
  }

  /// Rewrite everything that names `from` to name `to`: contacts, sensor locks
  /// and missile targets.
  ///
  /// Renaming used to leave `sensor_locks` pointing at the old name, silently
  /// dropping the lock, and missiles pointing at a target that no longer
  /// existed — which is worse than silent, because an unresolvable missile
  /// target makes the whole scenario fail to serialize.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a ship.
  fn rename_ship_references(&self, from: &str, to: &str) {
    // A missile holds its target by name and resolves it on every deep copy,
    // so a rename that does not follow leaves the missile pointing at a ship
    // that no longer exists under that name.
    for missile in self.missiles.values() {
      let mut missile = missile.write().unwrap();
      if missile.target == from {
        missile.target = to.to_string();
      }
    }

    for ship in self.ships.values() {
      let mut guard = ship.write().unwrap();
      // Reborrow through the guard once so the two field borrows below are
      // seen as disjoint rather than as two borrows of the guard itself.
      let ship = &mut *guard;
      for name in ship.contacts.iter_mut().chain(ship.sensor_locks.iter_mut()) {
        if name == from {
          to.clone_into(name);
        }
      }
    }
  }

  /// Reset the gravity wells for all planets.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write to a planet.
  pub fn reset_gravity_wells(&mut self) {
    for planet in self.planets.values() {
      let mut planet = planet.write().unwrap();
      planet.reset_gravity_wells();
    }
  }

  /// Its easier to assume actions will stay the same round to round.
  /// Given that scrub the actions so that actions are removed if:
  /// 1) They are a fire action and the target is no longer present.
  /// 2) They are a sensor action and the target is no longer present.
  /// 3) They are a jam missiles action and there are no missiles incoming.
  /// 4) They are a sensor lock action and a sensor lock has been achieved.
  /// 5) They are a break sensor lock action and the sensor lock has been broken.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn reset_actions(&mut self) {
    self.actions.iter_mut().for_each(|(ship_name, actions)| {
      // Find the actual ship. If it no longer exists we don't need any of these actions.
      if !self.ships.contains_key(ship_name) {
        actions.clear();
        return;
      }

      // Whether THIS ship already holds a sensor lock on `target`. SensorLock
      // is an *attempt*; once successful the lock persists in the attacker's
      // `sensor_locks` and the queued action should drop. Re-queue manually
      // if the lock is later broken.
      let attacker_has_lock_on = |target: &str| -> bool {
        self
          .ships
          .get(ship_name)
          .is_some_and(|s| s.read().unwrap().sensor_locks.iter().any(|n| n == target))
      };

      actions.retain(|action| {
        match action {
          // Keep FireActions, JamComms if the target still exists.
          ShipAction::JamComms { target } | ShipAction::FireAction { target, .. } => self.ships.contains_key(target),
          // Keep SensorLock only while the lock isn't yet established.
          ShipAction::SensorLock { target } => self.ships.contains_key(target) && !attacker_has_lock_on(target),
          // Keep BreakSensorLock if the target still exists and the target has a sensor lock.
          ShipAction::BreakSensorLock { target } => {
            if let Some(target_ship) = self.ships.get(target) {
              let target_ship = target_ship.read().unwrap();
              return target_ship.sensor_locks.contains(ship_name);
            }
            false
          }
          // Keep JamMissiles and PointDefense in all cases.
          ShipAction::PointDefenseAction { .. } | ShipAction::JamMissiles => true,
          // Engineer actions should be scrubbed each turn - they are one-time actions.
          // LeadershipCheck is also one-shot (the captain re-queues it each
          // turn through the Captain HUD).
          // Anti-actions are consumed by `merge` and should never reach here, but
          // strip them defensively if they do.
          ShipAction::DeleteFireAction { .. }
          | ShipAction::Jump
          | ShipAction::OverloadDrive
          | ShipAction::OverloadPlant
          | ShipAction::Repair { .. }
          | ShipAction::LeadershipCheck { .. }
          | ShipAction::ClearSensorAction
          | ShipAction::ClearEngineerAction
          | ShipAction::ClearLeadershipCheck => false,
        }
      });
    });
    self.actions.retain(|(_ship_name, actions)| {
      // If there are no actions for a ship, remove the ship from the actions list.
      !actions.is_empty()
    });
  }

  /// Evaluate all queued engineer actions at end-of-turn.
  ///
  /// Engineer actions are deferred from when the player queues them through
  /// `ModifyActions` to the end of the turn. This method walks the queued
  /// list, dispatches each action's specific helper
  /// (`process_overload_drive`, `process_overload_plant`, or `process_repair`),
  /// flags `engineer_action_taken` on the ship, and wraps each result in an
  /// `EffectMsg::EngineerAction` so it rides the existing `Effects` channel
  /// out to the FE.
  ///
  /// Defensive behavior:
  /// * If a ship is missing from `self.ships`, that entry is logged and skipped.
  /// * If `engineer_action_taken` is already true (shouldn't happen because
  ///   the per-turn reset clears it before this runs), the action is logged
  ///   and skipped to avoid double evaluation.
  /// * Non-engineer actions in the per-ship list are silently ignored — the
  ///   caller is expected to filter, but we don't trust it.
  ///
  /// # Arguments
  /// * `actions` - The engineer actions queued for this turn, grouped by ship.
  /// * `rng` - The random number generator used for skill checks.
  ///
  /// # Returns
  /// One `EffectMsg::EngineerAction` per evaluated action, in input order.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write the ship.
  pub fn engineer_actions(
    &mut self, actions: &[(String, Vec<ShipAction>)], boost_map: &BoostMap, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    let mut effects = Vec::new();
    // Ships that successfully jumped this turn — removed from the world after
    // the loop, since iteration borrows `self.ships`.
    let mut jumped_ships = Vec::<String>::new();

    for (ship_name, ship_actions) in actions {
      if !self.ships.contains_key(ship_name) {
        warn!("(engineer_actions) Cannot find ship {ship_name} for engineer action.");
        continue;
      }

      let boost = boost_for_engineer(boost_map, ship_name);

      for action in ship_actions {
        // Defensive: skip if already taken (reset_temporary_bonuses should
        // have cleared this before we ran).
        {
          let mut ship = self.ships.get(ship_name).unwrap().write().unwrap();
          if ship.has_engineer_action_taken() {
            warn!(
              "(engineer_actions) {ship_name} already has an engineer action taken this turn; skipping {action:?}."
            );
            continue;
          }
          ship.set_engineer_action_taken(true);
        }

        // Crits a failed overload does to its own ship, reported after the
        // check that caused them.
        let mut crits = Vec::new();
        let result = match action {
          ShipAction::OverloadDrive => self.process_overload_drive(ship_name, boost, &mut crits, rng),
          ShipAction::OverloadPlant => self.process_overload_plant(ship_name, boost, &mut crits, rng),
          ShipAction::Repair { system } => self.process_repair(ship_name, *system, boost, rng),
          ShipAction::Jump => {
            let (result, jumped) = self.process_jump(ship_name, boost, rng);
            if jumped {
              jumped_ships.push(ship_name.clone());
            }
            result
          }
          // Caller is expected to filter, but be defensive.
          _ => continue,
        };
        effects.push(EffectMsg::EngineerAction { result });
        effects.append(&mut crits);
      }
    }

    let jumped_any = !jumped_ships.is_empty();
    for ship_name in jumped_ships {
      self.ships.remove(&ship_name);
    }
    if jumped_any {
      self.prune_ship_references();
      effects.append(&mut self.prune_orphaned_missiles());
    }

    effects
  }

  /// Process a jump engineer action. Returns the result plus a flag indicating
  /// whether the ship actually jumped (so the caller can remove it from
  /// `self.ships` once iteration finishes).
  ///
  /// Mechanics: requires `can_jump == true` and enough fuel. Engineering check
  /// is `2d6 + engineering_jump + boost − 6`. Pass = clean jump. Fail = misjump
  /// (the ship still leaves the system but flagged as `critical_failure`).
  /// If preconditions aren't met the ship doesn't jump at all (`success: false`,
  /// no critical failure).
  fn process_jump(&mut self, ship_name: &str, boost: i16, rng: &mut dyn RngCore) -> (EngineerActionResult, bool) {
    let ship = self.ships.get(ship_name).unwrap().read().unwrap();
    let action = ShipAction::Jump;

    let stations_down: Vec<String> = [BridgeStation::Astrogation, BridgeStation::Computer]
      .into_iter()
      .filter(|station| !ship.station_working(*station))
      .map(|station| station.to_string())
      .collect();
    if !stations_down.is_empty() {
      return (
        EngineerActionResult {
          ship_name: ship_name.to_string(),
          action,
          success: false,
          check: 0,
          target: 0,
          message: format!("{ship_name} cannot jump: its {} station is out.", stations_down.join(" and ")),
          critical_failure: false,
        },
        false,
      );
    }

    if !ship.can_jump() || ship.current_fuel <= ship.design.hull / 10 {
      return (
        EngineerActionResult {
          ship_name: ship_name.to_string(),
          action,
          success: false,
          check: 0,
          target: 0,
          message: format!("{ship_name} cannot jump: insufficient fuel or not clear of gravity wells."),
          critical_failure: false,
        },
        false,
      );
    }

    let skill = ship.get_crew().get_engineering_jump();
    drop(ship);

    let target: u8 = 6;
    let (total, check) = engineer_check(
      roll_dice(2, rng),
      &[("engineering (j-drive)", i16::from(skill)), ("captain", boost.max(0))],
      target,
    );

    if total >= target {
      (
        EngineerActionResult {
          ship_name: ship_name.to_string(),
          action,
          success: true,
          check: total,
          target,
          message: format!("{ship_name} jump check {check}: jumps successfully."),
          critical_failure: false,
        },
        true,
      )
    } else {
      (
        EngineerActionResult {
          ship_name: ship_name.to_string(),
          action,
          success: false,
          check: total,
          target,
          message: format!("{ship_name} jump check {check}: misjumps! Ship is lost in jump space."),
          critical_failure: true,
        },
        true,
      )
    }
  }

  /// Process an overload drive engineer action.
  ///
  /// # Arguments
  /// * `ship_name` - The name of the ship performing the action.
  /// * `crits` - Receives the critical hit a critical failure does to the drive.
  /// * `rng` - The random number generator to use.
  ///
  /// # Returns
  /// The result of the overload drive action.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write the ship.
  fn process_overload_drive(
    &mut self, ship_name: &str, boost: i16, crits: &mut Vec<EffectMsg>, rng: &mut dyn RngCore,
  ) -> EngineerActionResult {
    let ship = self.ships.get(ship_name).unwrap();
    let skill = ship.read().unwrap().get_crew().get_engineering_maneuver();
    let target: u8 = 10;
    let (total, check) = engineer_check(
      roll_dice(2, rng),
      &[("engineering (m-drive)", i16::from(skill)), ("captain", boost.max(0))],
      target,
    );

    let action = ShipAction::OverloadDrive;

    if total >= target {
      // Success - set temporary_maneuver = 1
      ship.write().unwrap().set_temporary_maneuver(1);
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: true,
        check: total,
        target,
        message: format!("{ship_name} maneuver drive overload {check}: success, temporary +1 maneuver."),
        critical_failure: false,
      }
    } else if total <= 4 {
      // Critical failure (fail by 6+) - apply crit to maneuver drive
      crits.append(&mut apply_crit(1, ShipSystem::Maneuver, &mut ship.write().unwrap(), rng));
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: total,
        target,
        message: format!("{ship_name} maneuver drive overload {check}: critical failure, drive damaged."),
        critical_failure: true,
      }
    } else {
      // Normal failure
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: total,
        target,
        message: format!("{ship_name} maneuver drive overload {check}: failed."),
        critical_failure: false,
      }
    }
  }

  /// Process an overload plant engineer action.
  ///
  /// # Arguments
  /// * `ship_name` - The name of the ship performing the action.
  /// * `crits` - Receives the critical hit a critical failure does to the plant.
  /// * `rng` - The random number generator to use.
  ///
  /// # Returns
  /// The result of the overload plant action.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write the ship.
  fn process_overload_plant(
    &mut self, ship_name: &str, boost: i16, crits: &mut Vec<EffectMsg>, rng: &mut dyn RngCore,
  ) -> EngineerActionResult {
    let ship = self.ships.get(ship_name).unwrap();
    let skill = ship.read().unwrap().get_crew().get_engineering_power();
    let target: u8 = 10;
    let (total, check) = engineer_check(
      roll_dice(2, rng),
      &[("engineering (power)", i16::from(skill)), ("captain", boost.max(0))],
      target,
    );

    let action = ShipAction::OverloadPlant;

    if total >= target {
      // Success - set temporary_power_multiplier = 1.1
      ship.write().unwrap().set_temporary_power_multiplier(1.1);
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: true,
        check: total,
        target,
        message: format!("{ship_name} power plant overload {check}: success, temporary +10% power."),
        critical_failure: false,
      }
    } else if total <= 4 {
      // Critical failure (fail by 6+) - apply crit to powerplant
      crits.append(&mut apply_crit(1, ShipSystem::Powerplant, &mut ship.write().unwrap(), rng));
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: total,
        target,
        message: format!("{ship_name} power plant overload {check}: critical failure, plant damaged."),
        critical_failure: true,
      }
    } else {
      // Normal failure
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: total,
        target,
        message: format!("{ship_name} power plant overload {check}: failed."),
        critical_failure: false,
      }
    }
  }

  /// Process a repair engineer action.
  ///
  /// # Arguments
  /// * `ship_name` - The name of the ship performing the action.
  /// * `system` - The ship system to repair.
  /// * `rng` - The random number generator to use.
  ///
  /// # Returns
  /// The result of the repair action.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read or write the ship.
  fn process_repair(
    &mut self, ship_name: &str, system: ShipSystem, boost: i16, rng: &mut dyn RngCore,
  ) -> EngineerActionResult {
    let action = ShipAction::Repair { system };

    // Cannot repair Hull
    if system == ShipSystem::Hull {
      return EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: 0,
        target: 0,
        message: format!("{ship_name} cannot repair hull damage."),
        critical_failure: false,
      };
    }

    let ship = self.ships.get(ship_name).unwrap();
    let mut ship_write = ship.write().unwrap();

    // Engineering for the drives and the power plant; Mechanic for the rest of
    // the ship's equipment.
    let crew = ship_write.get_crew();
    let (skill, skill_name) = match system {
      ShipSystem::Jump => (crew.get_engineering_jump(), "engineering (j-drive)"),
      ShipSystem::Powerplant => (crew.get_engineering_power(), "engineering (power)"),
      ShipSystem::Weapon | ShipSystem::Sensors | ShipSystem::Bridge => (crew.get_mechanic(), "mechanic"),
      _ => (crew.get_engineering_maneuver(), "engineering (m-drive)"),
    };

    // Get current crit level for the system
    let crit_level = ship_write.crit_level[system as usize];

    // Get repair bonus if last_repair_component matches this system
    let repair_bonus = if ship_write.get_last_repair_component() == Some(system) {
      ship_write.get_repair_bonus()
    } else {
      ship_write.set_repair_bonus(0);
      0
    };

    let target: u8 = 8;
    let (total, check) = engineer_check(
      roll_dice(2, rng),
      &[
        (skill_name, i16::from(skill)),
        ("earlier attempts", i16::from(repair_bonus)),
        ("captain", boost.max(0)),
        ("damage", -i16::from(crit_level)),
      ],
      target,
    );

    if total >= target {
      // Success - reduce crit level by 1
      if ship_write.crit_level[system as usize] > 0 {
        ship_write.crit_level[system as usize] -= 1;
      }
      ship_write.set_repair_bonus(0);
      ship_write.set_last_repair_component(Some(system));
      // A bridge repair takes the most recent bridge damage back off, down to
      // the severity it now stands at.
      let restored = if system == ShipSystem::Bridge {
        let level = ship_write.crit_level[system as usize];
        ship_write.undo_bridge_damage(level)
      } else {
        vec![]
      };
      let station = if restored.is_empty() {
        String::new()
      } else {
        format!(" Restored: {}.", restored.join(", "))
      };
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: true,
        check: total,
        target,
        message: format!("{ship_name} repair {system:?} {check}: repaired.{station}"),
        critical_failure: false,
      }
    } else {
      // Failure - increment repair bonus
      let current_bonus = ship_write.get_repair_bonus();
      ship_write.set_repair_bonus(current_bonus.saturating_add(1));
      ship_write.set_last_repair_component(Some(system));
      EngineerActionResult {
        ship_name: ship_name.to_string(),
        action,
        success: false,
        check: total,
        target,
        message: format!("{ship_name} repair {system:?} {check}: failed."),
        critical_failure: false,
      }
    }
  }
}

use std::fmt::{Display, Error, Formatter};
impl std::fmt::Debug for Entities {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), Error> {
    (self as &dyn Display).fmt(f)
  }
}

impl std::fmt::Display for Entities {
  fn fmt(&self, f: &mut Formatter<'_>) -> std::result::Result<(), Error> {
    if self.ships.values().len() + self.missiles.values().len() + self.planets.values().len() == 0 {
      write!(f, "Entities {{}}")?;
      return Ok(());
    }

    writeln!(f, "Entities {{")?;
    for ship in self.ships.values() {
      writeln!(f, "  {:?},", ship.read().unwrap())?;
    }
    for missile in self.missiles.values() {
      writeln!(f, "  {:?},", missile.read().unwrap())?;
    }
    for planet in self.planets.values() {
      writeln!(f, "  {:?},", planet.read().unwrap())?;
    }
    write!(f, "}}")?;
    Ok(())
  }
}

// If we ever clone Entities (almost always for testing) we want it to be deep!
// Production wire-path code should call [`Entities::deep_copy`] directly so
// inconsistent state surfaces as a `Result::Err` instead of a panic. This
// `Clone` impl keeps tests + scenario-reset terse, but it WILL panic if the
// source's pointer references are dangling (planet primary / missile target).
impl Clone for Entities {
  fn clone(&self) -> Self {
    self
      .deep_copy()
      .expect("Entities::clone: dangling reference; call deep_copy() directly to handle the error")
  }
}

impl Serialize for Entities {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    #[derive(Serialize)]
    struct Entities<'a> {
      metadata: &'a MetaData,
      filename: &'a str,
      ships: Vec<Ship>,
      missiles: Vec<Missile>,
      planets: Vec<Planet>,
      actions: ShipActionList,
    }

    let mut entities = Entities {
      metadata: &self.metadata,
      filename: &self.filename,
      ships: self.ships.values().map(|s| s.read().unwrap().clone()).collect::<Vec<Ship>>(),
      missiles: self
        .missiles
        .values()
        .map(|m| m.read().unwrap().clone())
        .collect::<Vec<Missile>>(),
      planets: self
        .planets
        .values()
        .map(|p| p.read().unwrap().clone())
        .collect::<Vec<Planet>>(),
      actions: self.actions.clone(),
    };

    //The following sort_by is not necessary and adds inefficiency BUT ensures we serialize each item in the same order
    //each time. This makes writing tests a lot easier!
    entities.ships.sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
    entities
      .missiles
      .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
    entities.planets.sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
    entities.actions.sort_by_key(|a| a.0.clone());

    entities.serialize(serializer)
  }
}

/* Deserialize for Entities in the server is only ever used for writing unit tests. */
impl<'de> Deserialize<'de> for Entities {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    #[derive(Deserialize)]
    struct Entities {
      #[serde(default)]
      ships: Vec<Ship>,
      #[serde(default)]
      missiles: Vec<Missile>,
      #[serde(default)]
      planets: Vec<Planet>,
      #[serde(default)]
      actions: ShipActionList,
      #[serde(default)]
      metadata: MetaData,
    }

    let guts = Entities::deserialize(deserializer)?;
    Ok(crate::entity::Entities {
      ships: guts
        .ships
        .into_iter()
        .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
        .collect(),
      missiles: guts
        .missiles
        .into_iter()
        .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
        .collect(),
      planets: guts
        .planets
        .into_iter()
        .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
        .collect(),
      next_missile_id: 0,
      actions: guts.actions,
      metadata: guts.metadata,
      // Scenario files don't carry their own basename; load_from_file populates it.
      filename: String::new(),
    })
  }
}

#[cfg(test)]
mod tests {
  use crate::ship::{WeaponMount, WeaponType};

  /// The launcher every pre-torpedo test implicitly assumed: a single missile rack.
  fn test_missile_weapon() -> Weapon {
    Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1)
  }

  use super::*;
  use crate::crew::{Crew, Skills};
  use crate::debug;
  use crate::ship::{
    config_test_ship_templates, config_test_ship_templates_locked, get_ship_template, get_ship_templates_snapshot,
    lock_ship_templates_for_test, replace_ship_templates, ShipDesignTemplate, ShipTemplateTable,
  };
  use assert_json_diff::assert_json_eq;
  use cgmath::assert_relative_eq;
  use cgmath::{Vector2, Zero};
  use rand::rngs::{mock::StepRng, SmallRng};
  use rand::SeedableRng;
  use serde_json::json;
  use std::fs;
  use std::time::{SystemTime, UNIX_EPOCH};

  struct ShipTemplateRestoreGuard(ShipTemplateTable);

  impl Drop for ShipTemplateRestoreGuard {
    fn drop(&mut self) {
      replace_ship_templates(self.0.clone());
    }
  }

  #[test_log::test]
  fn test_entities_display_and_debug() -> Result<(), String> {
    let mut entities = Entities::new();

    // Add a ship
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Add another ship
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4.0, 5.0, 6.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Add a planet
    entities.add_planet(
      String::from("Planet1"),
      Vec3::new(4.0, 5.0, 6.0),
      String::from("blue"),
      None,
      6371e3,
      5.97e24,
      vec![],
    )?;

    // Launch a missile
    entities.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();

    // Test Display trait
    let display_output = format!("{entities}");
    assert!(display_output.contains("Ship1"));
    assert!(display_output.contains("Planet1"));
    assert!(display_output.contains("Ship2"));
    assert!(display_output.contains("Ship1::Ship2::0"));

    // Test Debug trait
    let debug_output = format!("{entities:?}");
    assert_eq!(display_output, debug_output, "Display and Debug outputs should be identical");

    // Test empty Entities
    let empty_entities = Entities::new();
    assert_eq!(
      format!("{empty_entities}"),
      "Entities {}",
      "Empty Entities should display as 'Entities {{}}'"
    );
    assert_eq!(
      format!("{empty_entities:?}"),
      "Entities {}",
      "Empty Entities should debug as 'Entities {{}}'"
    );

    Ok(())
  }

  #[test_log::test]
  fn test_add_ship() {
    let _ = pretty_env_logger::try_init();
    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4.0, 5.0, 6.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship3"),
      Vec3::new(7.0, 8.0, 9.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );

    assert_eq!(entities.ships.get("Ship1").unwrap().read().unwrap().get_name(), "Ship1");
    assert_eq!(entities.ships.get("Ship2").unwrap().read().unwrap().get_name(), "Ship2");
    assert_eq!(entities.ships.get("Ship3").unwrap().read().unwrap().get_name(), "Ship3");
  }

  #[test_log::test]
  fn test_rename_ship_and_planet() {
    let _ = pretty_env_logger::try_init();
    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4.0, 5.0, 6.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities
      .add_planet(
        String::from("Star"),
        Vec3::zero(),
        String::from("yellow"),
        None,
        1.0e9,
        2.0e30,
        Vec::new(),
      )
      .unwrap();
    entities
      .add_planet(
        String::from("Planet1"),
        Vec3::new(1.0e11, 0.0, 0.0),
        String::from("blue"),
        Some(String::from("Star")),
        6.4e6,
        6.0e24,
        Vec::new(),
      )
      .unwrap();

    // Capture the Arc identity so we can confirm rename mutates in place
    // rather than constructing a new entity (which would clone state).
    let ship1_arc_before = Arc::clone(entities.ships.get("Ship1").unwrap());

    // Happy path: rename Ship1 → Buccaneer.
    entities.rename("Ship1", "Buccaneer").unwrap();
    assert!(!entities.ships.contains_key("Ship1"), "Old key should be gone");
    let renamed = entities.ships.get("Buccaneer").expect("New key should exist");
    assert_eq!(renamed.read().unwrap().get_name(), "Buccaneer");
    assert!(
      Arc::ptr_eq(renamed, &ship1_arc_before),
      "Should preserve Arc identity (no copy)"
    );

    // Happy path planet rename also rewrites child planet's `primary`.
    entities.rename("Star", "Sol").unwrap();
    assert!(!entities.planets.contains_key("Star"));
    assert_eq!(entities.planets.get("Sol").unwrap().read().unwrap().get_name(), "Sol");
    assert_eq!(
      entities.planets.get("Planet1").unwrap().read().unwrap().primary.as_deref(),
      Some("Sol")
    );

    // No-op when current == new.
    assert!(entities.rename("Buccaneer", "Buccaneer").is_ok());

    // Whitespace-only new name.
    assert!(entities.rename("Buccaneer", "   ").is_err());

    // Collision with an existing ship.
    assert!(entities.rename("Buccaneer", "Ship2").is_err());

    // Collision with an existing planet.
    assert!(entities.rename("Ship2", "Sol").is_err());

    // Missing entity.
    assert!(entities.rename("DoesNotExist", "Anything").is_err());
  }

  #[test_log::test]
  fn test_update_all() {
    let _ = pretty_env_logger::try_init();
    let mut rng = SmallRng::seed_from_u64(0);

    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    // Create entities with random positions and names
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1000.0, 2000.0, 3000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4000.0, 5000.0, 6000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship3"),
      Vec3::new(7000.0, 8000.0, 9000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );

    // Assign random accelerations to entities
    let acceleration1 = Vec3::new(1.0, 1.0, 1.0) * G;
    let acceleration2 = Vec3::new(2.0, 1.0, -2.0) * G;
    let acceleration3 = Vec3::new(-1.0, -1.0, -0.0) * G;
    entities
      .set_flight_plan("Ship1", &FlightPlan((acceleration1, 50000).into(), None))
      .unwrap();
    entities
      .set_flight_plan("Ship2", &FlightPlan((acceleration2, 50000).into(), None))
      .unwrap();
    entities
      .set_flight_plan("Ship3", &FlightPlan((acceleration3, 50000).into(), None))
      .unwrap();

    // Update the entities a few times
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);

    // Validate the new positions for each entity
    let expected_position1 = Vec3::new(5_720_442.4, 5_721_442.4, 5_722_442.4);
    let expected_position2 = Vec3::new(11_442_884.8, 5_724_442.4, -11_432_884.8);
    let expected_position3 = Vec3::new(-5_712_442.4, -5_711_442.4, 9000.0);
    assert_relative_eq!(
      entities.ships.get("Ship1").unwrap().read().unwrap().get_position(),
      expected_position1,
      epsilon = 1e-7
    );
    assert_relative_eq!(
      entities.ships.get("Ship2").unwrap().read().unwrap().get_position(),
      expected_position2,
      epsilon = 1e-7
    );
    assert_relative_eq!(
      entities.ships.get("Ship3").unwrap().read().unwrap().get_position(),
      expected_position3,
      epsilon = 1e-7
    );
  }

  #[test_log::test]
  fn test_entities_validate() -> Result<(), String> {
    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    // Test 1: Empty entities should be valid
    assert!(entities.validate(), "Empty entities should be valid");

    // Test 2: Add a valid planet
    entities.add_planet(
      String::from("Sun"),
      Vec3::zero(),
      String::from("yellow"),
      None,
      6.96e8,
      1.989e30,
      vec![],
    )?;
    assert!(entities.validate(), "Entities with a single valid planet should be valid");

    // Test 3: Add a valid ship
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    assert!(entities.validate(), "Entities with a valid planet and ship should be valid");

    // Test 4: Add a second ship
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4.0, 5.0, 6.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    assert!(
      entities.validate(),
      "Entities with a valid planet and two ships should be valid"
    );

    // Test 5: Add a valid missile
    entities.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();
    assert!(
      entities.validate(),
      "Entities with a valid planet, two ships, and missile should be valid"
    );

    // Test 5: Add a planet with a missing primary_ptr
    let planet = Planet::new(
      String::from("InvalidPlanet2"),
      Vec3::new(7.0, 8.0, 9.0),
      String::from("red"),
      6371e3,
      5.97e24,
      Some(String::from("Sun")),
      &None,
      1,
    );

    entities
      .planets
      .insert(String::from("InvalidPlanet2"), Arc::new(RwLock::new(planet)));
    assert!(!entities.validate(), "Entities with an invalid primary_ptr should be invalid");

    // Test 6: Fix the invalid primary_ptr
    {
      let planets_table = &mut entities.planets;
      let sun = planets_table.get_mut("Sun").unwrap().clone();
      let mut planet = planets_table.get_mut("InvalidPlanet2").unwrap().write().unwrap();
      planet.primary_ptr = Some(sun);
    }
    assert!(entities.validate(), "Entities with fixed primary_ptr should be valid");

    // Test 7: Make the primary_ptr have a different name from the primary
    {
      let planets_table = &mut entities.planets;
      let invalid_planet = planets_table.get_mut("InvalidPlanet2").unwrap().clone();
      let mut planet = planets_table.get_mut("InvalidPlanet2").unwrap().write().unwrap();
      planet.primary_ptr = Some(invalid_planet);
      planet.primary = Some(String::from("Sun"));
    }
    assert!(
      !entities.validate(),
      "Entities with a primary_ptr having a different name should be invalid"
    );

    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(300.0, 200.0, 300.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );

    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(800.0, 500.0, 300.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();
    // Test 8: Create a missile with no target_ptr
    {
      entities
        .missiles
        .get_mut("Ship1::Ship2::0")
        .unwrap()
        .write()
        .unwrap()
        .target_ptr = None;
    }
    assert!(
      !entities.validate(),
      "Entities with a missile with no target_ptr should be invalid"
    );
    // Test 9: Fix the missile target_ptr
    {
      let missiles_table = &mut entities.missiles;
      let ship2 = entities.ships.get("Ship2").unwrap().clone();
      let mut missile = missiles_table.get_mut("Ship1::Ship2::0").unwrap().write().unwrap();
      missile.target_ptr = Some(ship2);
    }
    assert!(
      entities.validate(),
      "Entities with a missile with fixed target_ptr should be valid"
    );

    Ok(())
  }

  #[test_log::test]
  fn test_sun_update() -> Result<(), String> {
    let _ = pretty_env_logger::try_init();
    let mut rng = SmallRng::seed_from_u64(0);

    let mut entities = Entities::new();

    // Create some planets and see if they move.
    entities.add_planet(
      String::from("Sun"),
      Vec3::zero(),
      String::from("blue"),
      None,
      6.371e6,
      6e24,
      vec![],
    )?;

    // Update the planet a few times
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);
    let ship_snapshot = entities.ship_deep_copy();
    entities.update_all(&ship_snapshot, &BoostMap::default(), &mut rng);

    // Validate the position remains the same
    let expected_position = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(
      entities.planets.get("Sun").unwrap().read().unwrap().get_position(),
      expected_position
    );
    Ok(())
  }
  #[test_log::test]
  // TODO: Add test to add a moon.
  fn test_complex_planet_update() -> Result<(), String> {
    const EARTH_RADIUS: f64 = 151.25e9;

    fn check_radius_and_y(pos: Vec3, primary: Vec3, expected_mag: f64, expected_y: f64) -> (bool, bool) {
      const TOLERANCE: f64 = 0.01;
      let radius = pos - primary;
      let radius_2d = Vector2::<f64>::new(radius.x, radius.z);

      debug!(
        "Radius_2d.magnitude(): {:?} vs Expected: {}",
        radius_2d.magnitude(),
        expected_mag
      );
      (
        (radius_2d.magnitude() - expected_mag).abs() / expected_mag < TOLERANCE,
        (radius.y - expected_y).abs() / expected_y < TOLERANCE,
      )
    }

    let _ = pretty_env_logger::try_init();
    let mut rng = SmallRng::seed_from_u64(0);

    let mut entities = Entities::new();

    // Create some planets and see if they move.
    entities.add_planet(
      String::from("Planet1"),
      Vec3::new(EARTH_RADIUS, 2_000_000.0, 0.0),
      String::from("blue"),
      None,
      6.371e6,
      6e24,
      vec![],
    )?;
    entities.add_planet(
      String::from("Planet2"),
      Vec3::new(0.0, 5_000_000.0, EARTH_RADIUS),
      String::from("red"),
      None,
      3e7,
      3e23,
      vec![],
    )?;
    entities.add_planet(
      String::from("Planet3"),
      Vec3::new(EARTH_RADIUS / 2.0_f64.sqrt(), 8000.0, EARTH_RADIUS / 2.0_f64.sqrt()),
      String::from("green"),
      None,
      4e6,
      1e26,
      vec![],
    )?;

    // Update the entities a few times
    entities.update_all(&entities.ship_deep_copy(), &BoostMap::default(), &mut rng);
    entities.update_all(&entities.ship_deep_copy(), &BoostMap::default(), &mut rng);
    entities.update_all(&entities.ship_deep_copy(), &BoostMap::default(), &mut rng);

    // FIXME: This isn't really testing what we want to test.
    // Fix it so we have real primaries and test the distance to those.
    assert_eq!(
      (true, true),
      check_radius_and_y(
        entities.planets.get("Planet1").unwrap().read().unwrap().get_position(),
        Vec3::zero(),
        EARTH_RADIUS,
        2_000_000.0
      )
    );
    assert_eq!(
      (true, true),
      check_radius_and_y(
        entities.planets.get("Planet2").unwrap().read().unwrap().get_position(),
        Vec3::zero(),
        EARTH_RADIUS,
        5_000_000.0
      )
    );
    assert_eq!(
      (true, true),
      check_radius_and_y(
        entities.planets.get("Planet3").unwrap().read().unwrap().get_position(),
        Vec3::zero(),
        EARTH_RADIUS,
        8_000.0
      )
    );

    Ok(())
  }

  // A test of deserializing a planet string.
  #[test_log::test]
  fn test_serialize_planet() {
    let _ = pretty_env_logger::try_init();

    let tst_planet = Planet::new(
      String::from("Sun"),
      Vec3::zero(),
      String::from("yellow"),
      7e8,
      100.0,
      None,
      &None,
      0,
    );

    let tst_str = serde_json::to_string(&tst_planet).unwrap();
    assert_eq!(
      tst_str,
      r#"{"name":"Sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":700000000.0,"mass":100.0,"visual_effects":[]}"#
    );

    let tst_planet_2 = Planet::new(
      String::from("planet2"),
      Vec3 {
        x: 1_000_000_000.0,
        y: 0.0,
        z: 0.0,
      },
      String::from("red"),
      4e6,
      100.0,
      Some(String::from("planet1")),
      &Some(Arc::new(RwLock::new(tst_planet))),
      1,
    );

    let tst_str = serde_json::to_string(&tst_planet_2).unwrap();
    assert_eq!(
      tst_str,
      r#"{"name":"planet2","position":[1000000000.0,0.0,0.0],"velocity":[0.0,0.0,2.583215051055564e-9],"color":"red","radius":4000000.0,"mass":100.0,"primary":"planet1","visual_effects":[]}"#
    );

    // This is a special case of an planet.  It typically should never have a primary that is Some(...) but a primary_ptr that is None
    // However, the one exception is when it comes off the wire, which is what we are testing here.
    let tst_planet_3 = Planet::new(
      String::from("planet2"),
      Vec3::zero(),
      String::from("red"),
      4e6,
      100.0,
      Some(String::from("planet1")),
      &None,
      0,
    );

    let tst_str = r#"{"name":"planet2","position":[0,0,0],"velocity":[0.0,0.0,0.0],
        "color":"red","radius":4e6,"mass":100.0,"primary":"planet1"}"#;
    let tst_planet_4 = serde_json::from_str::<Planet>(tst_str).unwrap();

    assert_eq!(tst_planet_3, tst_planet_4);
  }

  #[test_log::test]
  fn test_mixed_entities_serialize() -> Result<(), String> {
    // This constant is the radius of the earth's orbit (distance from sun).
    // It is NOT the radius of the earth (6.371e6 m)
    const EARTH_RADIUS: f64 = 151.25e9;

    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    // Create some planets and see if they move.
    entities.add_planet(
      String::from("Planet1"),
      Vec3::new(EARTH_RADIUS, 2_000_000.0, 0.0),
      String::from("blue"),
      None,
      6.371e6,
      5.972e24,
      vec![],
    )?;
    entities.add_planet(
      String::from("Planet2"),
      Vec3::new(0.0, 5_000_000.0, EARTH_RADIUS),
      String::from("red"),
      None,
      3e7,
      3.00e23,
      vec![],
    )?;
    entities.add_planet(
      String::from("Planet3"),
      Vec3::new(EARTH_RADIUS / 2.0_f64.sqrt(), 8000.0, EARTH_RADIUS / 2.0_f64.sqrt()),
      String::from("green"),
      None,
      4e6,
      1e26,
      vec![],
    )?;

    // Create entities with random positions and names
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1000.0, 2000.0, 3000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4000.0, 5000.0, 6000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities.add_ship(
      String::from("Ship3"),
      Vec3::new(7000.0, 8000.0, 9000.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );

    let cmp = json!({
    "metadata":{"name":"","description":"","owner":""},
    "filename":"",
    "ships":[
        {"name":"Ship1","position":[1000.0,2000.0,3000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_computer": 5,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": [],
        "crit_level": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        },
        {"name":"Ship2","position":[4000.0,5000.0,6000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_computer": 5,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": [],
        "crit_level": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        },
        {"name":"Ship3","position":[7000.0,8000.0,9000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_computer": 5,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": [],
        "crit_level": [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]
        }],
    "missiles":[],
    "actions":[],
    "planets":[
        {"name":"Planet1","position":[151_250_000_000.0,2_000_000.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6_371_000.0,"mass":5.972e24,"visual_effects":[],
        "gravity_radius_1":6_375_069.342_849_095,"gravity_radius_05":9_015_709.525_726_125,"gravity_radius_025":12_750_138.685_698_19},
        {"name":"Planet2","position":[0.0,5_000_000.0,151_250_000_000.0],"velocity":[0.0,0.0,0.0],"color":"red","radius":30_000_000.0,"mass":3.00e23,"visual_effects":[]},
        {"name":"Planet3","position":[106_949_900_654.465_3,8000.0,106_949_900_654.465_3],"velocity":[0.0,0.0,0.0],"color":"green","radius":4_000_000.0,"mass":1e26,"visual_effects":[],
        "gravity_radius_2":18_446_331.779_326_223,"gravity_radius_1":26_087_052.578_356_97,"gravity_radius_05":36_892_663.558_652_446,"gravity_radius_025":52_174_105.156_713_94}
     ]});

    assert_json_eq!(&entities, &cmp);

    Ok(())
  }
  #[tokio::test]
  async fn test_unordered_scenario_file() {
    let _ = pretty_env_logger::try_init();
    config_test_ship_templates().await;

    let entities = Entities::load_from_file("./tests/scenarios/test-scenario.json").await.unwrap();
    assert!(entities.validate(), "Scenario file failed validation");
  }

  #[test_log::test(tokio::test)]
  async fn test_load_from_file_uses_provided_ship_template_snapshot() {
    // Held for the whole test: it installs its own global registry and asserts
    // on it further down, so no other test may re-seed SHIP_TEMPLATES in the
    // meantime. Declared before the restore guard so the guard runs first.
    let templates_lock = lock_ship_templates_for_test().await;
    config_test_ship_templates_locked(&templates_lock).await;

    let previous_templates = get_ship_templates_snapshot();
    let _restore_guard = ShipTemplateRestoreGuard(previous_templates.as_ref().clone());

    let design_name = "Scenario Snapshot Test".to_string();
    let original_template = Arc::new(ShipDesignTemplate {
      name: design_name.clone(),
      displacement: 100,
      hull: 10,
      armor: 2,
      maneuver: 1,
      jump: 1,
      power: 25,
      fuel: 10,
      crew: 4,
      sensors: crate::ship::Sensors::Basic,
      stealth: None,
      countermeasures: None,
      computer: 1,
      crew_skills: None,
      weapons: vec![],
      screens: vec![],
      tl: 10,
      role: None,
      source: None,
    });

    let mut scenario_templates = previous_templates.as_ref().clone();
    scenario_templates.insert(design_name.clone(), original_template);
    let scenario_templates = Arc::new(scenario_templates);

    let mut reloaded_templates = previous_templates.as_ref().clone();
    let mut updated_template = (*scenario_templates.get(&design_name).unwrap()).as_ref().clone();
    updated_template.power = 99;
    reloaded_templates.insert(design_name.clone(), Arc::new(updated_template));
    replace_ship_templates(reloaded_templates);

    let scenario_path = std::env::temp_dir().join(format!(
      "callisto_scenario_snapshot_{}_{}.json",
      std::process::id(),
      SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()
    ));

    let scenario = json!({
      "metadata": {"name": "Snapshot test", "description": "Template snapshot test", "owner": "test-user"},
      "ships": [{
        "name": "Snapshot Test Ship",
        "position": [0.0, 0.0, 0.0],
        "velocity": [0.0, 0.0, 0.0],
        "plan": [[[0.0, 0.0, 0.0], 50000]],
        "design": design_name,
      }],
      "planets": [],
      "missiles": [],
      "actions": []
    });
    fs::write(&scenario_path, serde_json::to_vec(&scenario).unwrap()).unwrap();

    let entities = Entities::load_from_file_with_ship_templates(scenario_path.to_str().unwrap(), scenario_templates)
      .await
      .unwrap();

    let _ = fs::remove_file(&scenario_path);

    let ship = entities.ships.get("Snapshot Test Ship").unwrap().read().unwrap();
    assert_eq!(ship.design.power, 25);
    assert_eq!(get_ship_template("Scenario Snapshot Test").unwrap().power, 99);
  }

  #[test_log::test(tokio::test)]
  async fn test_entities_equality() -> Result<(), String> {
    config_test_ship_templates().await;

    let mut entities1 = Entities::new();
    let mut entities2 = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    // Add some ships
    entities1.add_ship(
      "Ship1".to_string(),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::new(0.1, 0.2, 0.3),
      &design,
      None,
      None,
    );
    entities2.add_ship(
      "Ship1".to_string(),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::new(0.1, 0.2, 0.3),
      &design,
      None,
      None,
    );

    // Add some planets
    entities1.add_planet(
      "Planet1".to_string(),
      Vec3::new(7.0, 8.0, 9.0),
      "green".to_string(),
      None,
      6371e3,
      5.97e24,
      vec![],
    )?;
    entities2.add_planet(
      "Planet1".to_string(),
      Vec3::new(7.0, 8.0, 9.0),
      "green".to_string(),
      None,
      6371e3,
      5.97e24,
      vec![],
    )?;

    // Test equality
    assert_eq!(entities1, entities2, "Entities should be equal");

    // Modify one entity and test inequality
    entities2
      .ships
      .get_mut("Ship1")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(1.1, 2.1, 3.1));
    assert_ne!(entities1, entities2, "Entities should not be equal after modifying a ship");

    // Reset entities2
    entities2
      .ships
      .get_mut("Ship1")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(1.0, 2.0, 3.0));

    // Add an extra entity to entities1
    entities1.add_ship(
      "Ship2".to_string(),
      Vec3::new(10.0, 11.0, 12.0),
      Vec3::new(1.0, 1.1, 1.2),
      &design,
      None,
      None,
    );
    assert_ne!(
      entities1, entities2,
      "Entities should not be equal with different number of ships"
    );

    // Add the same extra entity to entities2
    entities2.add_ship(
      "Ship2".to_string(),
      Vec3::new(10.0, 11.0, 12.0),
      Vec3::new(1.0, 1.1, 1.2),
      &design,
      None,
      None,
    );
    assert_eq!(entities1, entities2, "Entities should be equal again");

    // Add some missiles to test
    entities1.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();

    // Test the two should not be equal
    assert_ne!(
      entities1, entities2,
      "Entities should not be equal with different number of missiles"
    );

    // Add the same missile to entities2
    entities2.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();
    assert_eq!(entities1, entities2, "Entities should be equal again");

    // Test with a different missile
    entities1.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();
    assert_ne!(entities1, entities2, "Entities should not be equal with different missiles");

    // Add the same missile to entities2
    entities2.launch_missile("Ship1", "Ship2", test_missile_weapon()).unwrap();
    assert_eq!(entities1, entities2, "Entities should be equal again");

    // Test with floating-point precision issues
    let mut entities3 = entities1.clone();
    entities3
      .planets
      .get_mut("Planet1")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(7.0 + 1e-32, 8.0, 9.0));
    assert_eq!(entities1, entities3, "Entities should be equal within floating-point precision");

    // Test with a significant change
    entities3
      .planets
      .get_mut("Planet1")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(7.0 + 1e-6, 8.0, 9.0));
    assert_ne!(
      entities1, entities3,
      "Entities should not be equal with significant position change"
    );

    // Test with velocity change.  This is kind of extreme as its on a missile and this should never happen in real code.980p[]'
    let mut entities4 = entities1.clone();
    entities4
      .missiles
      .get_mut("Ship1::Ship2::0")
      .unwrap()
      .write()
      .unwrap()
      .set_velocity(Vec3::new(0.41, 0.51, 0.61));
    assert_ne!(entities1, entities4, "Entities should not be equal after velocity change");

    Ok(())
  }

  #[test_log::test]
  fn test_entities_len_and_is_empty() -> Result<(), String> {
    let mut entities = Entities::new();

    // Test empty entities
    assert_eq!(entities.len(), 0);
    assert!(entities.is_empty());

    // Add a ship
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test entities with one ship
    assert_eq!(entities.len(), 1);
    assert!(!entities.is_empty());

    // Add a planet
    entities.add_planet(
      String::from("Planet1"),
      Vec3::new(4.0, 5.0, 6.0),
      String::from("blue"),
      None,
      6371e3,
      5.97e24,
      vec![],
    )?;

    // Test entities with one ship and one planet
    assert_eq!(entities.len(), 2);
    assert!(!entities.is_empty());

    // Test with an empty entities
    entities = Entities::new();
    assert_eq!(entities.len(), 0);
    assert!(entities.is_empty());

    Ok(())
  }
  #[test_log::test]
  fn test_launch_missile_invalid_target() {
    let mut entities = Entities::new();
    let design = Arc::new(ShipDesignTemplate::default());

    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );

    // Test launching a missile with an invalid target
    assert!(
      entities.launch_missile("Ship1", "Ship2", test_missile_weapon()).is_err(),
      "Launching a missile with an invalid target should be an error"
    );

    // Test launching a missile with an invalid source
    assert!(
      entities.launch_missile("Ship2", "Ship1", test_missile_weapon()).is_err(),
      "Launching a missile with an invalid source should be an error"
    );
  }

  #[test_log::test(tokio::test)]
  async fn test_fixup_pointers() {
    config_test_ship_templates().await;

    // The best way to test this to to build a scenario file and then
    // deserialize it into an Entities struct.
    // Then we run fixup_pointers on it.
    // Then we do the same thing but with an invalid scenario file.
    // Then we run fixup_pointers on it and it should fail.

    // Test 1: Valid file
    let scenario = json!({"ships":[
            {"name":"ship1","position":[1_000_000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500_000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30},
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"sun"}]});

    let mut entities = serde_json::from_value::<Entities>(scenario).unwrap();
    assert!(entities.fixup_pointers().is_ok(), "Error fixing up pointers");

    // Test 2: Add missile with a non-existent target
    let bad_scenario = json!({"ships":[
            {"name":"ship1","position":[1_000_000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500_000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2},
             {"name":"Invalid::1","source":"ship1","target":"InvalidShip","position":[0.0,0.0,500_000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30},
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"sun"}]});

    let mut entities = serde_json::from_value::<Entities>(bad_scenario).unwrap();
    assert!(
      entities.fixup_pointers().is_err(),
      "Scenario file with bad missile should fail fixup_pointers"
    );

    // Test3: Add a planet with a non-existent primary
    let bad_scenario = json!({"ships":[
            {"name":"ship1","position":[1_000_000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500_000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30},
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"InvalidPlanet"}]});

    let mut entities = serde_json::from_value::<Entities>(bad_scenario).unwrap();
    assert!(
      entities.fixup_pointers().is_err(),
      "Scenario file with bad planet should fail fixup_pointers"
    );
  }
  #[test_log::test]
  fn test_set_flight_plan() {
    let mut entities = Entities::new();

    // Add a ship
    entities.add_ship(
      String::from("TestShip"),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Create a flight plan
    let acceleration = Vec3::new(1.0, 2.0, 2.0);
    let duration = 5000;
    let plan = FlightPlan::new((acceleration, duration).into(), None);

    // Set the flight plan
    let result = entities.set_flight_plan("TestShip", &plan);

    // Assert that the flight plan was set successfully
    assert!(result.is_ok(), "Flight plan should be set successfully");

    // Verify that the flight plan was set correctly
    if let Some(ship) = entities.ships.get("TestShip") {
      let ship_plan = &ship.read().unwrap().plan;
      assert_eq!(ship_plan.0 .0, acceleration, "Acceleration should match");
      assert_eq!(ship_plan.0 .1, duration, "Duration should match");
      assert!(ship_plan.1.is_none(), "Second acceleration should be None");
    } else {
      panic!("TestShip not found in entities");
    }

    // Test setting flight plan for non-existent ship
    let result = entities.set_flight_plan("NonExistentShip", &plan);
    assert!(result.is_err(), "Setting flight plan for non-existent ship should fail");
  }

  fn create_test_ship_sensors(name: &str, sensor_skill: u8) -> Ship {
    let mut crew = Crew::default();
    crew.set_skill(Skills::Sensors, sensor_skill);
    let mut ship = Ship::default();
    ship.set_crew(crew);
    ship.set_name(name.to_string());
    ship
  }

  fn create_test_missile(name: &str, target: &str) -> Missile {
    let mut missile = Missile::default();
    missile.set_name(name.to_string());
    missile.target = target.to_string();
    missile
  }

  #[test_log::test]
  fn test_jam_missiles() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(5, 0); // Will always roll 6 for predictable results

    // Create a ship with good sensor skills
    let ship = create_test_ship_sensors("defender", 4);
    entities.ships.insert("defender".to_string(), Arc::new(RwLock::new(ship)));

    let actions = vec![("defender".to_string(), vec![ShipAction::JamMissiles])];

    // Create missiles targeting the defender
    for i in 1..=8 {
      let missile = create_test_missile(&format!("missile{i}"), "defender");
      entities.missiles.insert(format!("missile{i}"), Arc::new(RwLock::new(missile)));
    }

    entities.fixup_pointers().unwrap();

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    // With a roll of 6 and sensor skill of 4, check should be positive
    // resulting in successful jamming
    assert_eq!(entities.missiles.len(), 1); // Only one missile should be left
    assert_eq!(effects.len(), 15); // One message for jamming success and one for missile destruction
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("destroyed by jamming")
    )));

    let mut entities = Entities::default();
    let mut rng = StepRng::new(1, 0); // Will always roll 1 for predictable results
    let ship = create_test_ship_sensors("defender", 4);
    entities.ships.insert("defender".to_string(), Arc::new(RwLock::new(ship)));

    let actions = vec![("defender".to_string(), vec![ShipAction::JamMissiles])];

    // Create missiles targeting the defender
    let missile1 = create_test_missile("missile1", "defender");
    let missile2 = create_test_missile("missile2", "defender");
    entities
      .missiles
      .insert("missile1".to_string(), Arc::new(RwLock::new(missile1)));
    entities
      .missiles
      .insert("missile2".to_string(), Arc::new(RwLock::new(missile2)));

    entities.fixup_pointers().unwrap();

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    // With a roll of 1 and sensor skill of 4, check should be negative
    // resulting in failed jamming
    assert_eq!(entities.missiles.len(), 2); // No missiles should be destroyed due to check result
    assert_eq!(effects.len(), 1); // Only one message for jamming failure
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("defender jams inbound missiles") && content.contains("jamming failed")
    )));
  }

  /// Same-round-impact jamming: when an attacker launches a missile at a
  /// nearby defender close enough that the missile would impact during the
  /// same round's `update_all`, the defender's `JamMissiles` action must
  /// still be able to destroy it. This is the bug fixed by moving the
  /// `JamMissiles` pass to after `fire_actions` in `Player::update`. Here
  /// we exercise the same ordering directly on `Entities`: launch the
  /// missile, then run the jam pass, and confirm the just-launched missile
  /// is destroyed (and never had a chance to impact).
  #[test_log::test]
  fn test_jam_missiles_after_same_round_launch() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(5, 0); // Roll 6 → jamming succeeds with sensor 4

    // Attacker and a defender with strong sensors. No specific positioning
    // is required to exercise the bug at the Entities level — what matters
    // is that the missile is in `self.missiles` when `jam_missiles` runs,
    // which is exactly what happens once the jam pass moves after
    // `fire_actions`.
    entities.ships.insert(
      "attacker".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("attacker", 0))),
    );
    entities.ships.insert(
      "defender".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("defender", 4))),
    );

    // Mimic the post-fix round ordering: a launch (from fire_actions) followed
    // by a jam pass (the second sensor_actions call).
    entities.launch_missile("attacker", "defender", test_missile_weapon()).unwrap();
    assert_eq!(entities.missiles.len(), 1, "Missile should be in flight after launch");

    let actions = vec![("defender".to_string(), vec![ShipAction::JamMissiles])];
    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    assert_eq!(
      entities.missiles.len(),
      0,
      "Just-launched missile should be destroyed by post-fire jamming"
    );
    assert!(
      effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("destroyed by jamming")
      )),
      "Expected a 'destroyed by jamming' effect message"
    );
  }

  #[test_log::test]
  fn test_sensor_lock() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(1, 0); // Will always roll 2 for predictable results

    // Create two ships
    let ship1 = create_test_ship_sensors("attacker", 2);
    let ship2 = create_test_ship_sensors("target", 2);

    let actions = vec![(
      "attacker".to_string(),
      vec![ShipAction::SensorLock {
        target: "target".to_string(),
      }],
    )];
    entities.ships.insert("attacker".to_string(), Arc::new(RwLock::new(ship1)));
    entities.ships.insert("target".to_string(), Arc::new(RwLock::new(ship2)));
    // Inserting straight into the map bypasses add_ship, so seed the contacts a
    // loaded scenario would already have. Without them the action is refused
    // for want of a sensor contact rather than resolved.
    entities.establish_initial_contacts();

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("attempts a sensor lock on target") && content.contains("no lock")
    )));
    let mut rng = StepRng::new(5, 0); // Will always roll 6 for predictable results

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    // With a roll of 6 and sensor skill of 4, the lock should be established
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("attempts a sensor lock on target") && content.contains("lock established")
    )));

    let attacker = entities.ships.get("attacker").unwrap().read().unwrap();

    assert!(attacker.sensor_locks.contains(&"target".to_string()));
  }

  #[test]
  fn test_break_sensor_lock() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(6, 0);

    // Create two ships
    let ship1 = create_test_ship_sensors("defender", 4);
    let mut ship2 = create_test_ship_sensors("attacker", 2);

    // Set up initial sensor lock
    ship2.sensor_locks.push("defender".to_string());
    let actions = vec![(
      "defender".to_string(),
      vec![ShipAction::BreakSensorLock {
        target: "attacker".to_string(),
      }],
    )];

    entities.ships.insert("defender".to_string(), Arc::new(RwLock::new(ship1)));
    entities
      .ships
      .insert("attacker".to_string(), Arc::new(RwLock::new(ship2.clone())));

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    // Check that the lock was broken
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("broke")
    )));

    {
      let attacker = entities.ships.get("attacker").unwrap().read().unwrap();
      assert!(attacker.sensor_locks.is_empty());
    }

    let mut rng = StepRng::new(1, 1); // Increment rolls to have attacker win (they roll second)
    entities
      .ships
      .get("attacker")
      .unwrap()
      .write()
      .unwrap()
      .sensor_locks
      .push("defender".to_string());
    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    // Check that the lock was not broken
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("breaks the sensor lock held by") && content.contains("lock holds")
    )));
  }

  #[test]
  fn test_jam_comms() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(0, 0); // Will always roll 1 for predictable results

    // Create two ships
    let ship1 = create_test_ship_sensors("jammer", 2);
    let ship2 = create_test_ship_sensors("target", 3);

    let actions = vec![(
      "jammer".to_string(),
      vec![ShipAction::JamComms {
        target: "target".to_string(),
      }],
    )];

    entities.ships.insert("jammer".to_string(), Arc::new(RwLock::new(ship1)));
    entities.ships.insert("target".to_string(), Arc::new(RwLock::new(ship2)));
    // Bypassing add_ship means no contacts; seed them as a scenario load would.
    entities.establish_initial_contacts();

    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);

    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("jams comms on") && content.contains("jamming failed")
    )));

    let mut rng = StepRng::new(4, 1); // Going past 6 on second two rolls ensures jammer wins
    let boost_map = BoostMap::default();
    let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);
    // With high sensor skill and good roll, jamming should succeed
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content, .. } if content.contains("jams comms on") && content.contains("comms jammed")
    )));
  }

  async fn setup_sensor_test_ships(
    attack_name: &str, attack_crew_skill: u8, target_name: &str, target_crew_skill: u8, attack_design: &str,
    target_design: &str,
  ) -> Entities {
    // Load ship templates
    let templates = crate::ship::load_ship_templates_from_dir(crate::ship::DEFAULT_SHIP_TEMPLATES_DIR)
      .await
      .expect("Unable to load ship templates directory.");

    // Create entities
    let mut entities = Entities::new();

    // Create attacker ship
    let mut attack_ship = Ship::default();
    attack_ship.set_name(attack_name.to_string());
    attack_ship.design = templates.get(attack_design).unwrap().clone();
    attack_ship.current_sensors = attack_ship.design.sensors;
    let mut attack_crew = Crew::default();
    attack_crew.set_skill(Skills::Sensors, attack_crew_skill);
    attack_ship.set_crew(attack_crew);

    // Create target ship
    let mut target_ship = Ship::default();
    target_ship.set_name(target_name.to_string());
    target_ship.design = templates.get(target_design).unwrap().clone();
    let mut target_crew = Crew::default();
    target_crew.set_skill(Skills::Sensors, target_crew_skill);
    target_ship.set_crew(target_crew);

    // Add ships to entities
    entities
      .ships
      .insert(attack_name.to_string(), Arc::new(RwLock::new(attack_ship)));
    entities
      .ships
      .insert(target_name.to_string(), Arc::new(RwLock::new(target_ship)));

    entities
  }

  /// Build two ships a fixed distance apart, with an optional stealth grade on
  /// the second, for the detection-pass tests below.
  fn detection_pair(stealth: Option<crate::ship::Stealth>, separation: f64) -> Entities {
    let mut entities = Entities::default();
    let plain = Arc::new(ShipDesignTemplate::default());
    let hidden = Arc::new(ShipDesignTemplate {
      stealth,
      ..ShipDesignTemplate::default()
    });
    entities.add_ship("Seeker".to_string(), Vec3::zero(), Vec3::zero(), &plain, None, None);
    entities.add_ship(
      "Quarry".to_string(),
      Vec3::new(separation, 0.0, 0.0),
      Vec3::zero(),
      &hidden,
      None,
      None,
    );
    // Scenarios no longer open with contacts; these tests are about what
    // happens once there is one, so seed explicitly.
    entities.establish_initial_contacts();
    entities
  }

  fn holds_contact(entities: &Entities, observer: &str, target: &str) -> bool {
    entities
      .ships
      .get(observer)
      .unwrap()
      .read()
      .unwrap()
      .contacts
      .iter()
      .any(|name| name == target)
  }

  /// A stealthed hull is not handed to the enemy at scenario load; an ordinary
  /// one is, because it would be found within a round or two regardless.
  #[test]
  fn stealthed_ships_start_undetected() {
    let entities = detection_pair(Some(crate::ship::Stealth::Advanced), 10_000.0);
    assert!(!holds_contact(&entities, "Seeker", "Quarry"), "stealth should start hidden");
    assert!(
      holds_contact(&entities, "Quarry", "Seeker"),
      "the plain hull should still be visible to the stealth ship"
    );

    let entities = detection_pair(None, 10_000.0);
    assert!(
      holds_contact(&entities, "Seeker", "Quarry"),
      "an ordinary hull should start detected"
    );
  }

  /// A scenario that opens with ships beyond Distant opens with them unaware
  /// of each other: past 50,000 km everything is an undifferentiated blip, so
  /// there is nothing to seed.
  #[test]
  fn ships_beyond_distant_are_not_seeded() {
    let entities = detection_pair(None, 6.0e7);
    assert!(
      !holds_contact(&entities, "Seeker", "Quarry"),
      "nothing should be in contact across more than Distant"
    );
    assert!(!holds_contact(&entities, "Quarry", "Seeker"));

    // Just inside the edge, they are.
    let entities = detection_pair(None, 4.9e7);
    assert!(holds_contact(&entities, "Seeker", "Quarry"), "inside Distant is seeded");
  }

  /// Acquisition needs active sensors: "attempting to locate a ship with this
  /// level of accuracy requires the use of active sensors" (High Guard p. 77).
  #[test]
  fn a_dark_ship_acquires_nothing() {
    let mut entities = detection_pair(Some(crate::ship::Stealth::Basic), 10_000.0);
    entities
      .ships
      .get("Seeker")
      .unwrap()
      .write()
      .unwrap()
      .set_emissions(Some(false), None);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(4);

    for _ in 0..30 {
      entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);
    }

    assert!(
      !holds_contact(&entities, "Seeker", "Quarry"),
      "a ship running dark should never acquire a contact, however long it looks"
    );
  }

  /// With sensors up, a merely Basic-stealth hull is found before long.
  #[test]
  fn an_active_ship_eventually_finds_a_stealthed_one() {
    let mut entities = detection_pair(Some(crate::ship::Stealth::Basic), 10_000.0);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(4);

    let mut found = false;
    for _ in 0..30 {
      entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);
      if holds_contact(&entities, "Seeker", "Quarry") {
        found = true;
        break;
      }
    }
    assert!(found, "30 rounds should be more than enough to find a Basic-stealth hull");
  }

  /// Build a picket with a contact nobody else has, plus a quiet team-mate.
  fn handoff_pair(picket_team: Option<crate::ship::Team>, mate_team: Option<crate::ship::Team>) -> Entities {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    for (name, pos) in [("Picket", 0.0), ("Mate", 1.0e6), ("Bogey", 2.0e6)] {
      entities.add_ship(name.to_string(), Vec3::new(pos, 0.0, 0.0), Vec3::zero(), &design, None, None);
    }
    {
      let mut picket = entities.ships.get("Picket").unwrap().write().unwrap();
      picket.team = picket_team;
      picket.set_handoff_sensors(true);
    }
    // The picket has found everything; the quiet ship has found nothing, which
    // is what the hand-off is for.
    entities.establish_initial_contacts();
    {
      let mut mate = entities.ships.get("Mate").unwrap().write().unwrap();
      mate.team = mate_team;
      mate.contacts.clear();
    }
    entities
  }

  /// The point of the mechanic: a quiet ship inherits what the picket can see.
  #[test]
  fn handoff_shares_contacts_within_a_team() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    let effects = entities.sensor_handoff_pass();

    assert!(
      holds_contact(&entities, "Mate", "Bogey"),
      "the quiet ship should inherit the picket's contact"
    );
    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("receives contact on Bogey"))),
      "the hand-off should be reported"
    );
  }

  /// Nothing is shared across sides, or with an unaligned ship.
  #[test]
  fn handoff_does_not_cross_teams() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Blue));
    entities.sensor_handoff_pass();
    assert!(!holds_contact(&entities, "Mate", "Bogey"), "the other side should get nothing");

    let mut entities = handoff_pair(Some(crate::ship::Team::Red), None);
    entities.sensor_handoff_pass();
    assert!(
      !holds_contact(&entities, "Mate", "Bogey"),
      "an unaligned ship should get nothing"
    );
  }

  /// "A hand-off requires one point of available computer Bandwidth from both
  /// the host and recipient ship" (High Guard p. 78).
  #[test]
  fn handoff_needs_bandwidth_at_both_ends() {
    let says = |effects: &[EffectMsg], fragment: &str| {
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains(fragment)))
    };

    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities.ships.get("Mate").unwrap().write().unwrap().current_computer = 0;
    let effects = entities.sensor_handoff_pass();
    assert!(
      !holds_contact(&entities, "Mate", "Bogey"),
      "a recipient with no Bandwidth cannot receive"
    );
    assert!(
      says(&effects, "Mate cannot receive a sensor hand-off"),
      "and should be told why rather than failing quietly"
    );

    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities.ships.get("Picket").unwrap().write().unwrap().current_computer = 0;
    let effects = entities.sensor_handoff_pass();
    assert!(
      !holds_contact(&entities, "Mate", "Bogey"),
      "a host with no Bandwidth cannot send"
    );
    assert!(
      says(&effects, "Picket cannot hand off its sensor picture"),
      "and should be told why rather than failing quietly"
    );
  }

  /// The Bandwidth warnings are only worth printing when a hand-off was really
  /// on offer -- otherwise every quiet ship nags every round for nothing.
  #[test]
  fn no_bandwidth_warning_when_there_was_nothing_to_share() {
    // A host with the setting on but no contacts of its own.
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    {
      let mut picket = entities.ships.get("Picket").unwrap().write().unwrap();
      picket.current_computer = 0;
      picket.contacts.clear();
    }
    assert!(
      entities.sensor_handoff_pass().is_empty(),
      "a host with nothing to share should say nothing"
    );

    // A recipient that already holds everything the host could offer.
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    {
      let picket_contacts = entities.ships.get("Picket").unwrap().read().unwrap().contacts.clone();
      let mut mate = entities.ships.get("Mate").unwrap().write().unwrap();
      mate.current_computer = 0;
      mate.contacts = picket_contacts;
    }
    assert!(
      entities.sensor_handoff_pass().is_empty(),
      "a recipient that would gain nothing should not be warned"
    );
  }

  /// A ship loaded from a scenario file gets its Bandwidth from its design.
  ///
  /// Regression: `current_computer` was set only in `Ship::new`, and was the one
  /// `current_*` field missing from `fixup_current_values`. A ship whose JSON
  /// omitted the key -- which is every ship hand-added to a scenario file --
  /// therefore loaded with Bandwidth 0 and could neither send nor receive a
  /// hand-off, silently, while its design said 5. Every hand-off test above
  /// builds ships through `add_ship`, so none of them saw it.
  #[test_log::test(tokio::test)]
  async fn handoff_works_for_ships_loaded_without_a_computer_rating() {
    config_test_ship_templates().await;

    // Note what is NOT here: no `current_computer` on either ship.
    let scenario = json!({"ships":[
        {"name":"Picket","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
         "team":"Red","handoff_sensors":true,"contacts":["Bogey"]},
        {"name":"Mate","position":[1.0e6,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
         "team":"Red"},
        {"name":"Bogey","position":[2.0e6,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer"}]});

    let mut entities = Entities::parse_bytes_with_ship_templates(
      scenario.to_string().as_bytes(),
      "handoff.json",
      get_ship_templates_snapshot(),
    )
    .unwrap();

    for name in ["Picket", "Mate"] {
      let ship = entities.ships.get(name).unwrap().read().unwrap();
      assert_eq!(
        ship.current_computer, ship.design.computer,
        "{name} should take its Bandwidth from its design"
      );
      assert!(ship.current_computer > 0, "the test design must have a computer");
    }

    entities.sensor_handoff_pass();
    assert!(
      holds_contact(&entities, "Mate", "Bogey"),
      "a loaded ship with a design computer rating should be able to host a hand-off"
    );
  }

  /// A hand-off cannot convey a contact the recipient could not hold.
  ///
  /// Regression: the link's range was checked, the contact's was not. A picket
  /// close to the quarry shared it with a team-mate 60,000 km away, the
  /// detection pass dropped it as out of range, the hand-off pass re-shared it,
  /// and the contact sat beyond Very Long indefinitely.
  #[test]
  fn handoff_does_not_convey_a_contact_beyond_the_recipients_distant() {
    // Picket at the origin with the Bogey close by. The mate is placed so the
    // link to the picket holds (inside Distant) but the Bogey itself is not.
    let place_mate = |x: f64| {
      let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
      entities
        .ships
        .get("Mate")
        .unwrap()
        .write()
        .unwrap()
        .set_position(Vec3::new(x, 0.0, 0.0));
      entities.sensor_handoff_pass();
      holds_contact(&entities, "Mate", "Bogey")
    };
    // Bogey is at +2.0e6. Mate at -4.9e7: link is 4.9e7 (holds), Bogey is 5.1e7 (Distant).
    assert!(
      !place_mate(-4.9e7),
      "a contact beyond the recipient's Distant must not be handed to it"
    );
    // Mate at -4.7e7: link 4.7e7, Bogey 4.9e7 -- both inside Distant.
    assert!(place_mate(-4.7e7), "and inside it the hand-off still works");
  }

  /// "If one or more of the ships in a hand-off strays beyond Distant range,
  /// the connection is lost."
  #[test]
  fn handoff_breaks_beyond_distant() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities
      .ships
      .get("Mate")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(6.0e7, 0.0, 0.0));
    entities.sensor_handoff_pass();
    assert!(
      !holds_contact(&entities, "Mate", "Bogey"),
      "the link should not reach past Distant"
    );
  }

  /// A named ship's crew lives on its design, and a scenario inherits it.
  ///
  /// HMS Executor is one hull with one crew, but her skills were restated in
  /// every scenario and had drifted into three different crews. The design is
  /// now the single source; a scenario that states its own `crew` still wins,
  /// so a wounded or replacement crew stays expressible.
  #[test_log::test(tokio::test)]
  async fn a_scenario_inherits_its_design_crew_but_can_override_it() {
    config_test_ship_templates().await;
    let templates = get_ship_templates_snapshot();
    let design = templates.get("HMS Executor").expect("Executor design");
    let from_design = design.crew_skills.clone().expect("Executor names her crew");
    assert!(from_design.get_sensors() > 0, "the fixture must have a non-default crew");

    let parse = |json: serde_json::Value| {
      Entities::parse_bytes_with_ship_templates(json.to_string().as_bytes(), "crew.json", get_ship_templates_snapshot())
        .unwrap()
    };

    // No `crew` key: the design's crew comes aboard.
    let inherited = parse(json!({"ships":[
      {"name":"Executor","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"HMS Executor"}]}));
    let ship = inherited.ships.get("Executor").unwrap().read().unwrap();
    assert_eq!(
      ship.get_crew().get_sensors(),
      from_design.get_sensors(),
      "a scenario that names no crew should fly with the design's"
    );
    assert_eq!(ship.get_crew().get_gunnery(0), from_design.get_gunnery(0));
    drop(ship);

    // An explicit crew wins -- including one deliberately less skilled than the
    // design's, which is the case an all-zero default could never express.
    let overridden = parse(json!({"ships":[
      {"name":"Executor","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"HMS Executor",
       "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,
               "engineering_maneuver":0,"sensors":0,"gunnery":[]}}]}));
    let ship = overridden.ships.get("Executor").unwrap().read().unwrap();
    assert_eq!(
      ship.get_crew().get_sensors(),
      0,
      "a stated crew should override the design's, even an untrained one"
    );
  }

  /// A worked example: Tai'ao looking at HMS Executor in Treasure 1.
  ///
  /// The net DM is +9 against an Advanced-stealth hull three TLs above the
  /// observer, which reads as impossible until it is itemised. It is not: the
  /// -9 that stealth and TL are worth is simply outweighed by a target under
  /// 5G thrust, running active sensors, shooting, and hot from its criticals.
  /// Pinned because the arithmetic looked wrong enough to be worth checking.
  #[test_log::test(tokio::test)]
  async fn the_detection_dm_shows_its_working() {
    config_test_ship_templates().await;
    let templates = get_ship_templates_snapshot();
    let mut entities = Entities::default();
    entities.add_ship(
      "Tai'ao".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      templates.get("Tai'ao").expect("Tai'ao design"),
      None,
      None,
    );
    entities.add_ship(
      "HMS Executor".to_string(),
      Vec3::new(2.4e6, 0.0, 0.0),
      Vec3::zero(),
      templates.get("HMS Executor").expect("Executor design"),
      None,
      None,
    );

    {
      let mut executor = entities.ships.get("HMS Executor").unwrap().write().unwrap();
      executor.plan = FlightPlan::acceleration(Vec3::new(5.0 * G, 0.0, 0.0));
      executor.crit_level[0] = 4; // criticals totalling severity 4
      assert_eq!(executor.thrust_in_g(), 5);
    }
    {
      // Pinned here rather than read from the design. This test documents a
      // rules question -- how eight terms sum to a DM that looks impossible --
      // and must not start failing because the scenario was rebalanced.
      let mut taiao = entities.ships.get("Tai'ao").unwrap().write().unwrap();
      taiao.set_crew(serde_json::from_value(json!({"sensors": 4})).unwrap());
    }

    let target = entities.ships.get("HMS Executor").unwrap().read().unwrap().clone();
    let fired = HashSet::from(["HMS Executor".to_string()]);
    let terms = entities.detection_dm_terms("Tai'ao", "HMS Executor", &target, &fired);
    let named: HashMap<&str, i16> = terms.iter().copied().collect();

    // What Tai'ao brings: Military sensors are the baseline grade, and her
    // sensop is skilled.
    assert_eq!(named["sensor grade"], 0);
    assert_eq!(named["sensor skill"], 4);
    // What should be hiding Executor, and very nearly does.
    assert_eq!(named["TL"], 0, "a lower-TL observer gets no bonus, only no penalty");
    assert_eq!(named["stealth"], -6);
    assert_eq!(named["stealth TL"], -3, "DM-1 per TL the stealthed target is above");
    // What gives her away anyway.
    assert_eq!(named["active sensors"], 2);
    assert_eq!(named["thrust"], 5);
    assert_eq!(named["power plant"], 1);
    assert_eq!(named["firing"], 2);
    assert_eq!(named["damage heat"], 4);
    assert_eq!(named["transmitting"], 0, "Executor is not squawking");

    assert_eq!(
      entities.detection_dm("Tai'ao", "HMS Executor", &target, &fired),
      9,
      "the observed net DM"
    );
  }

  /// Every scenario that ships with the repo still loads.
  ///
  /// A scenario that fails to parse does not announce itself -- the loader logs
  /// and moves on, so the only symptom is the file quietly missing from the
  /// picker. That has already happened once, when adding `owner` to `MetaData`
  /// dropped every older file until the field was defaulted. This walks the
  /// whole directory so a new scenario, or a new required field, cannot break
  /// one unnoticed.
  #[test_log::test(tokio::test)]
  async fn every_bundled_scenario_loads() {
    config_test_ship_templates().await;

    let mut checked = 0;
    for entry in fs::read_dir("./scenarios").expect("scenarios directory") {
      let path = entry.expect("readable entry").path();
      if path.extension().is_none_or(|e| e != "json") {
        continue;
      }
      let name = path.display().to_string();
      let bytes = fs::read(&path).unwrap_or_else(|e| panic!("{name}: unreadable: {e}"));
      let entities = Entities::parse_bytes_with_ship_templates(&bytes, &name, get_ship_templates_snapshot())
        .unwrap_or_else(|e| panic!("{name}: failed to load: {e}"));
      assert!(
        !entities.ships.is_empty() || !entities.planets.is_empty(),
        "{name}: loaded but is empty, which usually means the shape is wrong rather than the syntax"
      );
      checked += 1;
    }
    assert!(checked > 0, "no scenarios found to check -- has the directory moved?");
  }

  /// The band sets the to-hit modifier, so a change is worth saying even when
  /// nothing is rolled -- and for an unstealthed target nothing ever is.
  #[test]
  fn a_range_band_change_is_reported_to_whoever_holds_the_contact() {
    let band_lines = |effects: &[EffectMsg]| {
      effects
        .iter()
        .filter_map(|e| match e {
          EffectMsg::Message { content, .. } if content.contains("now at") => Some(content.clone()),
          _ => None,
        })
        .collect::<Vec<_>>()
    };

    // No stealth, so the contact is never re-rolled: any message here is the
    // band change itself and nothing else.
    let mut entities = detection_pair(None, 1.0e6);
    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(5.0e6, 0.0, 0.0));

    let mut rng = SmallRng::seed_from_u64(7);
    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);
    let lines = band_lines(&effects);
    assert!(
      lines.iter().any(|l| l == "Seeker: Quarry now at Medium range (was Short)."),
      "the observer holding the contact should be told the band moved, got {lines:?}"
    );

    // Closing again is reported too: you want to know when your own to-hit
    // improves, not only when a contact is at risk.
    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(1.0e6, 0.0, 0.0));
    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);
    assert!(
      band_lines(&effects)
        .iter()
        .any(|l| l == "Seeker: Quarry now at Short range (was Medium)."),
      "closing the range should be reported as well as opening it"
    );
  }

  /// You cannot judge the range to a ship you cannot see, and saying so anyway
  /// would hand a player the position of ships they have no contact on.
  #[test]
  fn no_range_band_report_without_a_contact() {
    let mut entities = detection_pair(None, 1.0e6);
    for name in ["Seeker", "Quarry"] {
      entities.ships.get(name).unwrap().write().unwrap().contacts.clear();
    }
    // Dark on both sides, so nothing is acquired during the pass either.
    for name in ["Seeker", "Quarry"] {
      entities
        .ships
        .get(name)
        .unwrap()
        .write()
        .unwrap()
        .set_emissions(Some(false), None);
    }
    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(5.0e6, 0.0, 0.0));

    let mut rng = SmallRng::seed_from_u64(7);
    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);
    assert!(
      !effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("now at"))),
      "a ship with no contact should not be told the range, got {effects:?}"
    );
  }

  /// A captain can put the sensop's attention on one particular check.
  /// Detection is free and happens anyway — the boost is what leadership buys,
  /// and it is aimed at a specific pair rather than at the ship in general.
  #[test]
  fn a_captain_can_boost_one_detection_check() {
    use crate::action::BoostTarget;

    let run = |boost: bool| {
      let mut entities = detection_pair(Some(crate::ship::Stealth::Advanced), 1.0e6);
      entities.ships.get("Seeker").unwrap().write().unwrap().contacts.clear();
      let snapshot = entities.ship_deep_copy();
      let mut map = BoostMap::default();
      if boost {
        map.insert(BoostTarget::Detection {
          ship: "Seeker".to_string(),
          target: "Quarry".to_string(),
        });
      }
      let mut rng = SmallRng::seed_from_u64(11);
      entities
        .detection_pass(&snapshot, &HashSet::new(), &map, &mut rng)
        .iter()
        .find_map(|e| match e {
          EffectMsg::Message { content, .. } if content.contains("sensor check on") => Some(content.clone()),
          _ => None,
        })
        .expect("a check should be reported")
    };

    // Same seed, so the dice match and only the DM moves.
    assert_ne!(run(false), run(true), "the boost should change the check");
  }

  /// Every check actually rolled is reported, hit or miss, with the arithmetic
  /// shown. A referee watching a ship stay hidden needs to know whether the
  /// rolls were close or whether it was never findable.
  #[test]
  fn detection_reports_the_roll_and_the_result() {
    let mut entities = detection_pair(Some(crate::ship::Stealth::Advanced), 1.0e6);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(3);

    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

    let check = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { content, .. } if content.contains("sensor check on") => Some(content.clone()),
        _ => None,
      })
      .expect("the attempt should be reported");

    assert!(check.contains("Seeker sensor check on Quarry"), "{check}");
    assert!(check.contains("with roll "), "the roll should be shown: {check}");
    assert!(check.contains("a total of "), "the total should be shown: {check}");
    assert!(check.contains("against 8"), "the target number should be shown: {check}");
    assert!(
      check.contains("no contact") || check.contains("contact."),
      "the outcome should be shown: {check}"
    );
  }

  /// Nothing is reported for a check that was never made — out of range, or the
  /// observer running dark — so the log does not fill with non-events.
  #[test]
  fn no_roll_is_reported_when_no_check_is_made() {
    // Observer dark: it cannot acquire, so there is nothing to roll.
    let mut entities = detection_pair(Some(crate::ship::Stealth::Basic), 1.0e6);
    entities
      .ships
      .get("Seeker")
      .unwrap()
      .write()
      .unwrap()
      .set_emissions(Some(false), None);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(3);

    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

    assert!(
      !effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("sensor check on"))),
      "a ship running dark makes no check, so it should report none: {effects:#?}"
    );
  }

  /// A missile outliving its target must not take the scenario down with it.
  ///
  /// The missile resolves its target by name on every deep copy, and the live
  /// state is deep-copied to answer any request for entities — so one orphan
  /// made every request fail. The client stopped receiving updates and sat on
  /// stale state, still showing the destroyed ship as alive and targetable.
  #[test]
  fn a_missile_outliving_its_target_does_not_break_the_scenario() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship("Shooter".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    entities.add_ship(
      "Doomed".to_string(),
      Vec3::new(1.0e6, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities
      .launch_missile(
        "Shooter",
        "Doomed",
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1),
      )
      .expect("(test) launch");
    assert_eq!(entities.missiles.len(), 1);

    // The target dies with the missile still in flight.
    entities.ships.remove("Doomed");

    let effects = entities.prune_orphaned_missiles();
    assert!(entities.missiles.is_empty(), "the orphan should be dropped");
    assert_eq!(effects.len(), 1, "and reported, not silently vanished");
    assert!(matches!(effects[0], EffectMsg::ExhaustedMissile { .. }));

    entities.deep_copy().expect("entities must still be readable");
  }

  /// Belt and braces: even if an orphan does reach the live state, copying it
  /// heals rather than fails. Losing a missile beats losing the session.
  #[test]
  fn deep_copy_heals_an_orphaned_missile() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship("Shooter".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    entities.add_ship(
      "Doomed".to_string(),
      Vec3::new(1.0e6, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities
      .launch_missile(
        "Shooter",
        "Doomed",
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1),
      )
      .expect("(test) launch");
    entities.ships.remove("Doomed");

    // No pruning first: this is the path that used to return Err and take every
    // request for entities down with it.
    let copy = entities.deep_copy().expect("a stray missile must not break the copy");
    assert!(copy.missiles.is_empty());
    assert!(copy.ships.contains_key("Shooter"));
  }

  /// Renaming a ship has to follow the missiles flying at it, or they are left
  /// pointing at a name nothing answers to.
  #[test]
  fn renaming_a_ship_follows_missiles_targeting_it() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship("Shooter".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    entities.add_ship(
      "Quarry".to_string(),
      Vec3::new(1.0e6, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    entities
      .launch_missile(
        "Shooter",
        "Quarry",
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1),
      )
      .expect("(test) launch");

    entities.rename("Quarry", "Zulu").expect("(test) rename");

    let target = entities.missiles.values().next().unwrap().read().unwrap().target.clone();
    assert_eq!(target, "Zulu", "the missile should follow the rename");
    entities.deep_copy().expect("and the scenario stays readable");
  }

  /// Re-pointing a ship at a different design must not leave the old design's
  /// numbers behind. `fixup_current_values` only ever raises a current value,
  /// so a swap to a smaller hull used to keep the larger one's hull, thrust and
  /// sensors, and the ship went on flying at a rating its design cannot reach.
  #[test]
  fn changing_a_design_resets_the_ship_to_it() {
    let mut entities = Entities::default();
    let big = Arc::new(ShipDesignTemplate {
      name: "Big".to_string(),
      hull: 120,
      maneuver: 4,
      sensors: crate::ship::Sensors::Civilian,
      ..ShipDesignTemplate::default()
    });
    let small = Arc::new(ShipDesignTemplate {
      name: "Small".to_string(),
      hull: 40,
      maneuver: 2,
      sensors: crate::ship::Sensors::Military,
      ..ShipDesignTemplate::default()
    });

    entities.add_ship("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &big, None, None);
    {
      let ship = entities.ships.get("Dragon").unwrap().read().unwrap();
      assert_eq!(ship.current_hull, 120);
      assert_eq!(ship.current_maneuver, 4);
    }

    entities.add_ship("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &small, None, None);

    let ship = entities.ships.get("Dragon").unwrap().read().unwrap();
    assert_eq!(ship.current_hull, 40, "hull should follow the new design down");
    assert_eq!(ship.current_maneuver, 2, "so should thrust");
    assert_eq!(
      ship.current_sensors,
      crate::ship::Sensors::Military,
      "and the sensor suite, which changes what the ship can find"
    );
  }

  /// A two-ship board for the bridge station tests: Dragon, fully fuelled and
  /// clear to jump, with a contact on Quarry.
  fn bridge_board() -> Entities {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate {
      name: "Dragon".to_string(),
      hull: 120,
      maneuver: 4,
      jump: 2,
      fuel: 100,
      computer: 10,
      ..ShipDesignTemplate::default()
    });
    entities.add_ship("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    entities.add_ship(
      "Quarry".to_string(),
      Vec3::new(1.0e6, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let mut dragon = entities.ships.get("Dragon").unwrap().write().unwrap();
    dragon.enable_jump();
    dragon.contacts = vec!["Quarry".to_string()];
    drop(dragon);
    entities
  }

  /// Disabled is out for the round it was hit in and the whole of the next,
  /// then back on its own.
  #[test]
  fn disabled_station_is_out_this_round_and_next() {
    let entities = bridge_board();
    let dragon = entities.ships.get("Dragon").unwrap();
    {
      let mut ship = dragon.write().unwrap();
      ship.set_pilot_actions(Some(2), Some(true)).expect("(test) evade");
      ship.disable_station(BridgeStation::Pilot);
      assert!(!ship.can_accelerate());
      assert_eq!(ship.get_dodge_thrust(), 0, "no evasion without a pilot");
      assert!(!ship.get_assist_gunners(), "and no assisting the gunners");
    }

    entities.tick_bridge_stations();
    assert!(!dragon.read().unwrap().can_accelerate(), "still out for the next round");

    entities.tick_bridge_stations();
    let ship = dragon.read().unwrap();
    assert!(ship.can_accelerate(), "back after that");
    assert_eq!(ship.get_dodge_thrust(), 2, "with the pilot's orders intact");
  }

  /// A ship without its pilot coasts, and keeps the plan for when it has one.
  #[test]
  fn ship_without_a_pilot_coasts() {
    let mut entities = bridge_board();
    let dragon = entities.ships.get("Dragon").unwrap();
    {
      let mut ship = dragon.write().unwrap();
      ship
        .set_flight_plan(&FlightPlan::new((Vec3::new(0.0, 2.0 * G, 0.0), 10_000).into(), None))
        .expect("(test) plan");
      ship.destroy_station(BridgeStation::Pilot);
    }
    entities.ships.get_mut("Dragon").unwrap().write().unwrap().update();

    let ship = entities.ships.get("Dragon").unwrap().read().unwrap();
    assert_eq!(ship.get_velocity(), Vec3::zero(), "no burn");
    assert!(!ship.plan.empty(), "the plan waits");
  }

  /// A destroyed station stays out until an engineer repairs the bridge.
  #[test]
  fn bridge_repair_brings_a_destroyed_station_back() {
    let mut entities = bridge_board();
    {
      let mut ship = entities.ships.get("Dragon").unwrap().write().unwrap();
      ship.bridge_hit_destroy(1, BridgeStation::Computer);
      ship.bridge_hit_bandwidth(1, 0);
      // Low, so a 12 on the check clears the crit level's penalty.
      ship.crit_level[ShipSystem::Bridge as usize] = 1;
      ship.tick_bridge_stations();
      assert!(!ship.can_jump(), "destroyed does not tick away");
    }

    // Every die a 6, so the repair succeeds.
    let mut rng = StepRng::new(5, 0);
    let effects = entities.engineer_actions(
      &[(
        "Dragon".to_string(),
        vec![ShipAction::Repair {
          system: ShipSystem::Bridge,
        }],
      )],
      &BoostMap::default(),
      &mut rng,
    );

    let ship = entities.ships.get("Dragon").unwrap().read().unwrap();
    assert!(ship.station_working(BridgeStation::Computer));
    assert_eq!(ship.current_computer, 10, "at full Bandwidth");
    assert!(ship.can_jump());
    assert!(
      format!("{effects:?}").contains("Restored: computer Bandwidth back to 10, computer station working again."),
      "{effects:?}"
    );
    assert!(
      format!("{effects:?}").contains("with roll 12 and DM -1 (damage -1) for a total of 11 against 8"),
      "the check should be spelled out: {effects:?}"
    );
  }

  /// Each Bridge repair takes off the damage done at the severity it leaves,
  /// most recent first.
  #[test]
  fn bridge_repairs_undo_damage_newest_first() {
    let entities = bridge_board();
    let mut ship = entities.ships.get("Dragon").unwrap().write().unwrap();
    ship.bridge_hit_bandwidth(3, 5);
    ship.bridge_hit_destroy(4, BridgeStation::Pilot);
    ship.bridge_hit_destroy(5, BridgeStation::Computer);
    ship.bridge_hit_bandwidth(5, 0);

    // 5 -> 4: the computer comes back, at what it had before the level 5 hit.
    ship.undo_bridge_damage(4);
    assert!(ship.station_working(BridgeStation::Computer));
    assert_eq!(ship.current_computer, 5);
    assert!(!ship.station_working(BridgeStation::Pilot), "the level 4 damage is still there");

    // 4 -> 3: the pilot.
    ship.undo_bridge_damage(3);
    assert!(ship.station_working(BridgeStation::Pilot));
    assert_eq!(ship.current_computer, 5, "Bandwidth still halved");

    // 3 -> 2: the Bandwidth.
    assert_eq!(ship.undo_bridge_damage(2), vec!["computer Bandwidth back to 10".to_string()]);
    assert!(ship.bridge_damage.is_empty());
  }

  /// Undoing a destroyed station leaves it destroyed if an earlier hit had
  /// already destroyed it.
  #[test]
  fn undoing_a_second_destroy_keeps_the_first() {
    let entities = bridge_board();
    let mut ship = entities.ships.get("Dragon").unwrap().write().unwrap();
    ship.bridge_hit_destroy(4, BridgeStation::Pilot);
    ship.bridge_hit_destroy(6, BridgeStation::Pilot);

    ship.undo_bridge_damage(5);
    assert!(!ship.station_working(BridgeStation::Pilot));
    ship.undo_bridge_damage(3);
    assert!(ship.station_working(BridgeStation::Pilot));
  }

  /// Weapons, sensors and the bridge are repaired with Mechanic, not
  /// Engineering.
  #[test]
  fn mechanic_repairs_sensors() {
    let mut entities = bridge_board();
    let mut crew = Crew::new();
    crew.set_skill(Skills::Mechanic, 3);
    crew.set_skill(Skills::EngineeringManeuver, 1);
    {
      let mut ship = entities.ships.get("Dragon").unwrap().write().unwrap();
      ship.set_crew(crew);
      ship.crit_level[ShipSystem::Sensors as usize] = 1;
    }

    let mut rng = StepRng::new(0, 0);
    let effects = entities.engineer_actions(
      &[(
        "Dragon".to_string(),
        vec![ShipAction::Repair {
          system: ShipSystem::Sensors,
        }],
      )],
      &BoostMap::default(),
      &mut rng,
    );
    assert!(
      format!("{effects:?}").contains("with roll 2 and DM +2 (mechanic +3, damage -1)"),
      "{effects:?}"
    );
  }

  /// No astrogation, no jump -- and the engineer is told why.
  #[test]
  fn jump_needs_astrogation() {
    let mut entities = bridge_board();
    entities
      .ships
      .get("Dragon")
      .unwrap()
      .write()
      .unwrap()
      .disable_station(BridgeStation::Astrogation);

    let mut rng = StepRng::new(5, 0);
    let effects = entities.engineer_actions(
      &[("Dragon".to_string(), vec![ShipAction::Jump])],
      &BoostMap::default(),
      &mut rng,
    );

    assert!(entities.ships.contains_key("Dragon"), "Dragon should still be here");
    assert!(format!("{effects:?}").contains("astrogation station is out"), "{effects:?}");
  }

  /// Sensor and fire actions are refused with a message when their station is
  /// out, and nothing else happens.
  #[test]
  fn sensor_and_fire_actions_need_their_stations() {
    let mut entities = bridge_board();
    {
      let mut ship = entities.ships.get("Dragon").unwrap().write().unwrap();
      ship.disable_station(BridgeStation::Sensors);
      ship.destroy_station(BridgeStation::FireControl);
    }
    let mut rng = StepRng::new(5, 0);

    let effects = entities.sensor_actions(
      &[(
        "Dragon".to_string(),
        vec![ShipAction::SensorLock {
          target: "Quarry".to_string(),
        }],
      )],
      &BoostMap::default(),
      &mut rng,
    );
    assert!(format!("{effects:?}").contains("sensors station is out"), "{effects:?}");
    assert!(entities.ships.get("Dragon").unwrap().read().unwrap().sensor_locks.is_empty());

    let snapshot = entities.ship_deep_copy();
    let effects = entities.fire_actions(
      &[(
        "Dragon".to_string(),
        vec![ShipAction::FireAction {
          weapon_id: 0,
          target: "Quarry".to_string(),
          called_shot_system: None,
          firing_kind: None,
        }],
      )],
      &[],
      &snapshot,
      &BoostMap::default(),
      &mut rng,
    );
    assert_eq!(effects.len(), 1, "only the refusal: {effects:?}");
    assert!(format!("{effects:?}").contains("fire control station is out"), "{effects:?}");
  }

  /// A critically failed overload damages the drive the same way a hit does.
  /// It used to raise the drive's crit level and nothing else, so the log said
  /// "Drive damaged" while thrust stayed where it was.
  #[test]
  fn critically_failed_overload_damages_the_drive() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate {
      name: "Dragon".to_string(),
      hull: 120,
      maneuver: 4,
      ..ShipDesignTemplate::default()
    });
    entities.add_ship("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    // Every die a 1: a check of 2, a critical failure against 10.
    let mut rng = StepRng::new(0, 0);
    let effects = entities.engineer_actions(
      &[("Dragon".to_string(), vec![ShipAction::OverloadDrive])],
      &BoostMap::default(),
      &mut rng,
    );

    let ship = entities.ships.get("Dragon").unwrap().read().unwrap();
    assert_eq!(ship.crit_level[ShipSystem::Maneuver as usize], 1);
    assert_eq!(ship.current_maneuver, 3, "a level 1 drive crit costs a point of thrust");
    assert!(
      effects.iter().any(|effect| matches!(
        effect,
        EffectMsg::Message { category: MessageCategory::Critical, content, .. } if content.contains("maneuver critical hit")
      )),
      "the crit should be reported like any other: {effects:?}"
    );
  }

  /// A squadron knows its own formation. Team-mates never have to find each
  /// other, even when both are stealthed and running silent.
  #[test]
  fn teammates_always_detect_each_other() {
    use crate::ship::{Stealth, Team};
    let mut entities = Entities::default();
    let hidden = Arc::new(ShipDesignTemplate {
      stealth: Some(Stealth::Advanced),
      ..ShipDesignTemplate::default()
    });
    for name in ["Flayer", "Thrasher"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &hidden, None, None);
    }
    // Both stealthed, so neither is seeded as a contact for the other.
    assert!(!holds_contact(&entities, "Flayer", "Thrasher"), "not seeded");

    for name in ["Flayer", "Thrasher"] {
      entities.ships.get(name).unwrap().write().unwrap().team = Some(Team::Green);
      entities
        .ships
        .get(name)
        .unwrap()
        .write()
        .unwrap()
        .set_emissions(Some(false), Some(false));
    }

    assert!(
      entities.has_contact("Flayer", "Thrasher"),
      "team-mates know where each other are"
    );
    assert!(entities.has_contact("Thrasher", "Flayer"));
  }

  /// Being on a team says nothing about ships that are not on it.
  #[test]
  fn a_team_does_not_reveal_outsiders() {
    use crate::ship::{Stealth, Team};
    let mut entities = Entities::default();
    let hidden = Arc::new(ShipDesignTemplate {
      stealth: Some(Stealth::Advanced),
      ..ShipDesignTemplate::default()
    });
    for name in ["Flayer", "Thrasher", "Stranger"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &hidden, None, None);
    }
    for name in ["Flayer", "Thrasher"] {
      entities.ships.get(name).unwrap().write().unwrap().team = Some(Team::Green);
    }
    entities.ships.get("Stranger").unwrap().write().unwrap().team = Some(Team::Red);

    assert!(entities.has_contact("Flayer", "Thrasher"));
    assert!(!entities.has_contact("Flayer", "Stranger"), "the other side is still hidden");
    assert!(!entities.has_contact("Stranger", "Flayer"));
  }

  /// Jamming stops communication, and a hand-off is communication. Jamming the
  /// picket cuts the whole squadron off from what it can see.
  #[test]
  fn jamming_the_host_breaks_the_handoff() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities.ships.get("Picket").unwrap().write().unwrap().comms_jammed = true;

    entities.sensor_handoff_pass();

    assert!(!holds_contact(&entities, "Mate", "Bogey"), "a jammed picket cannot share");
  }

  /// Jamming one recipient cuts off that ship alone, not the rest of the team.
  #[test]
  fn jamming_a_recipient_cuts_off_only_that_ship() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    // A second quiet team-mate, not jammed.
    let design = Arc::new(ShipDesignTemplate::default());
    entities.add_ship(
      "Wingman".to_string(),
      Vec3::new(1.5e6, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    {
      let mut wingman = entities.ships.get("Wingman").unwrap().write().unwrap();
      wingman.team = Some(crate::ship::Team::Red);
    }
    // add_ship re-seeds every ship's contacts, so blind the two quiet ships
    // again afterwards: the point of the test is what the hand-off gives them.
    for quiet in ["Mate", "Wingman"] {
      entities.ships.get(quiet).unwrap().write().unwrap().contacts.clear();
    }
    entities.ships.get("Mate").unwrap().write().unwrap().comms_jammed = true;

    entities.sensor_handoff_pass();

    assert!(!holds_contact(&entities, "Mate", "Bogey"), "the jammed ship receives nothing");
    assert!(holds_contact(&entities, "Wingman", "Bogey"), "its team-mate is unaffected");
  }

  /// A jam lasts the round it was made in and no longer.
  #[test]
  fn comms_jamming_is_cleared_at_the_end_of_the_round() {
    let entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities.ships.get("Picket").unwrap().write().unwrap().comms_jammed = true;

    entities.clear_comms_jamming();

    assert!(!entities.ships.get("Picket").unwrap().read().unwrap().comms_jammed);
  }

  /// A contact once shared belongs to the receiver outright. Losing the picket,
  /// or being jammed afterwards, does not take it back.
  #[test]
  fn shared_contacts_survive_losing_the_link() {
    let mut entities = handoff_pair(Some(crate::ship::Team::Red), Some(crate::ship::Team::Red));
    entities.sensor_handoff_pass();
    assert!(holds_contact(&entities, "Mate", "Bogey"), "shared in the first place");

    // The picket is destroyed and its references pruned.
    entities.ships.remove("Picket");
    entities.prune_ship_references();

    assert!(
      holds_contact(&entities, "Mate", "Bogey"),
      "an inherited contact is the receiver's own; it does not depend on the host"
    );
  }

  /// Sharing means broadcasting: hand-off forces transmitting on and holds it
  /// there, so a ship cannot pass contacts while running silent.
  #[test]
  fn handoff_forces_transmitting() {
    let design = Arc::new(ShipDesignTemplate::default());
    let mut ship = Ship::new("Picket".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    assert!(!ship.transmitting, "ships start silent");

    ship.set_handoff_sensors(true);
    assert!(ship.transmitting, "turning hand-off on lights the ship up");

    // And it cannot be switched back off underneath the hand-off.
    ship.set_emissions(None, Some(false));
    assert!(ship.transmitting, "transmitting is held on while hand-off is on");

    // Turning hand-off off releases it, but does not go quiet on its own.
    ship.set_handoff_sensors(false);
    assert!(ship.transmitting, "releasing the hold should not surprise the crew");
    ship.set_emissions(None, Some(false));
    assert!(!ship.transmitting, "now it can go quiet");
  }

  /// A stealthed ship that shoots from concealment can be found.
  ///
  /// Before the two High Guard tables were unified, firing only counted when
  /// reacquiring a ship whose contact had already been lost, so a stealth hull
  /// could run dark and fire every round at a target it already held with the
  /// hunter having no chance whatsoever -- not merely a poor one, but a DM low
  /// enough that the best possible roll could not reach 8. Firing now counts
  /// towards acquisition too, and stacks with the rest.
  #[test]
  fn firing_from_concealment_can_be_detected() {
    let quiet = {
      let entities = detection_pair(Some(crate::ship::Stealth::Advanced), 1.0e6);
      let target = entities.ships.get("Quarry").unwrap().read().unwrap().clone();
      entities.detection_dm("Seeker", "Quarry", &target, &HashSet::new())
    };

    let firing = {
      let entities = detection_pair(Some(crate::ship::Stealth::Advanced), 1.0e6);
      let target = entities.ships.get("Quarry").unwrap().read().unwrap().clone();
      let fired: HashSet<String> = ["Quarry".to_string()].into_iter().collect();
      entities.detection_dm("Seeker", "Quarry", &target, &fired)
    };

    assert_eq!(
      firing,
      quiet + 2,
      "firing is worth DM+2 towards acquisition, not just reacquisition"
    );
  }

  /// The emission rows stack: doing two loud things is worse than one.
  #[test]
  fn emission_rows_stack() {
    let entities = detection_pair(Some(crate::ship::Stealth::Basic), 1.0e6);
    let fired: HashSet<String> = ["Quarry".to_string()].into_iter().collect();

    let dark_quiet = {
      let mut t = entities.ships.get("Quarry").unwrap().read().unwrap().clone();
      t.set_emissions(Some(false), None);
      entities.detection_dm("Seeker", "Quarry", &t, &HashSet::new())
    };
    let dark_firing = {
      let mut t = entities.ships.get("Quarry").unwrap().read().unwrap().clone();
      t.set_emissions(Some(false), None);
      entities.detection_dm("Seeker", "Quarry", &t, &fired)
    };
    let lit_firing = {
      let t = entities.ships.get("Quarry").unwrap().read().unwrap().clone();
      entities.detection_dm("Seeker", "Quarry", &t, &fired)
    };

    assert_eq!(dark_firing, dark_quiet + 2, "firing alone is +2");
    assert_eq!(lit_firing, dark_firing + 2, "active sensors stack on top of firing");
    assert_eq!(lit_firing, dark_quiet + 4, "both together are +4, not +2");
  }

  /// Beyond Distant everything is an undifferentiated blip, so contact cannot
  /// be held at all (High Guard p. 76, and decision E of the design).
  #[test]
  fn contact_is_lost_beyond_distant() {
    // Start in contact at Short range, then open the range past the 50,000 km
    // edge of Distant.
    let mut entities = detection_pair(None, 1.0e6);
    assert!(holds_contact(&entities, "Seeker", "Quarry"), "seeded at load");

    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(6.0e7, 0.0, 0.0));

    let mut rng = SmallRng::seed_from_u64(1);
    let effects = entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

    assert!(!holds_contact(&entities, "Seeker", "Quarry"), "too far to hold contact");
    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("lost sensor contact"))),
      "the loss should be reported"
    );
  }

  /// The reacquisition check fires only when the range opens, and only for a
  /// stealthed target. An ordinary hull is never lost this way.
  #[test]
  fn only_stealth_is_lost_when_the_range_opens() {
    // Start at Short (under 1,250 km) and end at Medium.
    let start = 1.0e6;
    let end = 5.0e6;

    for (stealth, should_keep) in [(None, true), (Some(crate::ship::Stealth::Advanced), false)] {
      let mut entities = detection_pair(stealth, start);
      // Give the seeker the contact either way, so the only variable is stealth.
      {
        let mut seeker = entities.ships.get("Seeker").unwrap().write().unwrap();
        if !seeker.contacts.iter().any(|n| n == "Quarry") {
          seeker.contacts.push("Quarry".to_string());
        }
      }
      let snapshot = entities.ship_deep_copy();
      // Now open the range to the next band.
      entities
        .ships
        .get("Quarry")
        .unwrap()
        .write()
        .unwrap()
        .set_position(Vec3::new(end, 0.0, 0.0));

      // A roll of 2 fails any check, so a stealthed target is certainly lost.
      let mut rng = StepRng::new(0, 0);
      entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

      assert_eq!(
        holds_contact(&entities, "Seeker", "Quarry"),
        should_keep,
        "stealth={stealth:?} should {} contact when the range opens",
        if should_keep { "keep" } else { "lose" }
      );
    }
  }

  /// Closing the range is not a trigger: only an opening one is.
  #[test]
  fn closing_the_range_never_costs_contact() {
    let mut entities = detection_pair(Some(crate::ship::Stealth::Advanced), 5.0e6);
    {
      let mut seeker = entities.ships.get("Seeker").unwrap().write().unwrap();
      seeker.contacts.push("Quarry".to_string());
    }
    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(1.0e6, 0.0, 0.0));

    let mut rng = StepRng::new(0, 0);
    entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

    assert!(
      holds_contact(&entities, "Seeker", "Quarry"),
      "closing the range should never trigger a reacquisition check"
    );
  }

  /// Losing contact takes the sensor lock with it: a lock cannot outlive the
  /// contact it was built on.
  #[test]
  fn losing_contact_drops_the_lock() {
    let mut entities = detection_pair(None, 1.0e6);
    {
      let mut seeker = entities.ships.get("Seeker").unwrap().write().unwrap();
      seeker.sensor_locks.push("Quarry".to_string());
    }
    let snapshot = entities.ship_deep_copy();
    entities
      .ships
      .get("Quarry")
      .unwrap()
      .write()
      .unwrap()
      .set_position(Vec3::new(6.0e7, 0.0, 0.0));
    let mut rng = SmallRng::seed_from_u64(1);
    entities.detection_pass(&snapshot, &HashSet::new(), &BoostMap::default(), &mut rng);

    let seeker = entities.ships.get("Seeker").unwrap().read().unwrap();
    assert!(seeker.contacts.is_empty());
    assert!(seeker.sensor_locks.is_empty(), "the lock should go with the contact");
  }

  /// Going dark drops locks but keeps contacts. High Guard p. 77 keeps
  /// detection "maintained under most circumstances", while a lock is
  /// deliberate illumination that a quiet ship is not performing.
  #[test]
  fn going_dark_drops_locks_but_keeps_contacts() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    for name in ["Alpha", "Bravo"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    }
    entities.establish_initial_contacts();
    let alpha = entities.ships.get("Alpha").unwrap();
    alpha.write().unwrap().sensor_locks.push("Bravo".to_string());

    let dropped = alpha.write().unwrap().set_emissions(Some(false), None);

    assert!(dropped, "going dark should report that locks were dropped");
    let alpha = alpha.read().unwrap();
    assert!(!alpha.active_sensors);
    assert!(alpha.sensor_locks.is_empty(), "locks should not survive going dark");
    assert_eq!(alpha.contacts, vec!["Bravo".to_string()], "contacts should survive going dark");
  }

  /// Only the transition to dark drops locks; coming back up, or setting the
  /// state it already had, leaves them alone.
  #[test]
  fn only_going_dark_drops_locks() {
    let design = Arc::new(ShipDesignTemplate::default());
    let mut ship = Ship::new("Alpha".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    ship.sensor_locks.push("Bravo".to_string());

    assert!(
      !ship.set_emissions(Some(true), None),
      "already-lit sensors should not drop locks"
    );
    assert_eq!(ship.sensor_locks.len(), 1);

    assert!(ship.set_emissions(Some(false), None), "going dark should drop them");
    assert!(ship.sensor_locks.is_empty());
    // Already dark: nothing left to drop, so no second report.
    assert!(!ship.set_emissions(Some(false), None));
  }

  /// A ship running dark cannot take a new lock, contact or no contact.
  #[test]
  fn a_dark_ship_cannot_sensor_lock() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(5, 0);
    entities.ships.insert(
      "attacker".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("attacker", 2))),
    );
    entities.ships.insert(
      "target".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("target", 2))),
    );
    entities.establish_initial_contacts();
    entities
      .ships
      .get("attacker")
      .unwrap()
      .write()
      .unwrap()
      .set_emissions(Some(false), None);

    let actions = vec![(
      "attacker".to_string(),
      vec![ShipAction::SensorLock {
        target: "target".to_string(),
      }],
    )];
    let effects = entities.sensor_actions(&actions, &BoostMap::default(), &mut rng);

    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("running dark"))),
      "expected a running-dark refusal, got {effects:?}"
    );
    assert!(
      entities.ships.get("attacker").unwrap().read().unwrap().sensor_locks.is_empty(),
      "a dark ship should not acquire a lock"
    );
  }

  /// A loaded scenario opens with everyone aware of everyone, and the list is
  /// sorted so the wire payload does not inherit the ship map's ordering.
  #[test]
  fn initial_contacts_are_mutual_and_sorted() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    for name in ["Charlie", "Alpha", "Bravo"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    }
    entities.establish_initial_contacts();

    for (name, ship) in &entities.ships {
      let contacts = &ship.read().unwrap().contacts;
      let expected: Vec<String> = ["Alpha", "Bravo", "Charlie"]
        .iter()
        .filter(|other| *other != name)
        .map(ToString::to_string)
        .collect();
      assert_eq!(*contacts, expected, "{name} should detect the other two, in sorted order");
    }
  }

  /// Losing a ship must not leave everyone else tracking a name that is gone.
  #[test]
  fn pruning_drops_references_to_departed_ships() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    for name in ["Alpha", "Bravo"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    }
    entities
      .ships
      .get("Alpha")
      .unwrap()
      .write()
      .unwrap()
      .sensor_locks
      .push("Bravo".to_string());

    entities.ships.remove("Bravo");
    entities.prune_ship_references();

    let alpha = entities.ships.get("Alpha").unwrap().read().unwrap();
    assert!(alpha.contacts.is_empty(), "contact on a departed ship should be dropped");
    assert!(alpha.sensor_locks.is_empty(), "lock on a departed ship should be dropped");
  }

  /// Renaming used to leave watchers pointing at the old name, silently losing
  /// both the contact and the lock.
  #[test]
  fn renaming_a_ship_follows_contacts_and_locks() {
    let mut entities = Entities::default();
    let design = Arc::new(ShipDesignTemplate::default());
    for name in ["Alpha", "Bravo"] {
      entities.add_ship(name.to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    }
    // This test is about the rename following references, so give it some.
    entities.establish_initial_contacts();
    entities
      .ships
      .get("Alpha")
      .unwrap()
      .write()
      .unwrap()
      .sensor_locks
      .push("Bravo".to_string());

    entities.rename("Bravo", "Zulu").expect("(test) rename should succeed");

    let alpha = entities.ships.get("Alpha").unwrap().read().unwrap();
    assert_eq!(alpha.contacts, vec!["Zulu".to_string()], "contact should follow the rename");
    assert_eq!(alpha.sensor_locks, vec!["Zulu".to_string()], "lock should follow the rename");
  }

  /// Nothing can be done to a ship that is not detected.
  #[test]
  fn actions_against_an_undetected_ship_are_refused() {
    let mut entities = Entities::default();
    let mut rng = StepRng::new(5, 0);
    entities.ships.insert(
      "attacker".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("attacker", 2))),
    );
    entities.ships.insert(
      "target".to_string(),
      Arc::new(RwLock::new(create_test_ship_sensors("target", 2))),
    );
    // Deliberately no contacts: the attacker cannot see the target at all.

    let boost_map = BoostMap::default();
    for (action, wording) in [
      (
        ShipAction::SensorLock {
          target: "target".to_string(),
        },
        "cannot lock onto",
      ),
      (
        ShipAction::JamComms {
          target: "target".to_string(),
        },
        "cannot jam",
      ),
    ] {
      let actions = vec![("attacker".to_string(), vec![action])];
      let effects = entities.sensor_actions(&actions, &boost_map, &mut rng);
      assert!(
        effects
          .iter()
          .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains(wording))),
        "expected a refusal containing {wording:?}, got {effects:?}"
      );
    }

    assert!(
      entities.ships.get("attacker").unwrap().read().unwrap().sensor_locks.is_empty(),
      "no lock should have been established on an undetected ship"
    );
  }

  #[test_log::test(tokio::test)]
  async fn test_sensor_detection_modifiers() {
    // Designs used here: Free Trader/Far Trader/Light Fighter are TL12 with no
    // stealth, Buccaneer is TL15 with no stealth, Harrier is TL15 with Advanced
    // stealth (DM-6).
    let test_cases = [
      // (observer_design, target_design, skill(ignored), skill(ignored), expected_modifier)
      // Same TL, no stealth: nothing applies.
      ("Free Trader", "Far Trader", 0, 0, 0),
      // Observer is LOWER TL with a plain target: no bonus, and no penalty
      // either - the TL penalty is a stealth-only rule.
      ("Light Fighter", "Buccaneer", 3, 0, 0),
      // High Guard's own worked example: "A TL15 ship receives DM+3 to detect a
      // TL12 ship." This returned 0 before the TL bonus was split out.
      ("Harrier", "Free Trader", 2, 0, 3),
      // TL12 observer vs TL15 Advanced-stealth target: -6 grade, -3 for the
      // three TLs the target has on it.
      ("Free Trader", "Harrier", 0, 0, -9),
      // Same TL as the stealthed target, so only the grade applies.
      ("Buccaneer", "Harrier", 0, 0, -6),
    ];

    for (observer_design, target_design, observer_skill, target_skill, expected) in test_cases {
      let entities = setup_sensor_test_ships(
        "attacker",
        observer_skill,
        "target",
        target_skill,
        observer_design,
        target_design,
      )
      .await;

      let result = entities.sensor_detection_modifiers("attacker", "target");
      assert_eq!(
        result, expected,
        "Failed with observer_design={observer_design}, target_design={target_design}, expected={expected}",
      );
    }
  }

  #[tokio::test]
  async fn test_sensor_quality_modifiers() {
    let test_cases = [
      // (attack_design, attack_skill, expected_modifier)
      ("Free Trader", 0, -2),
      // Light Fighter carries Improved sensors per High Guard (p137); it was
      // Military in the older Core Rulebook stats, which scored 3 here.
      ("Light Fighter", 3, 4),
      ("Buccaneer", 0, 1),
      ("Harrier", 2, 4),
    ];

    for (attack_design, attack_skill, expected) in test_cases {
      let entities =
        setup_sensor_test_ships("test_ship", attack_skill, "ignore", 0, attack_design, "Free Trader").await;

      let result = entities.sensor_quality_modifiers("test_ship");
      assert_eq!(
        result, expected,
        "Failed with attack_design={attack_design}, attack_skill={attack_skill}, expected={expected}",
      );
    }
  }

  #[test]
  #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
  fn test_sensor_quality_modifiers_invalid_ship() {
    let entities = Entities::new();
    entities.sensor_quality_modifiers("nonexistent_ship");
  }

  #[test]
  #[should_panic(expected = "called `Option::unwrap()` on a `None` value")]
  fn test_sensor_detection_modifiers_invalid_ships() {
    let entities = Entities::new();
    entities.sensor_detection_modifiers("nonexistent_attacker", "nonexistent_target");
  }
}

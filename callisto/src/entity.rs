use cgmath::{InnerSpace, Vector3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::payloads::EffectMsg;
use rand::seq::SliceRandom;
use rand::RngCore;

use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::action::{ShipAction, ShipActionList};
use crate::combat::{attack, create_sand_counts, do_fire_actions, roll_dice};
use crate::crew::Crew;
use crate::missile::Missile;
use crate::planet::Planet;
use crate::read_local_or_cloud_file;
use crate::rules_tables::{countermeasures_mod, stealth_mod, SENSOR_QUALITY_MOD};
use crate::ship::{FlightPlan, Ship, ShipDesignTemplate};
use crate::ship::{Weapon, WeaponMount, WeaponType};

#[allow(unused_imports)]
use crate::{debug, error, info, warn};

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

impl Entities {
  #[must_use]
  pub fn new() -> Self {
    Entities {
      ships: HashMap::new(),
      missiles: HashMap::new(),
      planets: HashMap::new(),
      next_missile_id: 0,
      actions: vec![],
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

  /// Do a deep copy from one `Entities` to another.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship, missile, or planet.
  pub fn deep_copy(&self, dest: &mut Self) {
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

    dest.fixup_pointers().unwrap();
    dest.reset_gravity_wells();
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
    info!("Load scenario file \"{}\".", file_name);

    let mut entities: Entities = serde_json::from_slice(&read_local_or_cloud_file(file_name).await?)?;

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

  /// Add a ship to the entities.
  ///
  /// # Arguments
  /// * `name` - The name of the ship.
  /// * `position` - The position of the ship.
  /// * `velocity` - The velocity of the ship.
  /// * `design` - The design of the ship.
  /// * `crew` - The crew of the ship.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read an existing ship that is being modified.
  pub fn add_ship(
    &mut self, name: String, position: Vec3, velocity: Vec3, design: &Arc<ShipDesignTemplate>, crew: Option<Crew>,
  ) {
    if let Some(ship) = self.ships.get(&name) {
      // If the ship already exists, then just update appropriate values.
      let mut ship = ship.write().unwrap();
      ship.set_position(position);
      ship.set_velocity(velocity);
      ship.design = design.clone();
      ship.crew = crew.unwrap_or_default();
      ship.fixup_current_values();
    } else {
      // Create a new ship and add it to the ship table
      let ship = Arc::new(RwLock::new(Ship::new(name.clone(), position, velocity, design, crew)));
      self.ships.insert(name, ship);
    }
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
  pub fn add_planet(
    &mut self, name: String, position: Vec3, color: String, primary: Option<String>, radius: f64, mass: f64,
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

      (&Some(primary.clone()), primary.read().unwrap().dependency + 1)
    } else {
      (&None, 0)
    };

    // A safety check to ensure we never have a pointer without a name of a primary or vis versa.
    if primary_ptr.is_some() ^ primary.is_some() {
      return Err(format!(
        "Planet {name} has a primary pointer but no primary name or vice versa."
      ));
    }

    let entity = Planet::new(name.clone(), position, color, radius, mass, primary, primary_ptr, dependency);

    debug!("Added planet with fixed gravity wells {:?}", entity);
    self.planets.insert(name, Arc::new(RwLock::new(entity)));
    Ok(())
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
  pub fn launch_missile(&mut self, source: &str, target: &str) -> Result<(), String> {
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
    &mut self, fire_actions: &[(String, Vec<ShipAction>)], ship_snapshot: &HashMap<String, Ship>, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    // Create a snapshot of all the sand capabilities of each ship.
    let mut sand_counts = create_sand_counts(ship_snapshot);

    let effects = fire_actions
      .iter()
      .flat_map(|(attacker, actions)| {
        let attack_ship = match ship_snapshot.get(attacker) {
          None => {
            warn!("Cannot find attacker {} for fire actions.", attacker);
            return vec![];
          }
          Some(ship) => ship,
        };

        let (missiles, effects) = do_fire_actions(attack_ship, &mut self.ships, &mut sand_counts, actions, rng);
        for missile in missiles {
          if let Err(msg) = self.launch_missile(&missile.source, &missile.target) {
            warn!("Could not launch missile: {}", msg);
          }
        }
        effects
      })
      .collect();
    effects
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
  pub fn update_all(&mut self, ship_snapshot: &HashMap<String, Ship>, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
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
        let missile_source = match ship_snapshot.get(&missile.source) {
          None => {
            warn!(
              "(Entity.update_all) Cannot find source {} for missile. It may have been destroyed.",
              &missile.source
            );
            return None;
          }
          Some(ship) => ship,
        };

        // We use UpdateAction vs just returning the effect so that the call to attack() stays at this level rather than
        // being embedded in the missile update code.  Also enables elimination of missiles.
        match update? {
          UpdateAction::ShipImpact { ship, missile } => {
            // When a missile impacts fake it as an attack by a single turret missile.
            const FAKE_MISSILE_LAUNCHER: Weapon = Weapon {
              kind: WeaponType::Missile,
              mount: WeaponMount::Turret(1),
            };
            info!("(Entity.update_all) Missile impact on {} by missile {}.", ship, missile);
            let target = self.ships.get(&ship).map_or_else(
              || {
                warn!("Cannot find target {} for missile. It may have been destroyed.", ship);
                None
              },
              |ship| Some(ship.clone()),
            );

            if let Some(target) = target {
              let mut target = target.write().unwrap();
              let effects = attack(
                0,
                0,
                missile_source,
                &mut target,
                &FAKE_MISSILE_LAUNCHER,
                // Missiles cannot do called shots
                None,
                rng,
              );
              cleanup_missile_list.push(missile);

              Some(effects)
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
            assert!(name == missile_name);
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
          let name = ship.get_name();
          let pos = ship.get_position();

          match update? {
            UpdateAction::ShipDestroyed => {
              debug!("(Entity.update_all) Ship {} destroyed at position {:?}.", name, pos);
              cleanup_ships_list.push(name.to_string());
              Some(vec![
                EffectMsg::ShipDestroyed { position: pos },
                EffectMsg::Message {
                  content: format!("{name} destroyed."),
                },
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
  pub fn sensor_actions(&mut self, actions: &[(String, Vec<ShipAction>)], rng: &mut dyn RngCore) -> Vec<EffectMsg> {
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
      // Process the actions for each ship.
      for action in actions {
        effects.append(&mut match action {
          ShipAction::JamMissiles => self.jam_missiles(ship_name, rng),
          ShipAction::BreakSensorLock { target } => {
            self.break_sensor_lock(ship_name, target, &reverse_sensor_locks, rng)
          }

          ShipAction::SensorLock { target } => {
            if !self.ships.contains_key(target) {
              warn!("(Entity.do_sensor_actions) Cannot find target {} for sensor lock.", target);
              return Vec::default();
            }
            self.sensor_lock(ship_name, target, rng)
          }
          ShipAction::JamComms { target } => {
            if !self.ships.contains_key(target) {
              warn!("(Entity.do_sensor_actions) Cannot find target {} for jamming comms.", target);
              return Vec::default();
            }
            self.jam_comms(ship_name, target, rng)
          }
          ShipAction::FireAction { .. } | ShipAction::DeleteFireAction { .. } | ShipAction::Jump => {
            error!("(Entity.do_sensor_actions) Unexpected sensor action {action:?}");
            Vec::default()
          }
        });
      }
    }
    effects
  }

  fn sensor_stealth_modifiers(&self, attack_ship_name: &str, target_ship_name: &str) -> i16 {
    let attack_ship = self.ships.get(attack_ship_name).unwrap().read().unwrap();
    let target_ship = self.ships.get(target_ship_name).unwrap().read().unwrap();

    // The result has to be negative.  You never get a bonus for "bad" stealth.
    if target_ship.design.stealth.is_some() {
      let delta_tl = i16::from(attack_ship.design.tl) - i16::from(target_ship.design.tl);
      (stealth_mod(target_ship.design.stealth) + delta_tl).min(0)
    } else {
      0
    }
  }

  // Quality modifiers are the level of sensors as well as skill of the crew
  fn sensor_quality_modifiers(&self, ship_name: &str) -> i16 {
    let ship = self.ships.get(ship_name).unwrap().read().unwrap();
    SENSOR_QUALITY_MOD[ship.current_sensors as usize] + i16::from(ship.crew.get_sensors())
  }

  fn sensor_lock(&mut self, ship_name: &String, target: &str, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    // Check if sensor lock is achieved.
    let check = i16::from(roll_dice(2, rng))
      + self.sensor_quality_modifiers(ship_name)
      + self.sensor_stealth_modifiers(ship_name, target)
      - 8;

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
      vec![EffectMsg::Message {
        content: format!("Sensor lock on {target} established by {ship_name}."),
      }]
    } else {
      vec![EffectMsg::Message {
        content: format!("Sensor lock on {target} not established by {ship_name}."),
      }]
    }
  }

  fn jam_comms(&self, ship_name: &String, target: &str, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    let check = i16::from(roll_dice(2, rng))
      + self.sensor_quality_modifiers(ship_name)
      + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
      - i16::from(roll_dice(2, rng))
      - self.sensor_quality_modifiers(target)
      - countermeasures_mod(self.ships.get(target).unwrap().read().unwrap().design.countermeasures);

    if check >= 0 {
      vec![EffectMsg::Message {
        content: format!("{ship_name} is jamming comms on {target}."),
      }]
    } else {
      vec![EffectMsg::Message {
        content: format!("{ship_name} failed to jam comms on {target}."),
      }]
    }
  }
  fn jam_missiles(&mut self, ship_name: &String, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    let mut effects = Vec::<EffectMsg>::new();
    // Find all missiles targeting this ship.
    let targeting_missiles = self
      .missiles
      .iter()
      .filter(|(_, missile)| missile.read().unwrap().target == *ship_name)
      .map(|(missile_name, _missile)| missile_name.clone())
      .collect::<Vec<_>>();

    let dice = roll_dice(2, rng);
    let check = i16::from(dice)
      + self.sensor_quality_modifiers(ship_name)
      + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
      - 10;

    if check >= 0 {
      // Deal with effect needing to allow one missile impact when the roll is made exactly.
      // Cast is safe because from above check >= 0.
      #[allow(clippy::cast_sign_loss)]
      let num_missiles = (check as usize).min(1);
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
              EffectMsg::Message {
                content: format!("Missile {} destroyed by jamming.", missile.get_name()),
              },
            ]
          })
          .collect::<Vec<_>>(),
      );
      // Remove the destroyed missiles from the list of all missiles.
    } else {
      // If the EW check failed, just let the users know.
      effects.push(EffectMsg::Message {
        content: format!("Missile jamming attempt by {ship_name} failed."),
      });
    }
    effects
  }

  fn break_sensor_lock(
    &self, ship_name: &String, target: &str, reverse_sensor_locks: &HashMap<String, Vec<String>>, rng: &mut dyn RngCore,
  ) -> Vec<EffectMsg> {
    // Check if the target of the BreakSensorLock has a sensor lock on this ship.
    // Get the list of every ship with a sensor lock on current ship; make sure the target of the BreakSensorLock is in that list.
    let valid_lock = reverse_sensor_locks
      .get(ship_name)
      .and_then(|ships_with_locks| ships_with_locks.iter().find(|&s| *s == target));
    if valid_lock.is_some() {
      // Make an opposed check - this ship vs the one with the lock..
      let check = i16::from(roll_dice(2, rng)) + self.sensor_quality_modifiers(ship_name)
        + countermeasures_mod(self.ships.get(ship_name).unwrap().read().unwrap().design.countermeasures)
        - self.sensor_quality_modifiers(target)
        // In this case the steal modifiers (which will be negative or 0) are a bonus.
        - self.sensor_stealth_modifiers(ship_name, target)
        - countermeasures_mod(self.ships.get(target).unwrap().read().unwrap().design.countermeasures)
        - i16::from(roll_dice(2, rng));
      if check >= 0 {
        self
          .ships
          .get(target)
          .unwrap()
          .write()
          .unwrap()
          .sensor_locks
          .retain(|s| s != ship_name);
        vec![EffectMsg::Message {
          content: format!("Sensor lock on {target} broken by {ship_name}."),
        }]
      } else {
        vec![EffectMsg::Message {
          content: format!("Sensor lock on {target} not broken by {ship_name}."),
        }]
      }
    } else {
      Vec::default()
    }
  }

  /// Attempt to jump all ships that have jump actions.
  /// 
  /// We assume (for now) a prior successful Astrogation check.  All that remains is the engineering check.
  /// Engineering check cannot stop you from jumping but can cause a misjump on failure.
  /// 
  /// # Arguments
  /// * `jump_actions` - The jump actions to process.
  /// * `rng` - The random number generator to use for engineering jump checks.
  /// 
  /// # Returns
  /// * A list of all the effects resulting from the jump attempts.
  /// 
  /// # Panics
  /// Panics if the lock cannot be obtained to read a ship.
  pub fn attempt_jumps(&mut self, jump_actions: &[(String, Vec<ShipAction>)], rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    let mut effects = Vec::<EffectMsg>::new();
    let mut jumped_ships = Vec::<String>::new();
    for (ship_name, actions) in jump_actions {
      // If there is even a single jump action (really should never be more than one) then attempt jump.
      if !actions.is_empty() {
        let Some(ship) = self.ships.get(ship_name) else {
          warn!("(Entity.attempt_jumps) Cannot find ship {} for jump.", ship_name);
          continue;
        };

        let ship = ship.read().unwrap();

        if ship.can_jump() && ship.current_fuel > ship.design.hull / 10 {
          // We intentionally skip the Astrogation check for now as there isn't astrogation skill in the simulation.
          // Also it could have happened at any time before this so its confusing.
          // Thus only relevant skill is engineering_jump.
          // The check should be an Easy (4+) check. However, in combat its rushed so its a Routine (6+) check.
          let check = i16::from(roll_dice(2, rng)) + i16::from(ship.crew.get_engineering_jump()) - 6;

          if check >= 0 {
            effects.push(EffectMsg::Message {
              content: format!("{ship_name} jumps successfully!"),
            });
          } else {
            effects.push(EffectMsg::Message {
              content: format!("{ship_name} jumped but with issues. Effect is {check}"),
            });
          }

          jumped_ships.push(ship_name.clone());
        }
      }
    }
    
    for ship_name in jumped_ships {
      self.ships.remove(&ship_name);
    }

    effects
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
        (Some(primary), Some(primary_ptr)) => {
          if primary_ptr.read().unwrap().get_name() != primary {
            return false;
          }
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
          .ok_or_else(|| format!("Unable to find entity named {} as primary for {}", primary, &name))?;
        planet.primary_ptr.replace(looked_up.clone());
      }
    }

    for missile in self.missiles.values() {
      let mut missile = missile.write().unwrap();
      let name = missile.get_name();
      let looked_up = self
        .ships
        .get(&missile.target)
        .ok_or_else(|| format!("Unable to find entity named {} as target for {}", missile.target, &name))?;
      missile.target_ptr.replace(looked_up.clone());
    }
    Ok(())
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
impl Clone for Entities {
  // This is an inefficient hack but simple - since its mostly for testing we'll use
  // this approach for now.
  fn clone(&self) -> Self {
    serde_json::from_str(&serde_json::to_string(self).unwrap()).unwrap()
  }
}

impl Serialize for Entities {
  fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
  where
    S: Serializer,
  {
    #[derive(Serialize)]
    struct Entities {
      ships: Vec<Ship>,
      missiles: Vec<Missile>,
      planets: Vec<Planet>,
      actions: ShipActionList,
    }

    let mut entities = Entities {
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
    })
  }
}

// Build a deep clone of the ships. It does not need to be thread safe so we can drop the use of Arc
pub(crate) fn deep_clone(ships: &HashMap<String, Arc<RwLock<Ship>>>) -> HashMap<String, Ship> {
  ships
    .iter()
    .map(|(name, ship)| (name.clone(), ship.read().unwrap().clone()))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::crew::{Crew, Skills};
  use crate::debug;
  use crate::ship::{config_test_ship_templates, ShipDesignTemplate};
  use assert_json_diff::assert_json_eq;
  use cgmath::assert_relative_eq;
  use cgmath::{Vector2, Zero};
  use rand::rngs::{mock::StepRng, SmallRng};
  use rand::SeedableRng;
  use serde_json::json;

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
    );

    // Add another ship
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4.0, 5.0, 6.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
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
    )?;

    // Launch a missile
    entities.launch_missile("Ship1", "Ship2").unwrap();

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
    entities.add_ship(String::from("Ship1"), Vec3::new(1.0, 2.0, 3.0), Vec3::zero(), &design, None);
    entities.add_ship(String::from("Ship2"), Vec3::new(4.0, 5.0, 6.0), Vec3::zero(), &design, None);
    entities.add_ship(String::from("Ship3"), Vec3::new(7.0, 8.0, 9.0), Vec3::zero(), &design, None);

    assert_eq!(entities.ships.get("Ship1").unwrap().read().unwrap().get_name(), "Ship1");
    assert_eq!(entities.ships.get("Ship2").unwrap().read().unwrap().get_name(), "Ship2");
    assert_eq!(entities.ships.get("Ship3").unwrap().read().unwrap().get_name(), "Ship3");
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
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4000.0, 5000.0, 6000.0),
      Vec3::zero(),
      &design,
      None,
    );
    entities.add_ship(
      String::from("Ship3"),
      Vec3::new(7000.0, 8000.0, 9000.0),
      Vec3::zero(),
      &design,
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
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);

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
    )?;
    assert!(entities.validate(), "Entities with a single valid planet should be valid");

    // Test 3: Add a valid ship
    entities.add_ship(String::from("Ship1"), Vec3::new(1.0, 2.0, 3.0), Vec3::zero(), &design, None);
    assert!(entities.validate(), "Entities with a valid planet and ship should be valid");

    // Test 4: Add a second ship
    entities.add_ship(String::from("Ship2"), Vec3::new(4.0, 5.0, 6.0), Vec3::zero(), &design, None);
    assert!(
      entities.validate(),
      "Entities with a valid planet and two ships should be valid"
    );

    // Test 5: Add a valid missile
    entities.launch_missile("Ship1", "Ship2").unwrap();
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
    );

    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(800.0, 500.0, 300.0),
      Vec3::zero(),
      &design,
      None,
    );
    entities.launch_missile("Ship1", "Ship2").unwrap();
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
    entities.add_planet(String::from("Sun"), Vec3::zero(), String::from("blue"), None, 6.371e6, 6e24)?;

    // Update the planet a few times
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);
    let ship_snapshot = deep_clone(&entities.ships);
    entities.update_all(&ship_snapshot, &mut rng);

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
    )?;
    entities.add_planet(
      String::from("Planet2"),
      Vec3::new(0.0, 5_000_000.0, EARTH_RADIUS),
      String::from("red"),
      None,
      3e7,
      3e23,
    )?;
    entities.add_planet(
      String::from("Planet3"),
      Vec3::new(EARTH_RADIUS / 2.0_f64.sqrt(), 8000.0, EARTH_RADIUS / 2.0_f64.sqrt()),
      String::from("green"),
      None,
      4e6,
      1e26,
    )?;

    // Update the entities a few times
    entities.update_all(&deep_clone(&entities.ships), &mut rng);
    entities.update_all(&deep_clone(&entities.ships), &mut rng);
    entities.update_all(&deep_clone(&entities.ships), &mut rng);

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
      r#"{"name":"Sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":700000000.0,"mass":100.0}"#
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
      r#"{"name":"planet2","position":[1000000000.0,0.0,0.0],"velocity":[0.0,0.0,2.583215051055564e-9],"color":"red","radius":4000000.0,"mass":100.0,"primary":"planet1"}"#
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
    )?;
    entities.add_planet(
      String::from("Planet2"),
      Vec3::new(0.0, 5_000_000.0, EARTH_RADIUS),
      String::from("red"),
      None,
      3e7,
      3.00e23,
    )?;
    entities.add_planet(
      String::from("Planet3"),
      Vec3::new(EARTH_RADIUS / 2.0_f64.sqrt(), 8000.0, EARTH_RADIUS / 2.0_f64.sqrt()),
      String::from("green"),
      None,
      4e6,
      1e26,
    )?;

    // Create entities with random positions and names
    entities.add_ship(
      String::from("Ship1"),
      Vec3::new(1000.0, 2000.0, 3000.0),
      Vec3::zero(),
      &design,
      None,
    );
    entities.add_ship(
      String::from("Ship2"),
      Vec3::new(4000.0, 5000.0, 6000.0),
      Vec3::zero(),
      &design,
      None,
    );
    entities.add_ship(
      String::from("Ship3"),
      Vec3::new(7000.0, 8000.0, 9000.0),
      Vec3::zero(),
      &design,
      None,
    );

    let cmp = json!({
    "ships":[
        {"name":"Ship1","position":[1000.0,2000.0,3000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": []
        },
        {"name":"Ship2","position":[4000.0,5000.0,6000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": []
        },
        {"name":"Ship3","position":[7000.0,8000.0,9000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
        "current_hull":160,
        "current_armor":5,
        "current_power":300,
        "current_maneuver":3,
        "current_jump":2,
        "current_fuel":81,
        "current_crew":11,
        "current_sensors": "Improved",
        "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": []
        }],
    "missiles":[],
    "actions":[],
    "planets":[
        {"name":"Planet1","position":[151_250_000_000.0,2_000_000.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6_371_000.0,"mass":5.972e24,
        "gravity_radius_1":6_375_069.342_849_095,"gravity_radius_05":9_015_709.525_726_125,"gravity_radius_025":12_750_138.685_698_19},
        {"name":"Planet2","position":[0.0,5_000_000.0,151_250_000_000.0],"velocity":[0.0,0.0,0.0],"color":"red","radius":30_000_000.0,"mass":3.00e23},
        {"name":"Planet3","position":[106_949_900_654.465_3,8000.0,106_949_900_654.465_3],"velocity":[0.0,0.0,0.0],"color":"green","radius":4_000_000.0,"mass":1e26,
        "gravity_radius_2":18_446_331.779_326_223,"gravity_radius_1":26_087_052.578_356_97,"gravity_radius_05":36_892_663.558_652_446,"gravity_radius_025":52_174_105.156_713_94}
     ]});

    assert_json_eq!(&entities, &cmp);

    Ok(())
  }
  #[tokio::test]
  async fn test_unordered_scenario_file() {
    let _ = pretty_env_logger::try_init();

    let entities = Entities::load_from_file("./tests/test-scenario.json").await.unwrap();
    assert!(entities.validate(), "Scenario file failed validation");
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
    );
    entities2.add_ship(
      "Ship1".to_string(),
      Vec3::new(1.0, 2.0, 3.0),
      Vec3::new(0.1, 0.2, 0.3),
      &design,
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
    )?;
    entities2.add_planet(
      "Planet1".to_string(),
      Vec3::new(7.0, 8.0, 9.0),
      "green".to_string(),
      None,
      6371e3,
      5.97e24,
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
    );
    assert_eq!(entities1, entities2, "Entities should be equal again");

    // Add some missiles to test
    entities1.launch_missile("Ship1", "Ship2").unwrap();

    // Test the two should not be equal
    assert_ne!(
      entities1, entities2,
      "Entities should not be equal with different number of missiles"
    );

    // Add the same missile to entities2
    entities2.launch_missile("Ship1", "Ship2").unwrap();
    assert_eq!(entities1, entities2, "Entities should be equal again");

    // Test with a different missile
    entities1.launch_missile("Ship1", "Ship2").unwrap();
    assert_ne!(entities1, entities2, "Entities should not be equal with different missiles");

    // Add the same missile to entities2
    entities2.launch_missile("Ship1", "Ship2").unwrap();
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

    entities.add_ship(String::from("Ship1"), Vec3::new(1.0, 2.0, 3.0), Vec3::zero(), &design, None);

    // Test launching a missile with an invalid target
    assert!(
      entities.launch_missile("Ship1", "Ship2").is_err(),
      "Launching a missile with an invalid target should be an error"
    );

    // Test launching a missile with an invalid source
    assert!(
      entities.launch_missile("Ship2", "Ship1").is_err(),
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
    ship.crew = crew;
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

    debug!(
      "(test_jam_missiles) roll1 = {} roll2={}",
      crate::combat::roll(&mut rng),
      crate::combat::roll(&mut rng)
    );
    // Create a ship with good sensor skills
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

    let effects = entities.sensor_actions(&actions, &mut rng);

    // With a roll of 6 and sensor skill of 4, check should be positive
    // resulting in successful jamming
    assert_eq!(entities.missiles.len(), 1); // Only one missile should be destroyed due to check result
    assert_eq!(effects.len(), 2); // One message for jamming success and one for missile destruction
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("destroyed by jamming")
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

    let effects = entities.sensor_actions(&actions, &mut rng);

    // With a roll of 1 and sensor skill of 4, check should be negative
    // resulting in failed jamming
    assert_eq!(entities.missiles.len(), 2); // No missiles should be destroyed due to check result
    assert_eq!(effects.len(), 1); // Only one message for jamming failure
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("jamming attempt by defender failed")
    )));
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

    let effects = entities.sensor_actions(&actions, &mut rng);
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("not established by")
    )));
    let mut rng = StepRng::new(5, 0); // Will always roll 6 for predictable results

    let effects = entities.sensor_actions(&actions, &mut rng);

    // With a roll of 6 and sensor skill of 4, the lock should be established
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("lock on target established by")
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

    let effects = entities.sensor_actions(&actions, &mut rng);

    // Check that the lock was broken
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("broken by")
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
    let effects = entities.sensor_actions(&actions, &mut rng);

    // Check that the lock was not broken
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("not broken by")
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

    let effects = entities.sensor_actions(&actions, &mut rng);

    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("failed to jam comms on")
    )));

    let mut rng = StepRng::new(4, 1); // Going past 6 on second two rolls ensures jammer wins
    let effects = entities.sensor_actions(&actions, &mut rng);
    // With high sensor skill and good roll, jamming should succeed
    assert!(effects.iter().any(|e| matches!(e,
        EffectMsg::Message { content } if content.contains("is jamming comms on")
    )));
  }

  async fn setup_sensor_test_ships(
    attack_name: &str, attack_crew_skill: u8, target_name: &str, target_crew_skill: u8, attack_design: &str,
    target_design: &str,
  ) -> Entities {
    const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";

    // Load ship templates
    let templates = crate::ship::load_ship_templates_from_file(DEFAULT_SHIP_TEMPLATES_FILE)
      .await
      .expect("Unable to load ship template file.");

    // Create entities
    let mut entities = Entities::new();

    // Create attacker ship
    let mut attack_ship = Ship::default();
    attack_ship.set_name(attack_name.to_string());
    attack_ship.design = templates.get(attack_design).unwrap().clone();
    attack_ship.current_sensors = attack_ship.design.sensors;
    let mut attack_crew = Crew::default();
    attack_crew.set_skill(Skills::Sensors, attack_crew_skill);
    attack_ship.crew = attack_crew;

    // Create target ship
    let mut target_ship = Ship::default();
    target_ship.set_name(target_name.to_string());
    target_ship.design = templates.get(target_design).unwrap().clone();
    let mut target_crew = Crew::default();
    target_crew.set_skill(Skills::Sensors, target_crew_skill);
    target_ship.crew = target_crew;

    // Add ships to entities
    entities
      .ships
      .insert(attack_name.to_string(), Arc::new(RwLock::new(attack_ship)));
    entities
      .ships
      .insert(target_name.to_string(), Arc::new(RwLock::new(target_ship)));

    entities
  }

  #[test_log::test(tokio::test)]
  async fn test_sensor_stealth_modifiers() {
    let test_cases = [
      // (attack_design, target_design, skill(ignored), skill (ignored), expected_modifier)
      ("Free Trader", "Far Trader", 0, 0, 0),  // No stealth - should be 0
      ("Light Fighter", "Buccaneer", 3, 0, 0), // No stealth - should be 0
      ("Harrier", "Free Trader", 2, 0, 0),
      ("Free Trader", "Harrier", 0, 0, -9),
    ];

    for (attack_design, target_design, attack_skill, target_skill, expected) in test_cases {
      let entities =
        setup_sensor_test_ships("attacker", attack_skill, "target", target_skill, attack_design, target_design).await;

      let result = entities.sensor_stealth_modifiers("attacker", "target");
      assert_eq!(
            result,
            expected,
            "Failed with attack_design={attack_design}, target_design={target_design}, attack_skill={attack_skill},target_skill={target_skill}, expected={expected}",
        );
    }
  }

  #[tokio::test]
  async fn test_sensor_quality_modifiers() {
    let test_cases = [
      // (attack_design, attack_skill, expected_modifier)
      ("Free Trader", 0, -2),
      ("Light Fighter", 3, 3),
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
  fn test_sensor_stealth_modifiers_invalid_ships() {
    let entities = Entities::new();
    entities.sensor_stealth_modifiers("nonexistent_attacker", "nonexistent_target");
  }
}

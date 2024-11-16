use cgmath::{InnerSpace, Vector3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::payloads::{EffectMsg, FireAction};
use rand::RngCore;

use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::combat::attack;
use crate::combat::{create_sand_counts, do_fire_actions};
use crate::missile::Missile;
use crate::planet::Planet;
use crate::ship::{FlightPlan, Ship, ShipDesignTemplate};
use crate::ship::{Weapon, WeaponMount, WeaponType};
use crate::{debug, error, info, warn};

pub const DELTA_TIME: u64 = 360;
pub const DEFAULT_ACCEL_DURATION: u64 = 10000;
// We will use 4 sig figs for every physics constant we import.
// This is the value of 1 (earth) gravity in m/s^2
pub const G: f64 = 9.807000000;
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
}

impl PartialEq for Entities {
    fn eq(&self, other: &Self) -> bool {
        self.ships.len() == other.ships.len()
            && self.missiles.len() == other.missiles.len()
            && self.planets.len() == other.planets.len()
            && self.ships.keys().all(|k| other.ships.contains_key(k))
            && self.missiles.keys().all(|k| other.missiles.contains_key(k))
            && self.planets.keys().all(|k| other.planets.contains_key(k))
            && self.ships.keys().all(|k| {
                self.ships[k]
                    .read()
                    .unwrap()
                    .eq(&other.ships[k].read().unwrap())
            })
            && self.missiles.keys().all(|k| {
                self.missiles[k]
                    .read()
                    .unwrap()
                    .eq(&other.missiles[k].read().unwrap())
            })
            && self.planets.keys().all(|k| {
                self.planets[k]
                    .read()
                    .unwrap()
                    .eq(&other.planets[k].read().unwrap())
            })
    }
}

impl Entities {
    pub fn new() -> Self {
        Entities {
            ships: HashMap::new(),
            missiles: HashMap::new(),
            planets: HashMap::new(),
            next_missile_id: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.ships.len() + self.missiles.len() + self.planets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ships.is_empty() && self.missiles.is_empty() && self.planets.is_empty()
    }

    pub fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(file_name)?;
        let reader = std::io::BufReader::new(file);
        let mut entities: Entities = serde_json::from_reader(reader)?;
        info!("Load scenario file \"{}\".", file_name);

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

    pub fn add_ship(
        &mut self,
        name: String,
        position: Vec3,
        velocity: Vec3,
        acceleration: Vec3,
        design: Arc<ShipDesignTemplate>,
    ) {
        let ship = Ship::new(
            name.clone(),
            position,
            velocity,
            FlightPlan::acceleration(acceleration),
            design,
        );
        self.ships.insert(name, Arc::new(RwLock::new(ship)));
    }

    pub fn add_planet(
        &mut self,
        name: String,
        position: Vec3,
        color: String,
        primary: Option<String>,
        radius: f64,
        mass: f64,
    ) {
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
            let primary = self.planets.get(primary_name).unwrap_or_else(|| {
                panic!(
                    "Primary planet {} not found for planet {}.",
                    primary_name, name
                )
            });

            (
                Some(primary.clone()),
                primary.read().unwrap().dependency + 1,
            )
        } else {
            (None, 0)
        };

        // A safety check to ensure we never have a pointer without a name of a primary or vis versa.
        assert!(
            primary_ptr.is_some() && primary.is_some()
                || primary_ptr.is_none() && primary.is_none()
        );

        let entity = Planet::new(
            name.clone(),
            position,
            color,
            radius,
            mass,
            primary,
            primary_ptr,
            dependency,
        );

        debug!("Added planet with fixed gravity wells {:?}", entity);
        self.planets.insert(name, Arc::new(RwLock::new(entity)));
    }

    pub fn launch_missile(&mut self, source: &str, target: &str) -> Result<(), String> {
        // Could use a random number generator here for the name but that makes tests flakey (random)
        // So this counter used to distinguish missiles between the same source and target
        let id = self.next_missile_id;
        self.next_missile_id += 1;

        let name = format!("{}::{}::{:X}", source, target, id);
        let source_ptr = self
            .ships
            .get(source)
            .ok_or_else(|| format!("Missile source {} not found for missile {}.", source, name))?
            .clone();

        let target_ptr = self
            .ships
            .get(target)
            .ok_or_else(|| format!("Target {} not found for missile {}.", target, name))?
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

    // Set the flight plan. Returns true if it was set. False if the plan was invalid for any reason.
    pub fn set_flight_plan(&mut self, name: &str, plan: &FlightPlan) -> Result<(), String> {
        if let Some(entity) = self.ships.get_mut(name) {
            entity.write().unwrap().set_flight_plan(plan)
        } else {
            Err(format!(
                "Could not set acceleration for non-existent entity {}",
                name
            ))
        }
    }

    pub fn fire_actions(
        &mut self,
        fire_actions: Vec<(String, Vec<FireAction>)>,
        ship_snapshot: &HashMap<String, Ship>,
        rng: &mut dyn RngCore,
    ) -> Vec<EffectMsg> {
        // Create a snapshot of all the sand capabilities of each ship.
        let mut sand_counts = create_sand_counts(ship_snapshot);

        let effects = fire_actions
            .iter()
            .flat_map(|(attacker, actions)| {
                let attack_ship = ship_snapshot.get(attacker).unwrap_or_else(|| {
                    panic!("Cannot find attacker {} for fire actions.", attacker)
                });
                let (missiles, effects) =
                    do_fire_actions(attack_ship, &mut self.ships, &mut sand_counts, actions, rng);
                for missile in missiles {
                    if let Err(msg) = self.launch_missile(&missile.source, &missile.target) {
                        error!("Could not launch missile: {}", msg);
                    }
                }
                effects
            })
            .collect();
        effects
    }

    pub fn update_all(
        &mut self,
        ship_snapshot: &HashMap<String, Ship>,
        rng: &mut dyn RngCore,
    ) -> Vec<EffectMsg> {
        let mut planets = self.planets.values_mut().collect::<Vec<_>>();
        planets.sort_by(|a, b| {
            let a_ent = a.read().unwrap();
            let b_ent = b.read().unwrap();
            a_ent.dependency.cmp(&b_ent.dependency)
        });

        // If we have effects from planet updates this has to change and get a bit more complex (like missiles below)
        planets.iter().for_each(|planet| {
            planet.write().unwrap().update();
        });

        let mut cleanup_missile_list = Vec::<String>::new();

        // Creating this sorted list is necessary ONLY to ensure unit tests run consistently
        // If it ends up being slow we should take it out.
        let mut sorted_missiles = self.missiles.values().collect::<Vec<_>>();
        sorted_missiles.sort_by(|a, b| {
            let a_ent = a.read().unwrap();
            let b_ent = b.read().unwrap();
            a_ent.get_name().partial_cmp(b_ent.get_name()).unwrap()
        });

        let mut effects = sorted_missiles
            .into_iter()
            .filter_map(|missile| {
                let mut missile = missile.write().unwrap();
                let update = missile.update();
                let missile_name = missile.get_name();
                let missile_pos = missile.get_position();
                let missile_source = ship_snapshot.get(&missile.source).unwrap();

                // We use UpdateAction vs just returning the effect so that the call to attack() stays at this level rather than
                // being embedded in the missile update code.  Also enables elimination of missiles.
                match update? {
                    UpdateAction::ShipImpact { ship, missile } => {
                        // When a missile impacts fake it as an attack by a single turret missile.
                        const FAKE_MISSILE_LAUNCHER: Weapon = Weapon {
                            kind: WeaponType::Missile,
                            mount: WeaponMount::Turret(1),
                        };
                        info!(
                            "(Entity.update_all) Missile impact on {} by missile {}.",
                            ship, missile
                        );
                        let target = self
                            .ships
                            .get(&ship)
                            .map_or_else(|| { warn!("Cannot find target {} for missile. It may have been destroyed.", ship); None},
                            |ship| Some(ship.clone()));
                        if let Some(target) = target {
                            let mut target = target.write().unwrap();
                            let effects = attack(
                                0,
                                0,
                                missile_source,
                                &mut target,
                                &FAKE_MISSILE_LAUNCHER,
                                rng,
                            );
                            cleanup_missile_list.push(missile);

                            Some(effects)
                         } else {
                            debug!(
                                "(Entity.update_all) Missile {} exhausted at position {:?}.",
                                missile, missile_pos
                            );
                            Some(vec![EffectMsg::ExhaustedMissile {
                                position: missile_pos,
                            }])
                        }
                    }
                    UpdateAction::ExhaustedMissile { name } => {
                        assert!(name == missile_name);
                        debug!("(Entity.update_all) Removing missile {}", name);
                        cleanup_missile_list.push(name.clone());
                        Some(vec![EffectMsg::ExhaustedMissile {
                            position: missile_pos,
                        }])
                    }
                    update => panic!(
                        "(Entity.update_all) Unexpected update {:?} during missile updates.",
                        update
                    ),
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
                            debug!(
                                "(Entity.update_all) Ship {} destroyed at position {:?}.",
                                name, pos
                            );
                            cleanup_ships_list.push(name.to_string());
                            Some(vec![
                                EffectMsg::ShipDestroyed { position: pos },
                                EffectMsg::Message {
                                    content: format!("{} destroyed.", name),
                                },
                            ])
                        }
                        update => panic!(
                            "(Entity.update_all) Unexpected update {:?} during ship updates.",
                            update
                        ),
                    }
                })
                .flatten()
                .collect::<Vec<_>>(),
        );

        cleanup_missile_list.iter().for_each(|name| {
            debug!("(Entity.update_all) Removing missile {}", name);
            self.missiles.remove(name);
        });

        cleanup_ships_list.iter().for_each(|name| {
            debug!("(Entity.update_all) Removing ship {}", name);
            self.ships.remove(name);
        });

        effects
    }

    pub fn validate(&self) -> bool {
        for planet in self.planets.values() {
            let planet = planet.read().unwrap();

            if planet.dependency < 0 {
                return false;
            }

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
                || missile
                    .target_ptr
                    .as_ref()
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_name()
                    != missile.target
            {
                return false;
            }
        }
        true
    }

    pub fn fixup_pointers(&mut self) -> Result<(), String> {
        for planet in self.planets.values() {
            let mut planet = planet.write().unwrap();
            let name = planet.get_name().to_string();
            match &mut planet.primary {
                Some(primary) => {
                    let looked_up = self.planets.get(primary).ok_or_else(|| {
                        format!(
                            "Unable to find entity named {} as primary for {}",
                            primary, &name
                        )
                    })?;
                    planet.primary_ptr.replace(looked_up.clone());
                }
                None => {}
            }
        }

        for missile in self.missiles.values() {
            let mut missile = missile.write().unwrap();
            let name = missile.get_name();
            let looked_up = self.ships.get(&missile.target).ok_or_else(|| {
                format!(
                    "Unable to find entity named {} as target for {}",
                    missile.target, &name
                )
            })?;
            missile.target_ptr.replace(looked_up.clone());
        }
        Ok(())
    }

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
        if self.ships.values().len() + self.missiles.values().len() + self.planets.values().len()
            == 0
        {
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
        }

        let mut entities = Entities {
            ships: self
                .ships
                .values()
                .map(|s| s.read().unwrap().clone())
                .collect::<Vec<Ship>>(),
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
        };

        //The following sort_by is not necessary and adds inefficiency BUT ensures we serialize each item in the same order
        //each time. This makes writing tests a lot easier!
        entities
            .ships
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
        entities
            .missiles
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
        entities
            .planets
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());

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
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::debug;
    use crate::ship::{config_test_ship_templates, ShipDesignTemplate};
    use assert_json_diff::assert_json_eq;
    use cgmath::assert_relative_eq;
    use cgmath::{Vector2, Zero};
    use rand::rngs::SmallRng;
    use rand::SeedableRng;
    use serde_json::json;

    #[test_log::test]
    fn test_entities_display_and_debug() {
        let mut entities = Entities::new();

        // Add a ship
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
            Arc::new(ShipDesignTemplate::default()),
        );

        // Add another ship
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
            Arc::new(ShipDesignTemplate::default()),
        );

        // Add a planet
        entities.add_planet(
            String::from("Planet1"),
            Vec3::new(4.0, 5.0, 6.0),
            String::from("blue"),
            None,
            6371e3,
            5.97e24,
        );

        // Launch a missile
        entities.launch_missile("Ship1", "Ship2").unwrap();

        // Test Display trait
        let display_output = format!("{}", entities);
        assert!(display_output.contains("Ship1"));
        assert!(display_output.contains("Planet1"));
        assert!(display_output.contains("Ship2"));
        assert!(display_output.contains("Ship1::Ship2::0"));

        // Test Debug trait
        let debug_output = format!("{:?}", entities);
        assert_eq!(
            display_output, debug_output,
            "Display and Debug outputs should be identical"
        );

        // Test empty Entities
        let empty_entities = Entities::new();
        assert_eq!(
            format!("{}", empty_entities),
            "Entities {}",
            "Empty Entities should display as 'Entities {{}}'"
        );
        assert_eq!(
            format!("{:?}", empty_entities),
            "Entities {}",
            "Empty Entities should debug as 'Entities {{}}'"
        );
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
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );

        assert_eq!(
            entities
                .ships
                .get("Ship1")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship1"
        );
        assert_eq!(
            entities
                .ships
                .get("Ship2")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship2"
        );
        assert_eq!(
            entities
                .ships
                .get("Ship3")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship3"
        );
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
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(1.0, 1.0, 1.0);
        let acceleration2 = Vec3::new(2.0, 1.0, -2.0);
        let acceleration3 = Vec3::new(-1.0, -1.0, -0.0);
        entities
            .set_flight_plan("Ship1", &FlightPlan((acceleration1, 10000).into(), None))
            .unwrap();
        entities
            .set_flight_plan("Ship2", &FlightPlan((acceleration2, 10000).into(), None))
            .unwrap();
        entities
            .set_flight_plan("Ship3", &FlightPlan((acceleration3, 10000).into(), None))
            .unwrap();

        // Update the entities a few times
        let ship_snapshot = deep_clone(&entities.ships);
        entities.update_all(&ship_snapshot, &mut rng);
        let ship_snapshot = deep_clone(&entities.ships);
        entities.update_all(&ship_snapshot, &mut rng);
        let ship_snapshot = deep_clone(&entities.ships);
        entities.update_all(&ship_snapshot, &mut rng);

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(5720442.4, 5721442.4, 5722442.4);
        let expected_position2 = Vec3::new(11442884.8, 5724442.4, -11432884.8);
        let expected_position3 = Vec3::new(-5712442.4, -5711442.4, 9000.0);
        assert_relative_eq!(
            entities
                .ships
                .get("Ship1")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position1,
            epsilon = 1e-7
        );
        assert_relative_eq!(
            entities
                .ships
                .get("Ship2")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position2,
            epsilon = 1e-7
        );
        assert_relative_eq!(
            entities
                .ships
                .get("Ship3")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position3,
            epsilon = 1e-7
        );
    }

    #[test_log::test]
    fn test_entities_validate() {
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
        );
        assert!(
            entities.validate(),
            "Entities with a single valid planet should be valid"
        );

        // Test 3: Add a valid ship
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        assert!(
            entities.validate(),
            "Entities with a valid planet and ship should be valid"
        );

        // Test 3.5: Add a second ship
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        assert!(
            entities.validate(),
            "Entities with a valid planet and two ships should be valid"
        );

        // Test 4: Add a valid missile
        entities.launch_missile("Ship1", "Ship2").unwrap();
        assert!(
            entities.validate(),
            "Entities with a valid planet, two ships, and missile should be valid"
        );

        // Test 5: Add a planet with an invalid dependency
        let planet = Planet::new(
            String::from("InvalidPlanet"),
            Vec3::new(7.0, 8.0, 9.0),
            String::from("red"),
            6371e3,
            5.97e24,
            Some(String::from("NonExistentPlanet")),
            None,
            -6,
        );

        entities
            .planets
            .insert(String::from("InvalidPlanet"), Arc::new(RwLock::new(planet)));
        assert!(
            !entities.validate(),
            "Entities with an invalid planet dependency should be invalid"
        );

        // Test 6: Fix the invalid dependency
        {
            // Do this in its own block to release the locks after modification.
            let mut planet = entities
                .planets
                .get_mut("InvalidPlanet")
                .unwrap()
                .write()
                .unwrap();
            planet.dependency = 0;
            planet.primary = None;
            planet.primary_ptr = None;
        }
        assert!(
            entities.validate(),
            "Entities with fixed planet dependency should be valid"
        );

        // Test 6.5: Add a planet with a missing primary_ptr
        let planet = Planet::new(
            String::from("InvalidPlanet2"),
            Vec3::new(7.0, 8.0, 9.0),
            String::from("red"),
            6371e3,
            5.97e24,
            Some(String::from("Sun")),
            None,
            1,
        );

        entities.planets.insert(
            String::from("InvalidPlanet2"),
            Arc::new(RwLock::new(planet)),
        );
        assert!(
            !entities.validate(),
            "Entities with an invalid primary_ptr should be invalid"
        );

        // Test 7: Fix the invalid primary_ptr
        {
            let planets_table = &mut entities.planets;
            let sun = planets_table.get_mut("Sun").unwrap().clone();
            let mut planet = planets_table
                .get_mut("InvalidPlanet2")
                .unwrap()
                .write()
                .unwrap();
            planet.primary_ptr = Some(sun);
        }
        assert!(
            entities.validate(),
            "Entities with fixed primary_ptr should be valid"
        );

        // Test 8: Make the primary_ptr have a different name from the primary
        {
            let planets_table = &mut entities.planets;
            let invalid_planet = planets_table.get_mut("InvalidPlanet").unwrap().clone();
            let mut planet = planets_table
                .get_mut("InvalidPlanet2")
                .unwrap()
                .write()
                .unwrap();
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
            Vec3::zero(),
            design.clone(),
        );

        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(800.0, 500.0, 300.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        entities.launch_missile("Ship1", "Ship2").unwrap();
        // Test 9: Create a missile with no target_ptr
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
        // Test 10: Fix the missile target_ptr
        {
            let missiles_table = &mut entities.missiles;
            let ship2 = entities.ships.get("Ship2").unwrap().clone();
            let mut missile = missiles_table
                .get_mut("Ship1::Ship2::0")
                .unwrap()
                .write()
                .unwrap();
            missile.target_ptr = Some(ship2);
        }
        assert!(
            entities.validate(),
            "Entities with a missile with fixed target_ptr should be valid"
        );
    }

    #[test]
    fn test_sun_update() {
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
        );

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
            entities
                .planets
                .get("Sun")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position
        );
    }
    #[test]
    // TODO: Add test to add a moon.
    fn test_complex_planet_update() {
        let _ = pretty_env_logger::try_init();
        let mut rng = SmallRng::seed_from_u64(0);

        fn check_radius_and_y(
            pos: Vec3,
            primary: Vec3,
            expected_mag: f64,
            expected_y: f64,
        ) -> (bool, bool) {
            const TOLERANCE: f64 = 0.01;
            let radius = pos - primary;
            let radius_2d = Vector2::<f64>::new(radius.x, radius.z);

            debug!(
                "Radius_2d.magnitude(): {:?} vs Expected: {}",
                radius_2d.magnitude(),
                expected_mag
            );
            return (
                (radius_2d.magnitude() - expected_mag).abs() / expected_mag < TOLERANCE,
                radius.y == expected_y,
            );
        }

        let mut entities = Entities::new();

        const EARTH_RADIUS: f64 = 151.25e9;
        // Create some planets and see if they move.
        entities.add_planet(
            String::from("Planet1"),
            Vec3::new(EARTH_RADIUS, 2000000.0, 0.0),
            String::from("blue"),
            None,
            6.371e6,
            6e24,
        );
        entities.add_planet(
            String::from("Planet2"),
            Vec3::new(0.0, 5000000.0, EARTH_RADIUS),
            String::from("red"),
            None,
            3e7,
            3e23,
        );
        entities.add_planet(
            String::from("Planet3"),
            Vec3::new(
                EARTH_RADIUS / (2.0 as f64).sqrt(),
                8000.0,
                EARTH_RADIUS / (2.0 as f64).sqrt(),
            ),
            String::from("green"),
            None,
            4e6,
            1e26,
        );

        // Update the entities a few times
        entities.update_all(&deep_clone(&entities.ships), &mut rng);
        entities.update_all(&deep_clone(&entities.ships), &mut rng);
        entities.update_all(&deep_clone(&entities.ships), &mut rng);

        // FIXME: This isn't really testing what we want to test.
        // Fix it so we have real primaries and test the distance to those.
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities
                    .planets
                    .get("Planet1")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
                Vec3::zero(),
                EARTH_RADIUS,
                2000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities
                    .planets
                    .get("Planet2")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
                Vec3::zero(),
                EARTH_RADIUS,
                5000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities
                    .planets
                    .get("Planet3")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
                Vec3::zero(),
                EARTH_RADIUS,
                8000.0
            )
        );
    }

    // A test of deserializing a planet string.
    #[test]
    fn test_serialize_planet() {
        let _ = pretty_env_logger::try_init();

        let tst_planet = Planet::new(
            String::from("Sun"),
            Vec3::zero(),
            String::from("yellow"),
            7e8,
            100.0,
            None,
            None,
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
                x: 1000000000.0,
                y: 0.0,
                z: 0.0,
            },
            String::from("red"),
            4e6,
            100.0,
            Some(String::from("planet1")),
            Some(Arc::new(RwLock::new(tst_planet))),
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
            None,
            0,
        );

        let tst_str = r#"{"name":"planet2","position":[0,0,0],"velocity":[0.0,0.0,0.0],
        "color":"red","radius":4e6,"mass":100.0,"primary":"planet1"}"#;
        let tst_planet_4 = serde_json::from_str::<Planet>(tst_str).unwrap();

        assert_eq!(tst_planet_3, tst_planet_4);
    }

    #[test_log::test]
    fn test_mixed_entities_serialize() {
        let mut entities = Entities::new();
        let design = Arc::new(ShipDesignTemplate::default());

        // This constant is the radius of the earth's orbit (distance from sun).
        // It is NOT the radius of the earth (6.371e6 m)
        const EARTH_RADIUS: f64 = 151.25e9;
        // Create some planets and see if they move.
        entities.add_planet(
            String::from("Planet1"),
            Vec3::new(EARTH_RADIUS, 2000000.0, 0.0),
            String::from("blue"),
            None,
            6.371e6,
            5.972e24,
        );
        entities.add_planet(
            String::from("Planet2"),
            Vec3::new(0.0, 5000000.0, EARTH_RADIUS),
            String::from("red"),
            None,
            3e7,
            3.00e23,
        );
        entities.add_planet(
            String::from("Planet3"),
            Vec3::new(
                EARTH_RADIUS / (2.0 as f64).sqrt(),
                8000.0,
                EARTH_RADIUS / (2.0 as f64).sqrt(),
            ),
            String::from("green"),
            None,
            4e6,
            1e26,
        );

        // Create entities with random positions and names
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );

        let cmp = json!({
        "ships":[
            {"name":"Ship1","position":[1000.0,2000.0,3000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
            "current_hull":160,
            "current_armor":5,
            "current_power":300,
            "current_maneuver":3,
            "current_jump":2,
            "current_fuel":81,
            "current_crew":11,
            "current_sensors": "Improved",
            "active_weapons": [true, true, true, true]
            },
            {"name":"Ship2","position":[4000.0,5000.0,6000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
            "current_hull":160,
            "current_armor":5,
            "current_power":300,
            "current_maneuver":3,
            "current_jump":2,
            "current_fuel":81,
            "current_crew":11,
            "current_sensors": "Improved",
            "active_weapons": [true, true, true, true]
            },
            {"name":"Ship3","position":[7000.0,8000.0,9000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
            "current_hull":160,
            "current_armor":5,
            "current_power":300,
            "current_maneuver":3,
            "current_jump":2,
            "current_fuel":81,
            "current_crew":11,
            "current_sensors": "Improved",
            "active_weapons": [true, true, true, true]
            }],
        "missiles":[],
        "planets":[
            {"name":"Planet1","position":[151250000000.0,2000000.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6371000.0,"mass":5.972e24,
            "gravity_radius_1":6375069.342849095,"gravity_radius_05":9015709.525726125,"gravity_radius_025":12750138.68569819},
            {"name":"Planet2","position":[0.0,5000000.0,151250000000.0],"velocity":[0.0,0.0,0.0],"color":"red","radius":30000000.0,"mass":3.00e23},
            {"name":"Planet3","position":[106949900654.4653,8000.0,106949900654.4653],"velocity":[0.0,0.0,0.0],"color":"green","radius":4000000.0,"mass":1e26,
            "gravity_radius_2":18446331.779326223,"gravity_radius_1":26087052.57835697,"gravity_radius_05":36892663.558652446,"gravity_radius_025":52174105.15671394}
         ]});

        assert_json_eq!(&entities, &cmp);
    }
    #[test]
    fn test_unordered_scenario_file() {
        let _ = pretty_env_logger::try_init();

        let entities = Entities::load_from_file("./tests/test-scenario.json").unwrap();
        assert!(entities.validate(), "Scenario file failed validation");
    }

    #[test]
    fn test_entities_equality() {
        config_test_ship_templates();

        let mut entities1 = Entities::new();
        let mut entities2 = Entities::new();
        let design = Arc::new(ShipDesignTemplate::default());

        // Add some ships
        entities1.add_ship(
            "Ship1".to_string(),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(0.1, 0.2, 0.3),
            Vec3::new(2.0, 0.0, 3.0),
            design.clone(),
        );
        entities2.add_ship(
            "Ship1".to_string(),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::new(0.1, 0.2, 0.3),
            Vec3::new(2.0, 0.0, 3.0),
            design.clone(),
        );

        // Add some planets
        entities1.add_planet(
            "Planet1".to_string(),
            Vec3::new(7.0, 8.0, 9.0),
            "green".to_string(),
            None,
            6371e3,
            5.97e24,
        );
        entities2.add_planet(
            "Planet1".to_string(),
            Vec3::new(7.0, 8.0, 9.0),
            "green".to_string(),
            None,
            6371e3,
            5.97e24,
        );

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
        assert_ne!(
            entities1, entities2,
            "Entities should not be equal after modifying a ship"
        );

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
            Vec3::new(1.0, 1.1, 1.2),
            design.clone(),
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
            Vec3::new(1.0, 1.1, 1.2),
            design.clone(),
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
        assert_ne!(
            entities1, entities2,
            "Entities should not be equal with different missiles"
        );

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
        assert_eq!(
            entities1, entities3,
            "Entities should be equal within floating-point precision"
        );

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
        assert_ne!(
            entities1, entities4,
            "Entities should not be equal after velocity change"
        );
    }

    #[test_log::test]
    fn test_entities_len_and_is_empty() {
        let mut entities = Entities::new();

        // Test empty entities
        assert_eq!(entities.len(), 0);
        assert!(entities.is_empty());

        // Add a ship
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
            Arc::new(ShipDesignTemplate::default()),
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
        );

        // Test entities with one ship and one planet
        assert_eq!(entities.len(), 2);
        assert!(!entities.is_empty());

        // Test with an empty entities
        entities = Entities::new();
        assert_eq!(entities.len(), 0);
        assert!(entities.is_empty());
    }
    #[test]
    fn test_launch_missile_invalid_target() {
        let mut entities = Entities::new();
        let design = Arc::new(ShipDesignTemplate::default());

        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
            design.clone(),
        );

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

    #[test_log::test]
    fn test_fixup_pointers() {
        config_test_ship_templates();

        // The best way to test this to to build a scenario file and then
        // deserialize it into an Entities struct.
        // Then we run fixup_pointers on it.
        // Then we do the same thing but with an invalid scenario file.
        // Then we run fixup_pointers on it and it should fail.

        // Test 1: Valid file
        let scenario = json!({"ships":[
            {"name":"ship1","position":[1000000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30}, 
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"sun"}]});

        let mut entities = serde_json::from_value::<Entities>(scenario).unwrap();
        assert!(
            entities.fixup_pointers().is_ok(),
            "Error fixing up pointers"
        );

        // Test 2: Add missile with a non-existent target
        let bad_scenario = json!({"ships":[
            {"name":"ship1","position":[1000000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2},
             {"name":"Invalid::1","source":"ship1","target":"InvalidShip","position":[0.0,0.0,500000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30}, 
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"sun"}]});

        let mut entities = serde_json::from_value::<Entities>(bad_scenario).unwrap();
        assert!(
            entities.fixup_pointers().is_err(),
            "Scenario file with bad missile should fail fixup_pointers"
        );

        // Test3: Add a planet with a non-existent primary
        let bad_scenario = json!({"ships":[
            {"name":"ship1","position":[1000000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":6,"structure":6},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
             "hull":4, "structure":6}],
             "missiles":[{"name":"ship1::ship2::0","source":"ship1","target":"ship2","position":[0.0,0.0,500000.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,58.0],"burns":2}],
             "planets":[{"name":"sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":6.96e8,"mass":1.989e30}, 
                        {"name":"earth","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6.371e6,"mass":5.972e24,"primary":"InvalidPlanet"}]});

        let mut entities = serde_json::from_value::<Entities>(bad_scenario).unwrap();
        assert!(
            entities.fixup_pointers().is_err(),
            "Scenario file with bad planet should fail fixup_pointers"
        );
    }
    #[test]
    fn test_set_flight_plan() {
        let mut entities = Entities::new();

        // Add a ship
        entities.add_ship(
            String::from("TestShip"),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::zero(),
            Vec3::zero(),
            Arc::new(ShipDesignTemplate::default()),
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
        assert!(
            result.is_err(),
            "Setting flight plan for non-existent ship should fail"
        );
    }
}

// Build a deep clone of the ships. It does not need to be thread safe so we can drop the use of Arc
pub(crate) fn deep_clone(ships: &HashMap<String, Arc<RwLock<Ship>>>) -> HashMap<String, Ship> {
    ships
        .iter()
        .map(|(name, ship)| (name.clone(), ship.read().unwrap().clone()))
        .collect()
}

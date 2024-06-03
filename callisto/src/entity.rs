use cgmath::{ElementWise, InnerSpace, Vector3, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use serde_with::{serde_as, skip_serializing_none};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::computer::{compute_target_path, FlightPlan, TargetParams};
use crate::payloads::Vec3asVec;

pub const DELTA_TIME: i64 = 1000;
pub const G: f64 = 9.81;

pub type Vec3 = Vector3<f64>;

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EntityKind {
    Ship,
    Planet {
        color: String,
        // The primary is the name of the planet which is the center of the orbit.
        // None means it not orbiting anything. Some("Earth") means its orbiting the planet named Earth.
        #[serde(default)]
        primary: Option<String>,

        // TODO: Ideally this would be in the primary structure rather than also be an Option outside it.  But I right now
        // can't figure out how to skip serde for a portion of a tuple.
        #[serde(skip)]
        primary_ptr: Option<Arc<RwLock<Entity>>>,
        radius: f64,
        mass: f64,
        // Dependency is used to enforce order of update.  Lower values (e.g. a star with value 0) are updated first.
        // Not needed to be passed in JSON to the client; not needed for comparison operations.
        #[serde(skip)]
        dependency: i32,
    },

    Missile {
        source: String,
        target: String,
        // FIXME: This is dangerous.  Its not clear how to deal with a Missile without a target_ptr.
        #[serde(skip)]
        target_ptr: Option<Arc<RwLock<Entity>>>,
        burns: i32,
    },
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entity {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    #[serde_as(as = "Vec3asVec")]
    acceleration: Vec3,
    pub kind: EntityKind,
}

impl PartialEq for Entity {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Entity {
    // Constructor for a new entity.
    pub fn new_ship(name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity,
            acceleration,
            kind: EntityKind::Ship,
        }
    }

    pub fn new_planet(
        name: String,
        position: Vec3,
        color: String,
        primary: Option<String>,
        primary_ptr: Option<Arc<RwLock<Entity>>>,
        radius: f64,
        mass: f64,
        dependency: i32,
    ) -> Self {
        Entity {
            name,
            position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
            kind: EntityKind::Planet {
                color,
                primary,
                primary_ptr,
                radius,
                mass,
                dependency,
            },
        }
    }

    pub fn new_missile(
        name: String,
        source: String,
        target: String,
        target_ptr: Arc<RwLock<Entity>>,
        position: Vec3,
        velocity: Vec3,
        burns: i32,
    ) -> Self {
        Entity {
            name,
            position,
            velocity,
            acceleration: Vec3::zero(),
            kind: EntityKind::Missile {
                source,
                target,
                target_ptr: Some(target_ptr),
                burns,
            },
        }
    }

    // Method to get the name of the entity.
    #[allow(dead_code)]
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    // Method to set the acceleration of the entity.
    pub fn set_acceleration(&mut self, acceleration: Vec3) {
        match self.kind {
            EntityKind::Planet { .. } => {
                panic!("Cannot set acceleration on Planet {:?}", self.name)
            }
            _ => self.acceleration = acceleration,
        }
    }

    pub fn get_position(&self) -> Vec3 {
        self.position
    }

    pub fn get_velocity(&self) -> Vec3 {
        self.velocity
    }

    // Method to update the position of the entity based on its velocity and acceleration.
    pub fn update(&mut self) -> Option<UpdateAction> {
        match &mut self.kind {
            EntityKind::Ship => {
                let old_velocity: Vec3 = self.velocity;
                self.velocity += self.acceleration * G * DELTA_TIME as f64;
                self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
                None
            }
            EntityKind::Planet { primary_ptr, .. } => {
                // This is the Gravitational Constant, not the acceleration due to gravity which is defined as G and used
                // more widely in this codebase.
                const G_CONST: f64 = 6.673e-11;

                if let Some(primary) = primary_ptr {
                    let primary = primary.read().unwrap();
                    let primary_mass = if let EntityKind::Planet {
                        mass: primary_mass, ..
                    } = primary.kind
                    {
                        primary_mass
                    } else {
                        unreachable!();
                    };

                    let primary_position = primary.position;
                    let primary_velocity = primary.velocity;

                    // We assume orbits are just on the x, z plane and around the primary.
                    let orbit_radius =
                        Vec3::new(1.0, 0.0, 1.0).mul_element_wise(self.position - primary_position);
                    let speed = (G_CONST * primary_mass / orbit_radius.magnitude()).sqrt();

                    debug!(
                        "Planet {} orbit radius: {:?}, radius magnitude {:?}, speed {:?}",
                        self.name,
                        orbit_radius,
                        orbit_radius.magnitude(),
                        speed
                    );

                    // UG! If I keep adding in the primary's velocity it won't work as I need to subtract what it was.
                    // Okay, try this - don't include this velocity in self.velocity. Instead add it this one time only into
                    // the position.
                    let old_velocity = self.velocity;
                    let tangent = Vec3::new(-orbit_radius.z, 0.0, orbit_radius.x).normalize();

                    self.velocity = tangent * speed;
                    debug!("Planet {} velocity: {:?}", self.name, self.velocity);

                    // Now that we have velocity, move the planet!
                    self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64
                        + primary_velocity * DELTA_TIME as f64;
                }
                None
            }
            EntityKind::Missile {
                target_ptr, burns, ..
            } => {
                // Using unwrap() below as it is an error condition if for some reason the target_ptr isn't set.
                let target = target_ptr.as_ref().unwrap().read().unwrap();
                if *burns > 0 {
                    // Temporary until missiles have actual acceleration built in
                    const MAX_ACCELERATION: f64 = 6.0*G;
                    const IMPACT_DISTANCE: f64 = 2500000.0;

                    debug!(
                        "Computing path for missile {} targeting {}: End pos: {:?} End vel: {:?}",
                        self.name, target.name, target.position, target.velocity
                    );

                    let params = TargetParams::new(
                        self.position,
                        target.position,
                        self.velocity,
                        target.velocity,
                        MAX_ACCELERATION,
                    );

                    debug!("Call targeting computer with params: {:?}", params);

                    let plan: FlightPlan = compute_target_path(&params);
                    debug!("Computed path: {:?}", plan);
                    self.acceleration = plan.accelerations[0].0;
                    let time = plan.accelerations[0].1.min(DELTA_TIME);

                    let old_velocity: Vec3 = self.velocity;
                    self.velocity += self.acceleration * G * time as f64;
                    self.position += (old_velocity + self.velocity) / 2.0 * time as f64;
                    *burns -= 1;

                    // See if we impacted.
                    if (self.position - target.position).magnitude() < IMPACT_DISTANCE {
                        debug!("Missile {} impacted target {}", self.name, target.name);
                        Some(UpdateAction::ShipImpact { ship: target.name.clone(), missile: self.name.clone() })
                    } else { None }
                } else {
                    debug!("Missile {} out of propellant", self.name);
                    Some(UpdateAction::ExhaustedMissile{ name: self.name.clone()})
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateAction {
    ShipImpact {
        ship: String,
        missile: String,
    },
    ExhaustedMissile {
        name: String
    }
}

#[serde_as]
#[derive(Debug, Default)]
pub struct Entities {
    entities: HashMap<String, Arc<RwLock<Entity>>>,
    missile_cnt: u16,
}

impl PartialEq for Entities {
    fn eq(&self, other: &Self) -> bool {
        self.entities.len() == other.entities.len()
            && self.entities.keys().all(|k| other.entities.contains_key(k))
            && self.entities.keys().all(|k| {
                self.entities[k]
                    .read()
                    .unwrap()
                    .eq(&other.entities[k].read().unwrap())
            })
    }
}

impl Entities {
    pub fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(file_name)?;
        let reader = std::io::BufReader::new(file);
        let mut entities: Entities = serde_json::from_reader(reader)?;
        info!("Load scenario file \"{}\".", file_name);

        entities.fixup_pointers();

        for entity in entities.entities.values() {
            debug!("Loaded entity {:?}", entity.read().unwrap());
        }
        assert!(entities.validate(), "Scenario file failed validation");
        Ok(entities)
    }

    pub fn add_ship(&mut self, name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) {
        let entity = Entity::new_ship(name, position, velocity, acceleration);
        self.add_entity(entity);
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
            "Add planet {} with primary {}",
            name,
            primary.as_ref().unwrap_or(&String::from("None"))
        );

        // FIXME: Is there a cleaner way to just assert that the primary exists and it must be of kind Planet?
        let (primary_ptr, dependency) = if let Some(primary_name) = &primary {
            // This is the case where this is a primary as noted by the fact primary (String name of primary) is not None

            // Look up the primary.  We need a pointer to this entity and then look into its dependency value.
            let entity = self.get(primary_name).unwrap_or_else(|| {
                panic!(
                    "Primary planet {} not found for planet {}.",
                    primary_name, name
                )
            });

            // Go get the dependency value.  At this point if this is a Planet its an error (cannot be a primary).
            if let EntityKind::Planet { dependency, .. } = &entity.read().unwrap().kind {
                (Some(entity.clone()), dependency + 1)
            } else {
                unreachable!();
            }
        } else {
            // If there is no primary then this is a "root" planet so has no Primary and dependency value is 0.
            (None, 0)
        };

        // Just a safety check to ensure we never have a pointer without a name of a primary or vis versa.
        assert!(
            primary_ptr.is_some() && primary.is_some()
                || primary_ptr.is_none() && primary.is_none()
        );

        let entity = Entity::new_planet(
            name,
            position,
            color,
            primary,
            primary_ptr,
            radius,
            mass,
            dependency,
        );
        self.add_entity(entity);
    }

    pub fn launch_missile(&mut self, source: String, target: String) {

        const DEFAULT_BURN: i32 = 2;

        // Could use a random number generator here for the name but that makes tests flakey (random)
        // So this counter used to distinguish missiles between the same source and target
        let id = self.missile_cnt;
        self.missile_cnt += 1;
        let name = format!("{}::{}::{:X}", source, target, id);
        let source_ptr = self
            .get(&source)
            .unwrap_or_else(|| panic!("Missile source {} not found for missile {}.", source, name))
            .clone();

        let source_entity = source_ptr.read().unwrap();

        let position = source_entity.position;
        let velocity = source_entity.velocity;

        let target_ptr = self
            .get(&target)
            .unwrap_or_else(|| panic!("Target {} not found for missile {}.", target, name))
            .clone();
        let entity =
            Entity::new_missile(name, source, target, target_ptr, position, velocity, DEFAULT_BURN);
        self.add_entity(entity);
    }

    pub fn add_entity(&mut self, entity: Entity) {
        self.entities
            .insert(entity.name.clone(), Arc::new(RwLock::new(entity)));
    }

    pub fn remove(&mut self, name: &str) {
        self.entities.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<RwLock<Entity>>> {
        self.entities.get(name)
    }

    pub fn set_acceleration(&mut self, name: &str, acceleration: Vec3) {
        if let Some(entity) = self.entities.get_mut(name) {
            entity.write().unwrap().set_acceleration(acceleration);
        } else {
            warn!(
                "Could not set acceleration for non-existent entity {}",
                name
            );
        }
    }

    pub fn update_all(&mut self) -> Vec<UpdateAction> {
        let (mut planets, other): (Vec<_>, Vec<_>) = self
            .entities
            .values()
            .partition(|e| matches!(e.read().unwrap().kind, EntityKind::Planet { .. }));

        planets.sort_by(|a, b| {
            let a_ent = a.read().unwrap();
            let b_ent = b.read().unwrap();
            match (&a_ent.kind, &b_ent.kind) {
                (
                    EntityKind::Planet {
                        dependency: dependency1,
                        ..
                    },
                    EntityKind::Planet {
                        dependency: dependency2,
                        ..
                    },
                ) => dependency1.cmp(dependency2),
                _ => unreachable!(),
            }
        });

        debug!("(Entities.update_all) Sorted planets: {:?}", planets);
        debug!("(Entities.update_all) Other: {:?}", other);

        for planet in planets.iter() {
            planet.write().unwrap().update();
        }

        let updates: Vec<_> = other.into_iter().filter_map(|entity| entity.write().unwrap().update()).collect();
        for update in updates.iter() {
            match update {
                UpdateAction::ExhaustedMissile { name } => {
                    debug!("Removing missile {}", name);
                    self.remove(&name);
                },
                UpdateAction::ShipImpact { ship, missile } => {
                    debug!("Missile impact on {} by missile {}.", ship, missile);
                    self.remove(&ship);
                    self.remove(&missile);
                }
            }
        }
        updates
    }

    pub fn validate(&self) -> bool {
        for entity in self.entities.values() {
            let ent = entity.read().unwrap();

            // Match any pattern that is invalid and return false.
            // If none match, then we return true at the end.
            match &ent.kind {
                EntityKind::Planet {
                    primary: Some(_),
                    primary_ptr: None,
                    ..
                } => return false,
                EntityKind::Planet {
                    primary: None,
                    primary_ptr: Some(_),
                    ..
                } => return false,
                EntityKind::Planet {
                    primary: Some(primary_name),
                    primary_ptr: Some(primary_ptr),
                    ..
                } => {
                    if primary_ptr.read().unwrap().name != *primary_name {
                        return false;
                    }
                }
                EntityKind::Missile {
                    target: _,
                    target_ptr: None,
                    ..
                } => return false,
                EntityKind::Missile {
                    target: target_name,
                    target_ptr: Some(target_ptr),
                    ..
                } => {
                    if target_ptr.read().unwrap().name != *target_name {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }

    pub fn fixup_pointers(&mut self) {
        for entity in self.entities.values() {
            let mut ent = entity.write().unwrap();
            let name = ent.name.clone();
            match &mut ent.kind {
                EntityKind::Planet {
                    primary: Some(primary),
                    primary_ptr,
                    ..
                } => {
                    let looked_up = self.get(primary).unwrap_or_else(|| {
                        panic!(
                            "Unable to find entity named {} as primary for {}",
                            primary, &name
                        )
                    });
                    primary_ptr.replace(looked_up.clone());
                }
                EntityKind::Missile {
                    target: target_name,
                    target_ptr,
                    ..
                } => {
                    let looked_up = self.get(target_name).unwrap_or_else(|| {
                        panic!(
                            "Unable to find entity named {} as target for {}",
                            target_name, &name
                        )
                    });
                    target_ptr.replace(looked_up.clone());
                }
                _ => {}
            }
        }
    }
}

impl Serialize for Entities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        //TODO: This makes a copy of every entity before serializing.  Not sure if there is a way to avoid it.
        let guts = self
            .entities
            .values()
            .map(|e| e.read().unwrap().clone())
            .collect::<Vec<Entity>>();
        guts.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Entities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let guts = Vec::<Entity>::deserialize(deserializer)?;
        Ok(Entities {
            entities: guts
                .into_iter()
                .map(|e| (e.name.clone(), Arc::new(RwLock::new(e))))
                .collect(),
            missile_cnt: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Vector2, Zero};

    #[test]
    fn test_add_entity() {
        let mut entities = Entities::default();

        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::zero(),
            Vec3::zero(),
        );

        assert_eq!(entities.get("Ship1").unwrap().read().unwrap().name, "Ship1");
        assert_eq!(entities.get("Ship2").unwrap().read().unwrap().name, "Ship2");
        assert_eq!(entities.get("Ship3").unwrap().read().unwrap().name, "Ship3");
    }

    #[test]
    fn test_update_all() {
        let _ = pretty_env_logger::try_init();

        let mut entities = Entities::default();

        // Create entities with random positions and names
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(1.0, 1.0, 1.0);
        let acceleration2 = Vec3::new(2.0, 2.0, -2.0);
        let acceleration3 = Vec3::new(4.0, -1.0, -0.0);
        entities.set_acceleration("Ship1", acceleration1);
        entities.set_acceleration("Ship2", acceleration2);
        entities.set_acceleration("Ship3", acceleration3);

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(44146000.0, 44147000.0, 44148000.0);
        let expected_position2 = Vec3::new(88294000.0, 88295000.0, -88284000.0);
        let expected_position3 = Vec3::new(176587000.0, -44137000.0, 9000.0);
        assert_eq!(
            entities.get("Ship1").unwrap().read().unwrap().position,
            expected_position1
        );
        assert_eq!(
            entities.get("Ship2").unwrap().read().unwrap().position,
            expected_position2
        );
        assert_eq!(
            entities.get("Ship3").unwrap().read().unwrap().position,
            expected_position3
        );
    }

    #[test]
    fn test_sun_update() {
        let _ = pretty_env_logger::try_init();

        let mut entities = Entities::default();

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
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the position remains the same
        let expected_position = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(
            entities.get("Sun").unwrap().read().unwrap().position,
            expected_position
        );
    }
    #[test]
    // TODO: Add test to add a moon.
    fn test_planet_update() {
        let _ = pretty_env_logger::try_init();

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

        let mut entities = Entities::default();

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
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // FIXME: This isn't really testing what we want to test.
        // Fix it so we have real primaries and test the distance to those.
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet1").unwrap().read().unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                2000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet2").unwrap().read().unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                5000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet3").unwrap().read().unwrap().position,
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

        let tst_planet = Entity::new_planet(
            String::from("Sun"),
            Vec3::zero(),
            String::from("yellow"),
            None,
            None,
            7e8,
            100.0,
            0,
        );

        let tst_str = serde_json::to_string(&tst_planet).unwrap();
        assert_eq!(
            tst_str,
            r#"{"name":"Sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":{"Planet":{"color":"yellow","radius":700000000.0,"mass":100.0}}}"#
        );

        let tst_planet_2 = Entity::new_planet(
            String::from("planet2"),
            Vec3::zero(),
            String::from("red"),
            Some(String::from("planet1")),
            Some(Arc::new(RwLock::new(tst_planet))),
            4e6,
            100.0,
            1,
        );

        let tst_str = serde_json::to_string(&tst_planet_2).unwrap();
        assert_eq!(
            tst_str,
            r#"{"name":"planet2","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":{"Planet":{"color":"red","primary":"planet1","radius":4000000.0,"mass":100.0}}}"#
        );

        let tst_str = r#"{"name":"planet2","position":[0,0,0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],
            "kind":{"Planet":{"color":"red","radius":1.5e8,"mass":100.0,"primary":"planet1"}}}"#;

        let tst_planet_3 = serde_json::from_str::<Entity>(tst_str).unwrap();
        assert_eq!(tst_planet_3, tst_planet_2);
    }

    #[test]
    fn test_unordered_scenario_file() {
        let _ = pretty_env_logger::try_init();

        let entities = Entities::load_from_file("./tests/test-scenario.json").unwrap();
        assert!(entities.validate(), "Scenario file failed validation");
    }
}

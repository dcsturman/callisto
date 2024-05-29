use cgmath::{ElementWise, InnerSpace, Vector3, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use serde_with::{serde_as, skip_serializing_none};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::computer::{compute_flight_path, FlightParams, FlightPlan};
use crate::payloads::Vec3asVec;

pub const DELTA_TIME: i32 = 1000;
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
            position: position,
            velocity: velocity,
            acceleration: acceleration,
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
            position: position,
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
        position: Vec3,
        target: String,
        target_ptr: Arc<RwLock<Entity>>,
        burns: i32,
    ) -> Self {
        Entity {
            name,
            position: position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
            kind: EntityKind::Missile {
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
    pub fn update(&mut self) {
        match &mut self.kind {
            EntityKind::Ship => {
                let old_velocity: Vec3 = self.velocity;
                self.velocity += self.acceleration * G * DELTA_TIME as f64;
                self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
            }
            EntityKind::Planet {
                primary_ptr,
                ..
            } => {
                // This is the Gravitational Constant, not the acceleration due to gravity which is defined as G and used
                // more widely in this codebase.
                const G_CONST: f64 = 6.673e-11;

                if let Some(primary) = primary_ptr {
                    let primary = primary.read().unwrap();
                    let primary_mass = if let EntityKind::Planet {
                        mass: primary_mass,
                        ..
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
            }
            EntityKind::Missile {
                target: _,
                target_ptr,
                burns,
            } => {
                // Using unwrap() below as it is an error condition if for some reason the target_ptr isn't set.
                let target = target_ptr.as_ref().unwrap().read().unwrap();
                if *burns > 0 {
                    // Temporary until missiles have actual acceleration built in
                    const MAX_ACCELERATION: f64 = 6.0;

                    debug!(
                        "Computing path for missile {} targeting {}: End pos: {:?} End vel: {:?}",
                        self.name, target.name, target.position, target.velocity
                    );

                    let params = FlightParams::new(
                        self.position,
                        target.position,
                        self.velocity,
                        target.velocity,
                        MAX_ACCELERATION,
                    );

                    debug!("Call computer with params: {:?}", params);

                    let plan: FlightPlan = compute_flight_path(&params);
                    debug!("Computed path: {:?}", plan);
                    self.acceleration = plan.accelerations[0].0;
                    let old_velocity: Vec3 = self.velocity;
                    self.velocity += self.acceleration * G * DELTA_TIME as f64;
                    self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
                    *burns -= 1;
                } else {
                    debug!("Missile {} out of propellant", self.name);
                };
                if *burns <= 0 {
                    // Maybe someday do something here.
                    //entities.remove(&self.name);
                }
            }
        }
    }
}

#[serde_as]
#[derive(Debug)]
pub struct Entities(HashMap<String, Arc<RwLock<Entity>>>);

impl PartialEq for Entities {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self.0.keys().all(|k| other.0.contains_key(k))
            && self
                .0
                .keys()
                .all(|k| self.0[k].read().unwrap().eq(&other.0[k].read().unwrap()))
    }
}

impl Entities {
    pub fn new() -> Self {
        Entities(HashMap::new())
    }

    pub fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(file_name)?;
        let reader = std::io::BufReader::new(file);
        let mut entities: Entities = serde_json::from_reader(reader)?;
        info!("Load scenario file \"{}\".", file_name);

        entities.fixup_pointers();

        for entity in entities.0.values() {
            debug!("Loaded entity {:?}", entity.read().unwrap());
        }
        assert!(entities.validate(),"Scenario file failed validation");
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
            let entity = self.get(&primary_name).expect(
                format!(
                    "Primary planet {} not found for planet {}.",
                    primary_name, name
                )
                .as_str(),
            );

            // Go get the dependency value.  At this point if this is a Planet its an error (cannot be a primary).
            if let EntityKind::Planet {
                dependency,
                ..
            } = &entity.read().unwrap().kind
            {
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

    pub fn add_missile(&mut self, name: String, position: Vec3, target: String, burns: i32) {
        let target_ptr = self
            .get(&target)
            .expect(format!("Target {} not found for missile {}.", target, name).as_str())
            .clone();
        let entity = Entity::new_missile(name, position, target, target_ptr, burns);
        self.add_entity(entity);
    }

    pub fn add_entity(&mut self, entity: Entity) {
        self.0
            .insert(entity.name.clone(), Arc::new(RwLock::new(entity)));
    }

    pub fn remove(&mut self, name: &str) {
        self.0.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<RwLock<Entity>>> {
        self.0.get(name)
    }

    pub fn set_acceleration(&mut self, name: &str, acceleration: Vec3) {
        if let Some(entity) = self.0.get_mut(name) {
            entity.write().unwrap().set_acceleration(acceleration);
        } else {
            warn!(
                "Could not set acceleration for non-existent entity {}",
                name
            );
        }
    }

    pub fn update_all(&mut self) {
        let (mut planets, other): (Vec<_>, Vec<_>) = self.0.values().partition(|e| {
            let ent = e.read().unwrap();
            if let EntityKind::Planet { .. } = ent.kind {
                true
            } else {
                false
            }
        });

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

        for entity in other.iter() {
            entity.write().unwrap().update();
        }
    }

    pub fn validate(&self) -> bool {
        for entity in self.0.values() {
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
        for entity in self.0.values() {
            let mut ent = entity.write().unwrap();
            let name = ent.name.clone();
            match &mut ent.kind {
                EntityKind::Planet {
                    primary: Some(primary),
                    primary_ptr,
                    ..
                } => {
                    let looked_up = self.get(&primary).expect(
                        format!(
                            "Unable to find entity named {} as primary for {}",
                            primary, &name
                        )
                        .as_str(),
                    );
                    primary_ptr.replace(looked_up.clone());
                }
                EntityKind::Missile {
                    target: target_name,
                    target_ptr,
                    ..
                } => {
                    let looked_up = self.get(&target_name).expect(
                        format!(
                            "Unable to find entity named {} as target for {}",
                            target_name, &name
                        )
                        .as_str(),
                    );
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
            .0
            .values()
            .map(|e| e.read().unwrap().clone())
            .collect::<Vec<Entity>>();
        guts.serialize(serializer) // This is a bit of a hack, but it works.
    }
}

impl<'de> Deserialize<'de> for Entities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let guts = Vec::<Entity>::deserialize(deserializer)?;
        Ok(Entities(
            guts.into_iter()
                .map(|e| (e.name.clone(), Arc::new(RwLock::new(e))))
                .collect(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Vector2, Zero};

    #[test]
    fn test_add_entity() {
        let mut entities = Entities::new();

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

        let mut entities = Entities::new();

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

use cgmath::{ElementWise, InnerSpace, Vector3, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Mutex, Weak};

use crate::payloads::Vec3asVec;
use crate::computer::{compute_flight_path, FlightParams, FlightPlan};

pub const DELTA_TIME: i32 = 1000;
pub const G: f64 = 9.81;

pub type Vec3 = Vector3<f64>;

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EntityKind {
    Ship,
    Planet {
        color: String,
        #[serde_as(as = "Vec3asVec")]
        primary: Vec3,
        radius: f64,
        mass: f64,
    },
    Missile {
        target: String,
        burns: i32,
        #[serde(skip)]
        entities: Weak<Mutex<Entities>>,
    },
}

impl PartialEq for EntityKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (EntityKind::Ship, EntityKind::Ship) => true,
            (
                EntityKind::Planet {
                    color: color1,
                    primary: primary1,
                    radius: radius1,
                    mass: mass1,
                },
                EntityKind::Planet {
                    color: color2,
                    primary: primary2,
                    radius: radius2,
                    mass: mass2,
                },
            ) => color1 == color2 && primary1 == primary2 && mass1 == mass2 && radius1 == radius2,
            (
                EntityKind::Missile {
                    target: target1,
                    burns: burns1,
                    entities: _,
                },
                EntityKind::Missile {
                    target: target2,
                    burns: burns2,
                    entities: _,
                },
            ) => target1 == target2 && burns1 == burns2,
            _ => false,
        }
    }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Entity {
    name: String,
    #[serde_as(as="Vec3asVec")]
    position: Vec3,
    #[serde_as(as="Vec3asVec")]
    velocity: Vec3,
    #[serde_as(as="Vec3asVec")]
    acceleration: Vec3,
    pub kind: EntityKind,
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
        primary: Vec3,
        radius: f64,
        mass: f64,
    ) -> Self {
        Entity {
            name,
            position: position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
            kind: EntityKind::Planet {
                color,
                primary,
                radius,
                mass,
            },
        }
    }

    pub fn new_missile(
        name: String,
        position: Vec3,
        target: String,
        burns: i32,
        entities: Weak<Mutex<Entities>>,
    ) -> Self {
        Entity {
            name,
            position: position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
            kind: EntityKind::Missile {
                target,
                burns,
                entities,
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
        match &self.kind {
            EntityKind::Ship => {
                let old_velocity: Vec3 = self.velocity;
                self.velocity += self.acceleration * G * DELTA_TIME as f64;
                self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
            }
            EntityKind::Planet {
                color: _,
                primary,
                radius: _,
                mass: _,
            } => {
                let old_velocity = self.velocity;
                // We assume orbits are just on the x, z plane and around the primary.
                // This is the Gravitational Constant, not the acceleration due to gravity.
                const G_CONST: f64 = 6.673e-11;
                // For now assume every start is the mass of the Sun. Will change in future.
                const SUN_MASS: f64 = 1.989e30;

                let radius = Vec3::new(1.0, 0.0, 1.0).mul_element_wise(self.position - primary);
                debug!("Planet {} radius: {:?}", self.name, radius);

                debug!(
                    "Planet {} radius magnitude: {:?}",
                    self.name,
                    radius.magnitude()
                );

                let radius_length = radius.magnitude();
                let speed = if radius_length == 0.0 {
                    0.0
                } else {
                    (G_CONST * SUN_MASS / radius.magnitude()).sqrt()
                };
                debug!("Planet {} speed: {:?}", self.name, speed);

                let tangent = if radius_length == 0.0 {
                    Vec3::zero()
                } else {
                    Vec3::new(-radius.z, 0.0, radius.x).normalize()
                };
                debug!("Planet {} tangent: {:?}", self.name, tangent);

                self.velocity = tangent * speed;
                debug!("Planet {} velocity: {:?}", self.name, self.velocity);

                // Now that we have velocity, move the planet!
                self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
            }
            EntityKind::Missile {
                target,
                burns,
                entities,
            } => {
                if let Some(entities) = entities.upgrade() {
                    let mut entities = entities.lock().unwrap();
                    if let Some(target) = entities.get(target) {
                        if *burns > 0 {
                            // Temporary until missiles have actual acceleration built in
                            const MAX_ACCELERATION: f64 = 6.0;

                            debug!("Computing path for missile {} targeting {}: End pos: {:?} End vel: {:?}", self.name, target.name, target.position, target.velocity);

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
                            self.position +=
                                (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
                        } else {
                            debug!("Missile {} out of propellant", self.name);
                            entities.remove(&self.name);
                        }
                    }
                }
            }
        }
    }
}

#[serde_as]
#[derive(Debug, PartialEq)]
pub struct Entities(HashMap<String, Entity>);

impl Entities {
    pub fn new() -> Self {
        Entities (HashMap::new())
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
        primary: Vec3,
        radius: f64,
        mass: f64,
    ) {
        let entity = Entity::new_planet(name, position, color, primary, radius, mass);
        self.add_entity(entity);
    }

    pub fn add_missile(
        &mut self,
        name: String,
        position: Vec3,
        target: String,
        burns: i32,
        entities: Weak<Mutex<Entities>>,
    ) {
        let entity = Entity::new_missile(name, position, target, burns, entities);
        self.add_entity(entity);
    }

    pub fn add_entity(&mut self, entity: Entity) {
        self.0.insert(entity.name.clone(), entity);
    }

    pub fn remove(&mut self, name: &str) {
        self.0.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Entity> {
        self.0.get(name)
    }

    pub fn set_acceleration(&mut self, name: &str, acceleration: Vec3) {
        if let Some(entity) = self.0.get_mut(name) {
            entity.set_acceleration(acceleration);
        }
    }

    pub fn update_all(&mut self) {
        for entity in self.0.values_mut() {
            entity.update();
        }
    }
}

impl Serialize for Entities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let guts = self.0.values().collect::<Vec<&Entity>>();
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
            guts.into_iter().map(|e| (e.name.clone(), e)).collect(),
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

        assert_eq!(entities.get("Ship1").unwrap().name, "Ship1");
        assert_eq!(entities.get("Ship2").unwrap().name, "Ship2");
        assert_eq!(entities.get("Ship3").unwrap().name, "Ship3");
    }

    #[test]
    fn test_update_all() {
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
        assert_eq!(entities.get("Ship1").unwrap().position, expected_position1);
        assert_eq!(entities.get("Ship2").unwrap().position, expected_position2);
        assert_eq!(entities.get("Ship3").unwrap().position, expected_position3);
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
            Vec3::zero(),
            15.371e8,
            6e24,
        );

        // Update the planet a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the position remains the same
        let expected_position = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(entities.get("Sun").unwrap().position, expected_position);
    }
    #[test]
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
            Vec3::zero(),
            1.5e8,
            6e24,
        );
        entities.add_planet(
            String::from("Planet2"),
            Vec3::new(0.0, 5000000.0, EARTH_RADIUS),
            String::from("red"),
            Vec3::zero(),
            1.5e8,
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
            Vec3::zero(),
            1.5e8,
            1e26,
        );

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet1").unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                2000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet2").unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                5000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet3").unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                8000.0
            )
        );
    }
}

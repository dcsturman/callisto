use cgmath::{Vector3, Zero};
use serde::{Deserialize, Serialize};
use serde::ser::SerializeStruct;

use serde_json;
use std::collections::HashMap;

pub const DELTA_TIME: i32 = 1000;
pub const G: f64 = 9.81;

pub type Vec3 = Vector3<f64>;

#[derive(Deserialize, Debug)]
pub struct Entity {
    name: String,
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
}

impl Entity {
    // Most common constructor for an Entity.  Give it a name and a position in space.
    pub fn new(name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity,
            acceleration,
        }
    }

    // More flexible constructor that also allows an initial velocity to be set.
    pub fn new_with_pos(name: String, position: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
        }
    }

    // Method to set the acceleration of the entity.
    pub fn set_acceleration(&mut self, acceleration: Vec3) {
        self.acceleration = acceleration;
    }

    // Method to update the position of the entity based on its velocity and acceleration.
    pub fn update(&mut self) {
        let old_velocity = self.velocity;
        self.velocity += self.acceleration * G * DELTA_TIME as f64;
        self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
    }
}

impl Serialize for Entity {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::ser::Serializer,
    {
        let mut state = serializer.serialize_struct("Entity", 4)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("position", &vec![self.position.x, self.position.y, self.position.z])?;
        state.serialize_field("velocity", &vec![self.velocity.x, self.velocity.y, self.velocity.z])?;
        state.serialize_field("acceleration", &vec![self.acceleration.x, self.acceleration.y, self.acceleration.z])?;
        state.end()
    }
}
#[derive(Serialize, Deserialize, Debug)]
pub struct Entities {
    entities: HashMap<String, Entity>,
}

impl Entities {
    pub fn new() -> Self {
        Entities {
            entities: HashMap::new(),
        }
    }

    pub fn add(&mut self, name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) {
        let entity = Entity::new(name, position, velocity, acceleration);
        self.entities.insert(entity.name.clone(), entity);
    }

    pub fn remove(&mut self, name: &str) {
        self.entities.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Entity> {
        self.entities.get(name)
    }

    pub fn set_acceleration(&mut self, name: &str, acceleration: Vec3) {
        if let Some(entity) = self.entities.get_mut(name) {
            entity.set_acceleration(acceleration);
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        let guts = self.entities.values().collect::<Vec<&Entity>>();
        serde_json::to_string(&guts)
    }

    pub fn update_all(&mut self) {
        for entity in self.entities.values_mut() {
            entity.update();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_entity() {
        let mut entities = Entities::new();

        entities.add(String::from("Entity1"), Vec3::new(1.0, 2.0, 3.0), Vec3::zero(), Vec3::zero());
        entities.add(String::from("Entity2"), Vec3::new(4.0, 5.0, 6.0), Vec3::zero(), Vec3::zero());
        entities.add(String::from("Entity3"), Vec3::new(7.0, 8.0, 9.0), Vec3::zero(), Vec3::zero());

        assert_eq!(entities.get("Entity1").unwrap().name, "Entity1");
        assert_eq!(entities.get("Entity2").unwrap().name, "Entity2");
        assert_eq!(entities.get("Entity3").unwrap().name, "Entity3");
    }

    #[test]
    fn test_update_all() {
        let mut entities = Entities::new();

        // Create entities with random positions and names
        entities.add(String::from("Entity1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero()
        );
        entities.add(String::from("Entity2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero()
        );
        entities.add(String::from("Entity3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero()
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(1.0, 1.0, 1.0);
        let acceleration2 = Vec3::new(2.0, 2.0, -2.0);
        let acceleration3 = Vec3::new(4.0, -1.0, -0.0);
        entities.set_acceleration("Entity1", acceleration1);
        entities.set_acceleration("Entity2", acceleration2);
        entities.set_acceleration("Entity3", acceleration3);

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(44146000.0, 44147000.0, 44148000.0);
        let expected_position2 = Vec3::new(88294000.0, 88295000.0, -88284000.0);
        let expected_position3 = Vec3::new(176587000.0, -44137000.0, 9000.0);
        assert_eq!(
            entities.get("Entity1").unwrap().position,
            expected_position1
        );
        assert_eq!(
            entities.get("Entity2").unwrap().position,
            expected_position2
        );
        assert_eq!(
            entities.get("Entity3").unwrap().position,
            expected_position3
        );
    }
}

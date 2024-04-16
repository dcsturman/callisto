use cgmath::{Vector3, Zero};
use serde::{Deserialize, Serialize};
use serde_json;
use std::collections::HashMap;

pub const DELTA_TIME: i32 = 1000;

pub type Vec3 = Vector3<f64>;

#[derive(Serialize, Deserialize, Debug)]
pub struct Entity {
    name: String,
    position: Vec3,
    velocity: Vec3,
    acceleration: Vec3,
}

impl Entity {
    // Most common constructor for an Entity.  Give it a name and a position in space.
    pub fn new(name: String, position: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
        }
    }

    // More flexible constructor that also allows an initial velocity to be set.
    pub fn new_with_velocity(name: String, position: Vec3, velocity: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity,
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
        self.velocity += self.acceleration * DELTA_TIME as f64;
        self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
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

    pub fn add(&mut self, name: String, position: Vec3) {
        let entity = Entity::new(name, position);
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
        serde_json::to_string(self)
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

        entities.add(String::from("Entity1"), Vec3::new(1.0, 2.0, 3.0));
        entities.add(String::from("Entity2"), Vec3::new(4.0, 5.0, 6.0));
        entities.add(String::from("Entity3"), Vec3::new(7.0, 8.0, 9.0));

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
        );
        entities.add(String::from("Entity2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
        );
        entities.add(String::from("Entity3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(0.1, 0.2, 0.3);
        let acceleration2 = Vec3::new(0.4, 0.5, 0.6);
        let acceleration3 = Vec3::new(0.7, 0.8, 0.9);
        entities.set_acceleration("Entity1", acceleration1);
        entities.set_acceleration("Entity2", acceleration2);
        entities.set_acceleration("Entity3", acceleration3);

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(451000.0, 902000.0, 1353000.0);
        let expected_position2 = Vec3::new(1804000.0, 2255000.0, 2706000.0);
        let expected_position3 = Vec3::new(3157000.0, 3608000.0, 4059000.0);
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

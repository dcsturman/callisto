use std::sync::{Arc, RwLock};

use cgmath::{ElementWise, InnerSpace, Zero};
use serde_with::{serde_as, skip_serializing_none};
use serde::{Deserialize, Serialize};
use derivative::Derivative;

use super::entity::{DELTA_TIME, Entity, UpdateAction, Vec3};
use super::payloads::Vec3asVec;

#[derive(Derivative)]
#[derivative(PartialEq)]
#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]

pub struct Planet {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    // Any valid color string OR a string starting with "!" then referring to a special template    
    pub color: String,
    pub radius: f64,
    pub mass: f64,
    #[serde(default)]
    pub primary: Option<String>,

    // TODO: Ideally this would be in the primary structure rather than also be an Option outside it.  But I right now
    // can't figure out how to skip serde for a portion of a tuple.
    #[serde(skip)]
    #[derivative(PartialEq = "ignore")]
    pub primary_ptr: Option<Arc<RwLock<Planet>>>,

    // Dependency is used to enforce order of update.  Lower values (e.g. a star with value 0) are updated first.
            // Not needed to be passed in JSON to the client; not needed for comparison operations.
    #[serde(skip)]
    pub dependency: i32,

}

impl Planet {
    #[allow(clippy::too_many_arguments)]    
    pub fn new(name: String, position: Vec3, color: String, radius: f64, mass: f64, primary: Option<String>, primary_ptr: Option<Arc<RwLock<Planet>>>, dependency: i32) -> Self {
        Planet {
            name,
            position,
            velocity: Vec3::zero(),
            color,
            radius,
            mass,
            primary,
            primary_ptr,
            dependency,
        }
    }
}

impl Entity for Planet {
    fn get_name(&self) -> &str {
        &self.name
    }

    fn set_name(&mut self, name: String) {
        self.name = name;
    }

    fn get_position(&self) -> Vec3 {
        self.position
    }

    fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    fn get_velocity(&self) -> Vec3 {
        self.velocity
    }

    fn set_velocity(&mut self, velocity: Vec3) {
        self.velocity = velocity;
    }

    fn update(&mut self) -> Option<UpdateAction> {
        debug!("Updating planet {:?}", self.name);
        // This is the Gravitational Constant, not the acceleration due to gravity which is defined as G and used
        // more widely in this codebase.
        const G_CONST: f64 = 6.673e-11;

        if let Some(primary) = &self.primary_ptr {
            let primary = primary.read().unwrap();

            // We assume orbits are just on the x, z plane and around the primary.
            let orbit_radius =
                Vec3::new(1.0, 0.0, 1.0).mul_element_wise(self.position - primary.get_position());
            let speed = (G_CONST * primary.mass / orbit_radius.magnitude()).sqrt();

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

            self.velocity = tangent * speed + primary.get_velocity();
            debug!("Planet {} velocity: {:?}", self.name, self.velocity);

            // Now that we have velocity, move the planet!
            self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
        }
        None
    }
}
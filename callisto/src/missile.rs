use std::sync::{Arc, RwLock};

use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use cgmath::InnerSpace;

use crate::computer::{compute_target_path, FlightPathResult, TargetParams};
use crate::entity::{Entity, UpdateAction, Vec3, DELTA_TIME, G};
use crate::payloads::Vec3asVec;
use crate::ship::Ship;
use crate::debug;

// Temporary until missiles have actual acceleration built in
const MAX_MISSILE_ACCELERATION: f64 = 10.0 * G;
pub const DEFAULT_BURN: i32 = 10;
pub const IMPACT_DISTANCE: f64 = 25000.0;

#[derive(Derivative)]
#[derivative(PartialEq)]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
#[skip_serializing_none]
pub struct Missile {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    pub source: String,
    pub target: String,
    #[serde(skip)]
    #[derivative(PartialEq = "ignore")]
    pub target_ptr: Option<Arc<RwLock<Ship>>>,
    #[serde_as(as = "Vec3asVec")]
    pub acceleration: Vec3,
    pub burns: i32,
}

impl Missile {
    pub fn new(
        name: String,
        source: String,
        target: String,
        target_ptr: Arc<RwLock<Ship>>,
        position: Vec3,
        velocity: Vec3,
        burns: i32,
    ) -> Self {
        // We need to construct an initial route for the missile primarily so
        // it can be shown in the UX once creation of the missile returns.
        let target_pos = target_ptr.read().unwrap().get_position();
        let target_vel = target_ptr.read().unwrap().get_velocity();

        let params = TargetParams::new(
            position,
            target_pos,
            velocity,
            target_vel,
            MAX_MISSILE_ACCELERATION,
        );

        debug!(
            "(Missile.new) Creating initial missile acceleration and calling targeting computer for missile {} with params: {:?}",
            name, params
        );

        let path: FlightPathResult = compute_target_path(&params);
        let acceleration = path.plan.0 .0;
        Missile {
            name,
            position,
            velocity,
            source,
            target,
            target_ptr: Some(target_ptr),
            acceleration,
            burns,
        }
    }
}

impl Entity for Missile {
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
        debug!("Updating missile {:?}", self.name);
        // Using unwrap() below as it is an error condition if for some reason the target_ptr isn't set.
        let target = self.target_ptr.as_ref().unwrap().read().unwrap();
        if self.burns > 0 {
            debug!(
                "Computing path for missile {} targeting {}: End pos: {:?} End vel: {:?}",
                self.name,
                target.get_name(),
                target.get_position(),
                target.get_velocity()
            );

            let params = TargetParams::new(
                self.position,
                target.get_position(),
                self.velocity,
                target.get_velocity(),
                MAX_MISSILE_ACCELERATION,
            );

            debug!(
                "Call targeting computer for missile {} with params: {:?}",
                self.name, params
            );

            let mut path: FlightPathResult = compute_target_path(&params);
            debug!("Computed path: {:?} with expected time to impact of {} turns.", path, path.path.len()-1);

            // The computed path should be an acceleration towards the target.
            // For a missile, we should always have a single acceleration (towards the target at full thrust).
            // It might not be for full DELTA_TIME but that is okay.
            // We don't actually save the path anywhere as we will recompute each round.
            // We do save the current acceleration just for display purposes.
            // In the future its possible to have "dumb missiles" in which case we'll need to treat this
            // as a precomputed path instead.
            let next = path.plan.advance_time(DELTA_TIME);

            assert!(
                !next.has_second(),
                "Missile {} has more than one acceleration.",
                self.name
            );

            // This is only safe because of the assertion above.
            let (accel, time) = next.0.into();
            self.acceleration = accel;

            let old_velocity: Vec3 = self.velocity;
            self.velocity += accel * G * time as f64;
            self.position += (old_velocity + self.velocity) / 2.0 * time as f64;
            self.burns -= 1;

            // See if we impacted.
            if (self.position - target.get_position()).magnitude() < IMPACT_DISTANCE {
                debug!(
                    "Missile {} impacted target {}",
                    self.name,
                    target.get_name()
                );
                Some(UpdateAction::ShipImpact {
                    ship: target.get_name().to_string(),
                    missile: self.name.clone(),
                })
            } else {
                None
            }
        } else {
            debug!("Missile {} out of propellant", self.name);
            Some(UpdateAction::ExhaustedMissile {
                name: self.name.clone(),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Vec3;
    use crate::ship::{FlightPlan, Ship, ShipDesignTemplate};
    use cgmath::Zero;
    use std::sync::{Arc, RwLock};

    #[test_log::test]
    fn test_missile_basics() {
        let _ = pretty_env_logger::try_init();
        let mut missile = Missile::new(
            String::from("missile1"),
            String::from("source1"),
            String::from("target1"),
            Arc::new(RwLock::new(Ship::new(
                String::from("target1"),
                Vec3::zero(),
                Vec3::zero(),
                FlightPlan::default(),
                Arc::new(ShipDesignTemplate::default())
            ))),
            Vec3::zero(),
            Vec3::zero(),
            100,
        );
        assert_eq!(missile.get_name(), "missile1");
        assert_eq!(missile.get_position(), Vec3::zero());
        assert_eq!(missile.get_velocity(), Vec3::zero());
        missile.set_name("missile2".to_string());
        missile.set_position(Vec3::new(1000.0, 2000.0, 3000.0));
        missile.set_velocity(Vec3::new(4000.0, 5000.0, 6000.0));
        assert_eq!(missile.get_name(), "missile2");
        assert_eq!(missile.get_position(), Vec3::new(1000.0, 2000.0, 3000.0));
        assert_eq!(missile.get_velocity(), Vec3::new(4000.0, 5000.0, 6000.0));
    }
}

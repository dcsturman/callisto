use std::sync::{Arc, RwLock};

use cgmath::{InnerSpace, Zero};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};

use crate::computer::TargetParams;
use crate::entity::{Entity, UpdateAction, Vec3, DELTA_TIME, DELTA_TIME_F64, G};
use crate::payloads::Vec3asVec;
use crate::ship::Ship;
use crate::{debug, error, info};

// Temporary until missiles have actual acceleration built in
const MAX_MISSILE_ACCELERATION: f64 = 10.0 * G;
pub const DEFAULT_BURN: i32 = 10;
pub const IMPACT_DISTANCE: f64 = 250_000.0;

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
  /// Constructor to create a new missile.
  ///
  /// # Panics
  ///
  /// Panics if the lock cannot be obtained to read the target ship.
  pub fn new(
    name: String, source: String, target: String, target_ptr: Arc<RwLock<Ship>>, position: Vec3, velocity: Vec3,
    burns: i32,
  ) -> Self {
    // We need to construct an initial route for the missile primarily so
    // it can be shown in the UX once creation of the missile returns.
    let target_pos = target_ptr.read().unwrap().get_position();
    let target_vel = target_ptr.read().unwrap().get_velocity();
    let target_accel = target_ptr.read().unwrap().get_acceleration();

    let params = TargetParams::new(
      position,
      target_pos,
      velocity,
      target_vel,
      target_accel,
      MAX_MISSILE_ACCELERATION,
    );

    debug!(
            "(Missile.new) Creating initial missile acceleration and calling targeting computer for missile {} with params: {:?}",
            name, params
        );

    let acceleration = if let Some(path) = params.compute_target_path() {
      path.plan.0 .0
    } else {
      Vec3::zero()
    };

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
        "(update) Computing path for missile {} targeting {}: End pos: {:0.0?} End vel: {:0.0?}",
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
        target.get_acceleration(),
        MAX_MISSILE_ACCELERATION,
      );

      debug!(
        "(update) Call targeting computer for missile {} with params: {:0.0?}",
        self.name, params
      );

      let Some(mut path) = params.compute_target_path() else {
        error!("(update) Unable to compute path for missile {}", self.name);
        return Some(UpdateAction::ExhaustedMissile {
          name: self.name.clone(),
        });
      };

      debug!(
        "(update) Computed path: {:?} with expected time to impact of {} turns.",
        path,
        path.path.len() - 1
      );

      // The computed path should be an acceleration towards the target.
      // For a missile, we should always have a single acceleration (towards the target at full thrust).
      // It might not be for full DELTA_TIME but that is okay.
      // We don't actually save the path anywhere as we will recompute each round.
      // We do save the current acceleration just for display purposes.
      let next = path.plan.advance_time(DELTA_TIME);

      assert!(
        !next.has_second(),
        "(missile.update) Missile {} has more than one acceleration.",
        self.name
      );

      // This is only safe because of the assertion above.
      let (accel, time) = next.0.into();
      self.acceleration = accel;

      let old_velocity: Vec3 = self.velocity;

      // Not ideal but we'll take the precision loss here in the case where
      // time is very large.
      #[allow(clippy::cast_precision_loss)]
      let time = time as f64;
      self.velocity += accel * time;
      self.position += (old_velocity + self.velocity) / 2.0 * time;
      self.burns -= 1;

      // See if we impacted.
      debug!(
        "(update) Missile {} is {:0.0} away from target {}",
        self.name,
        (self.position - target.get_position()).magnitude(),
        target.get_name()
      );

      // If our simulation went tick by tick the following guard would be correct.
      // But we do not: we move missiles then ships.  So a missile on track to impact a ship
      // part way through a turn will miss due to the imprecision of the simulation.
      // if (self.position - target.get_position()).magnitude() < IMPACT_DISTANCE {

      // So instead we assume that if time is less than the turn length, we impact!
      if time < DELTA_TIME_F64 {
        debug!("(update) Missile {} impacted target {}", self.name, target.get_name());
        Some(UpdateAction::ShipImpact {
          ship: target.get_name().to_string(),
          missile: self.name.clone(),
        })
      } else {
        None
      }
    } else {
      info!("(update) Missile {} out of propellant", self.name);
      Some(UpdateAction::ExhaustedMissile {
        name: self.name.clone(),
      })
    }
  }
}

impl Default for Missile {
  fn default() -> Self {
    Missile {
      name: "Default Missile".to_string(),
      position: Vec3::zero(),
      velocity: Vec3::zero(),
      source: "Default Source".to_string(),
      target: "Default Target".to_string(),
      target_ptr: None,
      acceleration: Vec3::zero(),
      burns: 0,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::entity::Vec3;
  use crate::ship::{Ship, ShipDesignTemplate};
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
        &Arc::new(ShipDesignTemplate::default()),
        None,
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

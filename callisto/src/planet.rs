use std::sync::{Arc, RwLock};

use cgmath::{ElementWise, InnerSpace, Zero};
use derivative::Derivative;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};

use crate::entity::{Entity, UpdateAction, Vec3, DELTA_TIME, G};
use crate::payloads::Vec3asVec;
use crate::{debug, info};

// This is the Gravitational Constant, not the acceleration due to gravity which is defined as G and used
// more widely in this codebase.  So intentionally not "pub"
const G_CONST: f64 = 6.673e-11;

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
  #[serde(skip)]
  #[derivative(PartialEq = "ignore")]
  pub primary_ptr: Option<Arc<RwLock<Planet>>>,

  // Dependency is used to enforce order of update.  Lower values (e.g. a star with value 0) are updated first.
  // Not needed to be passed in JSON to the client; not needed for comparison operations.
  #[serde(skip)]
  #[derivative(PartialEq = "ignore")]
  pub dependency: u32,

  #[derivative(PartialEq = "ignore")]
  pub gravity_radius_2: Option<f64>,
  #[derivative(PartialEq = "ignore")]
  pub gravity_radius_1: Option<f64>,
  #[derivative(PartialEq = "ignore")]
  pub gravity_radius_05: Option<f64>,
  #[derivative(PartialEq = "ignore")]
  pub gravity_radius_025: Option<f64>,
}

fn gravity_radius(power: f64, mass: f64) -> f64 {
  const GRAVITY_CONST: f64 = 6.674e-11;
  (GRAVITY_CONST * mass / (G * power)).sqrt()
}

fn above_surface_or_none(surface: f64, distance: f64) -> Option<f64> {
  if distance < surface {
    None
  } else {
    Some(distance)
  }
}
impl Planet {
  /// Constructor to create a new planet.
  ///
  /// # Panics
  ///
  /// Panics if the lock cannot be obtained to read the primary planet.
  #[allow(clippy::too_many_arguments)]
  #[must_use]
  pub fn new(
    name: String, position: Vec3, color: String, radius: f64, mass: f64, primary: Option<String>,
    primary_ptr: &Option<Arc<RwLock<Planet>>>, dependency: u32,
  ) -> Self {
    let mut p = Planet {
      name,
      position,
      velocity: Vec3::zero(),
      color,
      radius,
      mass,
      primary,
      primary_ptr: primary_ptr.clone(),
      dependency,
      gravity_radius_2: None,
      gravity_radius_1: None,
      gravity_radius_05: None,
      gravity_radius_025: None,
    };

    p.reset_gravity_wells();
    if primary_ptr.is_some() {
      p.velocity = p.calculate_rotational_velocity().unwrap();
    }
    p
  }

  pub fn reset_gravity_wells(&mut self) {
    // Names intentionally similar but clear to author
    let gravity_radius_2 = above_surface_or_none(self.radius, gravity_radius(2.0, self.mass));
    let gravity_radius_1 = above_surface_or_none(self.radius, gravity_radius(1.0, self.mass));
    let gravity_radius_05 = above_surface_or_none(self.radius, gravity_radius(0.5, self.mass));
    let gravity_radius_025 = above_surface_or_none(self.radius, gravity_radius(0.25, self.mass));

    debug!(
      "Gravity radius 025: {:?}: given radius {:?} and gravity_radius {}",
      gravity_radius_025,
      self.radius,
      gravity_radius(0.25, self.mass)
    );
    info!(
      "(planet.reset_gravity_wells) Planet {} has gravity wells {:?}, {:?}, {:?}, {:?}",
      self.name, gravity_radius_2, gravity_radius_1, gravity_radius_05, gravity_radius_025
    );
    self.gravity_radius_2 = gravity_radius_2;
    self.gravity_radius_1 = gravity_radius_1;
    self.gravity_radius_05 = gravity_radius_05;
    self.gravity_radius_025 = gravity_radius_025;
  }

  /// Calculate the rotational velocity of the planet around its primary.
  ///
  /// # Errors
  ///
  /// Returns an error if the planet has no primary.
  ///
  /// # Panics
  ///
  /// Panics if the lock cannot be obtained to read the primary planet.
  pub fn calculate_rotational_velocity(&self) -> Result<Vec3, String> {
    // We assume orbits are just on the x, z plane and around the primary.
    let primary = self
      .primary_ptr
      .as_ref()
      .ok_or_else(|| format!("Planet {} has no primary.", self.name))?
      .read()
      .unwrap();
    let orbit_radius = Vec3::new(1.0, 0.0, 1.0).mul_element_wise(self.position - primary.get_position());
    let speed = (G_CONST * primary.mass / orbit_radius.magnitude()).sqrt();

    let tangent = Vec3::new(-orbit_radius.z, 0.0, orbit_radius.x).normalize();
    debug!("(Planet.calculate_rotational_velocity) Planet {} orbit radius = {:?}, speed = {}, tangent = {:?}, tangent*speed = {:?}",
            self.get_name(), orbit_radius, speed, tangent, tangent*speed);

    Ok(tangent * speed + primary.get_velocity())
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
    debug!("(Planet.update) Updating planet {:?}", self.name);
    if let Some(_primary) = &self.primary_ptr {
      // To avoid error and avoid making this overly complex, we're going to go in small steps of time.
      // If this ends up being too expensive in the long run we have to find a new approach.
      const MINI_STEP: u32 = 10;

      let mut time: u64 = 0;
      let orig_velocity = self.velocity;
      let mut old_velocity;
      while time < DELTA_TIME {
        // Unwrap should be 100% safe here as we are in the "if let" statement
        old_velocity = self.velocity;
        self.velocity = self.calculate_rotational_velocity().unwrap();

        // Now that we have velocity, move the planet the average of the prior velocity and the new velocity
        self.position += (old_velocity + self.velocity) / 2.0 * f64::from(MINI_STEP);
        time += u64::from(MINI_STEP);
      }
      debug!(
        "(Planet.update) Planet {} old velocity {:?} new velocity: {:?}",
        self.name, orig_velocity, self.velocity
      );
    } else {
      debug!("(Planet.update) Planet {} has no primary.", self.name);
    }
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::entity::Vec3;

  #[test_log::test]
  fn test_get_set() {
    //A unit test to test the get and set functions.
    let mut planet = Planet::new(
      String::from("Sun"),
      Vec3::zero(),
      String::from("yellow"),
      6.96e8,
      1.989e30,
      None,
      &None,
      0,
    );

    assert_eq!(planet.get_name(), "Sun");
    assert_eq!(planet.get_position(), Vec3::zero());
    assert_eq!(planet.get_velocity(), Vec3::zero());
    planet.set_name("Sun2".to_string());
    planet.set_position(Vec3::new(1000.0, 2000.0, 3000.0));
    planet.set_velocity(Vec3::new(4000.0, 5000.0, 6000.0));
    assert_eq!(planet.get_name(), "Sun2");
    assert_eq!(planet.get_position(), Vec3::new(1000.0, 2000.0, 3000.0));
    assert_eq!(planet.get_velocity(), Vec3::new(4000.0, 5000.0, 6000.0));
  }
  #[test_log::test]
  fn test_planet_update() {
    const TARGET_DISTANCE: f64 = 1_000_000_000.0;

    let _ = pretty_env_logger::try_init();
    // Create a primary (Sun) for the planet
    let sun = Arc::new(RwLock::new(Planet::new(
      String::from("Sun"),
      Vec3::zero(),
      String::from("yellow"),
      6.96e8,
      1.989e30,
      None,
      &None,
      0,
    )));

    let mut planet = Planet::new(
      String::from("Earth"),
      Vec3::new(TARGET_DISTANCE, 0.0, 0.0),
      String::from("blue"),
      6.371e6,
      5.97e24,
      Some("Sun".to_string()),
      &Some(Arc::clone(&sun)),
      1,
    );

    // Initial position and velocity
    let initial_position = planet.get_position();
    let initial_velocity = planet.get_velocity();

    // Update the planet
    planet.update();

    // Check that the position has changed
    assert_ne!(
      planet.get_position(),
      initial_position,
      "Planet position should change after update"
    );

    let initial_position = planet.get_position();

    // Velocity doesn't actually change on first update as
    // velocity vector created on planet creation is same as on first update.
    assert_ne!(
      planet.get_velocity(),
      initial_velocity,
      "Planet velocity should change after update."
    );

    // Update the planet
    planet.update();

    // Check that the position has changed
    assert_ne!(
      planet.get_position(),
      initial_position,
      "Planet position should change after update"
    );

    // Check velocity has changed.
    assert_ne!(
      planet.get_velocity(),
      initial_velocity,
      "Planet velocity should change after second update."
    );

    // Check that the planet is still orbiting around the primary (Sun)
    let sun_position = sun.read().unwrap().get_position();
    let distance_to_sun = (planet.get_position() - sun_position).magnitude();

    println!("Distance to Sun: {distance_to_sun}");

    // The distance should be roughly constant (allowing for some numerical error)
    assert!(
      (distance_to_sun - TARGET_DISTANCE).abs() < TARGET_DISTANCE / 100.0,
      "Planet should maintain a roughly constant distance from the Sun: {}",
      (distance_to_sun - TARGET_DISTANCE).abs()
    );

    // Velocity should be perpendicular to the position vector (circular orbit). However we have plenty of error in
    // this math.  5e12 (really 0.5e13) is about half a percent error.
    let position_velocity_dot = (planet.get_position() - sun.read().unwrap().get_position()).dot(planet.get_velocity());
    assert!(
      position_velocity_dot.abs() < 5e12,
      "Velocity should be perpendicular to the position vector: {position_velocity_dot}"
    );
  }
}



use cgmath::Zero;
use serde::{Serialize, Deserialize};

use derivative::Derivative;

use serde_with::{serde_as, skip_serializing_none};
use std::fmt::Debug;

use crate::entity::{DELTA_TIME, DEFAULT_ACCEL_DURATION, G, Entity, UpdateAction, Vec3};
use crate::payloads::Vec3asVec;

#[derive(Derivative)]
#[derivative(PartialEq)]
#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Ship {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    pub plan: FlightPlan,
}

impl Ship {
    pub fn new(name: String, position: Vec3, velocity: Vec3, plan: FlightPlan) -> Self {
        Ship {
            name,
            position,
            velocity,
            plan,
        }
    }

    pub fn set_flight_plan(&mut self, new_plan: FlightPlan) {
        self.plan = new_plan;
    }
}

impl PartialOrd for Ship {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Entity for Ship {
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
        debug!("(Entity.update) Updating ship {:?}", self.name);
        if self.plan.empty() {
            // Just move at current velocity
            self.position += self.velocity * DELTA_TIME as f64;
            debug!("(Entity.update) No acceleration for {}: move at velocity {:0.0?} for time {}, position now {:0.0?}", self.name, self.velocity, DELTA_TIME, self.position);
        } else {
            let moves = self.plan.advance_time(DELTA_TIME);

            for ap in moves.iter() {
                let old_velocity: Vec3 = self.velocity;
                let (accel, duration) = ap.into();
                self.velocity += accel * G * duration as f64;
                self.position += (old_velocity + self.velocity) / 2.0 * duration as f64;
                debug!(
                    "(Entity.update) Accelerate at {:0.3?} m/s for time {}",
                    accel * G,
                    duration
                );
                debug!(
                    "(Entity.update) New velocity: {:0.0?} New position: {:0.0?}",
                    self.velocity, self.position
                );
            }
        }
        None
    }
}

/*
impl PartialEq for Ship {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.position == other.position
            && self.velocity == other.velocity
            && self.plan == other.plan
    }
}
 */

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AccelPair(#[serde_as(as = "Vec3asVec")] pub Vec3, pub u64);

impl From<(Vec3, u64)> for AccelPair {
    fn from(tuple: (Vec3, u64)) -> Self {
        AccelPair(tuple.0, tuple.1)
    }
}

impl From<AccelPair> for (Vec3, u64) {
    fn from(val: AccelPair) -> Self {
        (val.0, val.1)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FlightPlan(
    pub AccelPair,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::unwrap_or_skip"
    )]
    pub Option<AccelPair>,
);

impl FlightPlan {
    pub fn new(first: AccelPair, second: Option<AccelPair>) -> Self {
        FlightPlan(first, second)
    }

    // Constructor that creates a flight plan that just has a single acceleration.
    // We use i64::MAX to represent infinite time.
    pub fn acceleration(accel: Vec3) -> Self {
        FlightPlan((accel, DEFAULT_ACCEL_DURATION).into(), None)
    }

    pub fn set_first(&mut self, accel: Vec3, time: u64) {
        self.0 = (accel, time).into();
        self.1 = None;
    }
    pub fn set_second(&mut self, accel: Vec3, time: u64) {
        self.1 = Some((accel, time).into());
    }

    pub fn has_second(&self) -> bool {
        self.1.is_some()
    }

    pub fn duration(&self) -> u64 {
        self.0 .1 + self.1.as_ref().map(|a| a.1).unwrap_or(0)
    }

    pub fn empty(&self) -> bool {
        self.0 .1 == 0 || self.0 .0 == Vec3::zero()
    }

    pub fn advance_time(&mut self, time: u64) -> Self {
        if time < self.0 .1 {
            // If time is less than the first duration:
            // This plan: first acceleration reduced by the time
            // Return: the first acceleration for time
            self.0 .1 -= time;
            FlightPlan::new((self.0 .0, time).into(), None)
        } else if matches!(&self.1, Some(second) if time < self.0.1 + second.1) {
            // If time is between the first duration plus the second duration:
            // This plan: The second acceleration for the remaining time (duration of the entire plan less the time)
            // Return: The first acceleration for its full time, and the portion of the second acceleration up to time.
            let new_first = self.0.clone();
            let first_time = self.0 .1;
            let second = self.1.clone().unwrap();
            self.0 = (second.0, second.1 - (time - self.0 .1)).into();
            self.1 = None;
            debug!("(FlightPlan.advance_time) self: {:?} new_first: {:?} second: {:?} time: {} first_time: {}", self, new_first, second, time, first_time);
            FlightPlan::new(new_first, Some((second.0, time - first_time).into()))
        } else {
            // If time is more than first and second durations:
            // This plan: becomes a zero acceleration plan.
            // Return: the entire plan.
            let result = self.clone();
            self.0 = (Vec3::zero(), 0).into();
            self.1 = None;
            result
        }
    }

    pub fn iter(&self) -> impl Iterator<Item = AccelPair> + '_ {
        if let Some(second) = &self.1 {
            vec![self.0.clone(), second.clone()].into_iter()
        } else {
            vec![self.0.clone()].into_iter()
        }
    }
}


use cgmath::Zero;
use serde::{Deserialize, Serialize};

use derivative::Derivative;

use serde_with::{serde_as, skip_serializing_none};
use std::fmt::Debug;

use crate::entity::{Entity, UpdateAction, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME, G};
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
    #[serde_as(as = "USPasString")]
    pub usp: USP,

    // usp used to record damage (vs ideal state).
    // Is just cloned from the usp initially so never serialized or deserialized.  We may need
    // to send it explicitly in some messages.
    #[serde(skip)]
    pub current_usp: USP,

    // Need structure and hull as they are derived from USP.
    #[serde(skip)]
    pub hull: u8,
    #[serde(skip)]
    pub structure: u8,
}

impl Ship {
    pub fn new(name: String, position: Vec3, velocity: Vec3, plan: FlightPlan, usp: USP) -> Self {
        let hull = usp.hull;
        Ship {
            name,
            position,
            velocity,
            plan,
            usp: usp.clone(),
            current_usp: usp,
            hull,
            structure: hull,
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

/**
 * Structure of the USP and how to decode it:
 * Hull size (basically 1 code per 100 tons up to 2K tons)  Gives Hull points and Structure points.
 * Armor level
 * Jump level
 * Manuever level
 * Power plant level
 * Computer code
 * Crew size code
 * <dash>
 * Beam Lasers
 * Pulse Lasers
 * Particle Beam
 * Missiles
 * <skip bay weapons for now>
 * Sand
 * <skip screens for now>
 * <dash>
 * TL
 */
#[derive(Default,Debug, PartialEq, Clone)]
pub struct USP {
    pub hull: u8,
    pub armor: u8,
    pub jump: u8,
    pub maneuver: u8,
    pub powerplant: u8,
    pub computer: u8,
    pub crew: u8,
    pub beam: u8,
    pub pulse: u8,
    pub particle: u8,
    pub missile: u8,
    pub sand: u8,
    pub tl: u8,
}

const USP_LEN: usize = 13;

pub const EXAMPLE_USP: &str = "38266C2-30060-B";

fn digit_to_int(code: char) -> u8 {
    match code {
        '0' => 0,
        '1' => 1,
        '2' => 2,
        '3' => 3,
        '4' => 4,
        '5' => 5,
        '6' => 6,
        '7' => 7,
        '8' => 8,
        '9' => 9,
        'A' => 10,
        'B' => 11,
        'C' => 12,
        'D' => 13,
        'E' => 14,
        'F' => 15,
        'G' => 16,
        'H' => 17,
        'J' => 18,
        'K' => 19,
        'L' => 20,
        'M' => 21,
        'N' => 22,
        'P' => 23,
        'Q' => 24,
        'R' => 25,
        'S' => 26,
        'T' => 27,
        'U' => 28,
        'V' => 29,
        'W' => 30,
        'X' => 31,
        'Y' => 32,
        'Z' => 33,
        _ => panic!("(ship.digitToInt) Unknown code: {}", code),
    }
}

fn int_to_digit(code: u8) -> char {
    match code {
        x if x <= 9 => (x + '0' as u8) as char,
        x if x <= 17 => (x - 10 + 'A' as u8) as char,
        x if x <= 22 => (x - 18 + 'J' as u8) as char,
        x if x <= 33 => (x - 23 + 'P' as u8) as char,
        _ => panic!("(ship.intToDigit) Unknown code: {}", code),
    }
}

impl From<String> for USP {
    fn from(usp: String) -> Self {
        let mut codes = usp.chars().filter(|c| *c != '-');
        assert_eq!(codes.clone().count(), USP_LEN, "USP must be {} characters long: {}", USP_LEN, usp);
        USP {
            hull: digit_to_int(codes.next().unwrap()),
            armor: digit_to_int(codes.next().unwrap()),
            jump: digit_to_int(codes.next().unwrap()),
            maneuver: digit_to_int(codes.next().unwrap()),
            powerplant: digit_to_int(codes.next().unwrap()),
            computer: digit_to_int(codes.next().unwrap()),
            crew: digit_to_int(codes.next().unwrap()),
            beam: digit_to_int(codes.next().unwrap()),
            pulse: digit_to_int(codes.next().unwrap()),
            particle: digit_to_int(codes.next().unwrap()),
            missile: digit_to_int(codes.next().unwrap()),
            sand: digit_to_int(codes.next().unwrap()),
            tl: digit_to_int(codes.next().unwrap()),
        }
    }
}

impl From<&USP> for String {
    fn from(usp: &USP) -> Self {
        let mut result = String::new();
        result.push(int_to_digit(usp.hull));
        result.push(int_to_digit(usp.armor));
        result.push(int_to_digit(usp.jump));
        result.push(int_to_digit(usp.maneuver));
        result.push(int_to_digit(usp.powerplant));
        result.push(int_to_digit(usp.computer));
        result.push(int_to_digit(usp.crew));
        result.push('-');
        result.push(int_to_digit(usp.beam));
        result.push(int_to_digit(usp.pulse));
        result.push(int_to_digit(usp.particle));
        result.push(int_to_digit(usp.missile));
        result.push(int_to_digit(usp.sand));
        result.push('-');        
        result.push(int_to_digit(usp.tl));
        result.to_string()
    }
}

serde_with::serde_conv!(
    USPasString,
    USP,
    |usp: &USP| -> String {
        usp.into()
    },
    |s: String| -> Result<_, std::convert::Infallible> {
        Ok(USP::from(s))
    }
);

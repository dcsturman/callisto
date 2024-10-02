use cgmath::{InnerSpace, Zero};
use serde::{Deserialize, Serialize};

use serde_with::{serde_as, skip_serializing_none};
use std::fmt::Debug;

use crate::cov_util::debug;
use crate::entity::{Entity, UpdateAction, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME, G};
use crate::payloads::Vec3asVec;

#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Ship {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    pub plan: FlightPlan,
    #[serde_as(as = "USPasString")]
    pub usp: USP,
    // Structure and hull are derived from USP (2x the hull) but can change in combat.
    pub hull: u8,
    pub structure: u8,

    // usp used to record ideal state (vs damage)
    // Is just cloned from the usp initially so never serialized or deserialized.  We may need
    // to send it explicitly in some messages.
    #[serde(skip)]
    pub original_usp: USP,
}

impl Ship {
    pub fn new(name: String, position: Vec3, velocity: Vec3, plan: FlightPlan, usp: USP) -> Self {
        let hull = usp.hull * 2;
        Ship {
            name,
            position,
            velocity,
            plan,
            usp: usp.clone(),
            original_usp: usp,
            hull,
            structure: hull,
        }
    }

    pub fn set_flight_plan(&mut self, new_plan: &FlightPlan) -> Result<(), String> {
        // First validate the plan to make sure its legal.
        // Its legal as long as the magnitudes in the flight plan are less than the max of the maneuverability rating
        // and the powerplant rating.
        // We use the current maneuverability rating in case the ship took damage
        let max_accel = self.max_acceleration();
        if new_plan.0 .0.magnitude() <= max_accel {
            if let Some(second) = &new_plan.1 {
                if second.0.magnitude() <= max_accel {
                    self.plan = new_plan.clone();
                    Ok(())
                } else {
                    Err("Flight plan has second acceleration that exceeds max acceleration".to_string())
                }
            } else {
                self.plan = new_plan.clone();
                Ok(())
            }
        } else  {
            Err("Flight plan has first acceleration that exceeds max acceleration".to_string())
        }
    }

    pub fn post_deserialize(&mut self) {
        self.original_usp = self.usp.clone();
    }

    pub fn max_acceleration(&self) -> f64 {
        u8::max(self.usp.maneuver, self.usp.powerplant) as f64
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
        debug!("(Ship.update) Updating ship {:?}", self.name);

        // If our ship is blow up, just return that effect (no need to do anything else)
        if self.structure == 0 {
            debug!("(Ship.update) Ship {} is destroyed.", self.name);
            return Some(UpdateAction::ShipDestroyed);
        }

        if self.plan.empty() {
            // Just move at current velocity
            self.position += self.velocity * DELTA_TIME as f64;
            debug!("(Ship.update) No acceleration for {}: move at velocity {:0.0?} for time {}, position now {:0.0?}", self.name, self.velocity, DELTA_TIME, self.position);
        } else {
            // Adjust time in case max acceleration has changed due to combat damage.  Note this might be simplistic and require a new plan but that is up
            // to the user to notice and fix.
            let max_thrust = u8::max(self.usp.maneuver, self.usp.powerplant) as f64;
            self.plan.ensure_thrust_limit(max_thrust);
            let moves = self.plan.advance_time(DELTA_TIME);

            for ap in moves.iter() {
                let old_velocity: Vec3 = self.velocity;
                let (accel, duration) = ap.into();
                self.velocity += accel * G * duration as f64;
                self.position += (old_velocity + self.velocity) / 2.0 * duration as f64;
                debug!(
                    "(Ship.update) Accelerate at {:0.3?} m/s for time {}",
                    accel * G,
                    duration
                );
                debug!(
                    "(Ship.update) New velocity: {:0.0?} New position: {:0.0?}",
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

fn renormalize(orig: Vec3, limit: f64) -> Vec3 {
    orig / orig.magnitude() * limit
}
impl Default for FlightPlan {
    fn default() -> Self {
        FlightPlan(AccelPair(Vec3::zero(), 0), None)
    }
}

impl FlightPlan {
    pub fn new(first: AccelPair, second: Option<AccelPair>) -> Self {
        FlightPlan(first, second)
    }

    // Constructor that creates a flight plan that just has a single acceleration.
    // We use i64::MAX to represent infinite time.
    pub fn acceleration(accel: Vec3) -> Self {
        FlightPlan((accel, DEFAULT_ACCEL_DURATION).into(), None)
    }

    // When the first element is set we clear the second element.
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

    pub fn ensure_thrust_limit(&mut self, limit: f64) {
        if self.0 .0.magnitude() > limit {
            self.0 .0 = renormalize(self.0 .0, limit);
        }

        if let Some(second) = &self.1 {
            if second.0.magnitude() > limit {
                self.set_second(renormalize(second.0, limit), second.1)
            }
        }
    }

    // Modify this plan by advancing time and adjusting it based on that time.
    // i.e. the flight plan advances.
    // Returns the portion of the plan that was advanced.
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
            self.0 = (second.0, second.1 - (
                time - self.0 .1)).into();
            self.1 = None;
            debug!("(FlightPlan.advance_time) self: {:?} new_first: {:?} second: {:?} time: {} first_time: {}", self, new_first, second, time, first_time);
            FlightPlan::new(new_first, if time <= first_time { None } else { Some((second.0, time - first_time).into())})
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
 * --dash--
 * Beam Lasers
 * Pulse Lasers
 * Particle Beam
 * Missiles
 * --skip bay weapons for now--
 * Sand
 * --skip screens for now--
 * --dash--
 * TL
 */
#[derive(Default, Debug, PartialEq, Clone)]
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
        x if x <= 9 => (x + b'0') as char,
        x if x <= 17 => (x - 10 + b'A') as char,
        x if x <= 22 => (x - 18 + b'J') as char,
        x if x <= 33 => (x - 23 + b'P') as char,
        _ => panic!("(ship.intToDigit) Unknown code: {}", code),
    }
}

impl From<String> for USP {
    fn from(usp: String) -> Self {
        let mut codes = usp.chars().filter(|c| *c != '-');
        assert_eq!(
            codes.clone().count(),
            USP_LEN,
            "USP must be {} characters long: {}",
            USP_LEN,
            usp
        );
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
    |usp: &USP| -> String { usp.into() },
    |s: String| -> Result<_, std::convert::Infallible> { Ok(USP::from(s)) }
);

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::assert_ulps_eq;

    #[test]
    fn test_digit_to_int() {
        assert_eq!(digit_to_int('0'), 0);
        assert_eq!(digit_to_int('7'), 7);
        assert_eq!(digit_to_int('9'), 9);
        assert_eq!(digit_to_int('A'), 10);
        assert_eq!(digit_to_int('D'), 13);
        assert_eq!(digit_to_int('E'), 14);
        assert_eq!(digit_to_int('F'), 15);
        assert_eq!(digit_to_int('G'), 16);
        assert_eq!(digit_to_int('H'), 17);
        assert_eq!(digit_to_int('Z'), 33);

        for i in 0..5 {
            assert_eq!(
                digit_to_int(char::from_u32((i + b'J') as u32).unwrap()),
                i + 18
            );
        }

        for i in 0..10 {
            assert_eq!(
                digit_to_int(char::from_u32((i + b'P') as u32).unwrap()),
                i + 23
            );
        }
    }

    #[test]
    #[should_panic(expected = "Unknown code")]
    fn test_digit_to_int_invalid() {
        digit_to_int('I');
    }

    #[test]
    fn test_int_to_digit() {
        assert_eq!(int_to_digit(0), '0');
        assert_eq!(int_to_digit(9), '9');
        assert_eq!(int_to_digit(10), 'A');
        assert_eq!(int_to_digit(15), 'F');
        assert_eq!(int_to_digit(16), 'G');
        assert_eq!(int_to_digit(33), 'Z');
    }

    #[test]
    #[should_panic(expected = "Unknown code")]
    fn test_int_to_digit_invalid() {
        int_to_digit(34);
    }

    #[test]
    fn test_ship_setters_and_getters() {
        let initial_position = Vec3::new(0.0, 0.0, 0.0);
        let initial_velocity = Vec3::new(1.0, 1.0, 1.0);
        let initial_plan = FlightPlan::default();
        let initial_usp = "38266C2-30060-B".to_string();

        let mut ship = Ship::new(
            "TestShip".to_string(),
            initial_position,
            initial_velocity,
            initial_plan.clone(),
            initial_usp.clone().into(),
        );

        // Test initial values
        assert_eq!(ship.get_name(), "TestShip");
        assert_eq!(ship.get_position(), initial_position);
        assert_eq!(ship.get_velocity(), initial_velocity);
        assert_eq!(ship.plan, initial_plan);
        let usp_str: String = From::<&USP>::from(&ship.usp);
        assert_eq!(usp_str.as_str(), initial_usp.as_str());

        // Test setters
        let new_name = "UpdatedShip".to_string();
        let new_position = Vec3::new(10.0, 20.0, 30.0);
        let new_velocity = Vec3::new(2.0, 3.0, 4.0);
        let new_plan = FlightPlan::acceleration(Vec3::new(1.0, 1.0, 1.0));

        ship.set_name(new_name.clone());
        ship.set_position(new_position);
        ship.set_velocity(new_velocity);
        assert!(ship.set_flight_plan(&new_plan).is_ok());

        // Test updated values
        assert_eq!(ship.get_name(), new_name);
        assert_eq!(ship.get_position(), new_position);
        assert_eq!(ship.get_velocity(), new_velocity);
        assert_eq!(ship.plan, new_plan);

        // Test hull and structure
        assert_eq!(ship.hull, 6); // 2 * usp.hull (3 for '3' in the USP)
        assert_eq!(ship.structure, 6);

        // Test invalid flight plan
        let invalid_plan = FlightPlan::acceleration(Vec3::new(100.0, 100.0, 100.0)); // Assuming this exceeds max acceleration
        assert!(ship.set_flight_plan(&invalid_plan).is_err());
        assert_eq!(ship.plan, new_plan); // Plan should not have changed
    }
    #[test]
    fn test_flight_plan_set_first_and_second() {
        let mut flight_plan = FlightPlan::default();

        // Test set_first
        let accel1 = Vec3::new(1.0, 2.0, 3.0);
        let time1 = 5000;
        flight_plan.set_first(accel1, time1);

        assert_eq!(flight_plan.0 .0, accel1);
        assert_eq!(flight_plan.0 .1, time1);
        assert_eq!(flight_plan.1, None);

        // Test set_second
        let accel2 = Vec3::new(-2.0, -1.0, 0.0);
        let time2 = 3000;
        flight_plan.set_second(accel2, time2);

        assert_eq!(flight_plan.0 .0, accel1);
        assert_eq!(flight_plan.0 .1, time1);
        assert_eq!(flight_plan.1, Some(AccelPair(accel2, time2)));

        // Test overwriting first acceleration
        let new_accel1 = Vec3::new(4.0, 5.0, 6.0);
        let new_time1 = 2000;
        flight_plan.set_first(new_accel1, new_time1);

        assert_eq!(flight_plan.0 .0, new_accel1);
        assert_eq!(flight_plan.0 .1, new_time1);
        assert_eq!(flight_plan.1, None);

        // Test overwriting second acceleration
        flight_plan.set_second(accel2, time2);
        let new_accel2 = Vec3::new(-3.0, -4.0, -5.0);
        let new_time2 = 4000;
        flight_plan.set_second(new_accel2, new_time2);

        assert_eq!(flight_plan.0 .0, new_accel1);
        assert_eq!(flight_plan.0 .1, new_time1);
        assert_eq!(flight_plan.1, Some(AccelPair(new_accel2, new_time2)));
    }

    #[test]
    fn test_flight_plan_ensure_thrust_limit() {
        let mut flight_plan = FlightPlan::default();

        // Test case 1: Acceleration within limit
        let accel1 = Vec3::new(3.0, 4.0, 0.0); // magnitude 5
        let time1 = 5000;
        flight_plan.set_first(accel1, time1);
        flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0), 3000); // magnitude 3

        flight_plan.ensure_thrust_limit(6.0);

        assert_ulps_eq!(flight_plan.0.0, accel1);
        assert_eq!(flight_plan.0.1, time1);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

        // Test case 2: First acceleration exceeds limit
        let accel2 = Vec3::new(6.0, 8.0, 0.0); // magnitude 10
        flight_plan.set_first(accel2, time1);
        flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0), 3000); // magnitude 3

        flight_plan.ensure_thrust_limit(6.0);

        let expected_accel2 = accel2.normalize() * 6.0;
        assert_ulps_eq!(flight_plan.0.0, expected_accel2);
        assert_eq!(flight_plan.0.1, time1);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

        // Test case 3: Second acceleration exceeds limit
        flight_plan.set_second(Vec3::new(4.0, 4.0, 4.0), 2000); // magnitude ~6.93

        flight_plan.ensure_thrust_limit(6.0);

        assert_ulps_eq!(flight_plan.0.0, expected_accel2);
        assert_eq!(flight_plan.0.1, time1);
        let expected_accel3 = Vec3::new(4.0, 4.0, 4.0).normalize() * 6.0;
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, expected_accel3);
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 2000);

        // Test case 4: Both accelerations exceed limit
        flight_plan.set_first(Vec3::new(10.0, 0.0, 0.0), 1000);
        flight_plan.set_second(Vec3::new(0.0, 8.0, 6.0), 1500);

        flight_plan.ensure_thrust_limit(4.0);

        assert_ulps_eq!(flight_plan.0.0, Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(flight_plan.0.1, 1000);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(0.0, 3.2, 2.4));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 1500);
    }

    #[test]
    fn test_flight_plan_advance_time() {
        let mut flight_plan = FlightPlan::default();
        let accel1 = Vec3::new(1.0, 2.0, 3.0);
        let time1 = 5000;
        let accel2 = Vec3::new(-2.0, -1.0, 0.0);
        let time2 = 3000;
        flight_plan.set_first(accel1, time1);
        flight_plan.set_second(accel2, time2);

        // Test case 1: Advance time less than first duration
        let result = flight_plan.advance_time(2000);
        assert_eq!(result.0.0, accel1);
        assert_eq!(result.0.1, 2000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0.0, accel1);
        assert_eq!(flight_plan.0.1, 3000);
        assert_eq!(flight_plan.1, Some(AccelPair(accel2, time2)));

        // Test case 2: Advance time equal to remaining first duration
        let result = flight_plan.advance_time(3000);
        assert_eq!(result.0.0, accel1);
        assert_eq!(result.0.1, 3000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0.0, accel2);
        assert_eq!(flight_plan.0.1, time2);
        assert_eq!(flight_plan.1, None);

        // Reset flight plan for next test
        flight_plan.set_first(accel1, time1);
        flight_plan.set_second(accel2, time2);

        // Test case 3: Advance time more than first duration but less than total duration
        let result = flight_plan.advance_time(6000);
        assert_eq!(result.0.0, accel1);
        assert_eq!(result.0.1, time1);
        assert_eq!(result.1, Some(AccelPair(accel2, 1000)));
        assert_eq!(flight_plan.0.0, accel2);
        assert_eq!(flight_plan.0.1, 2000);
        assert_eq!(flight_plan.1, None);

        // Test case 4: Advance time more than total duration
        let result = flight_plan.advance_time(3000);
        assert_eq!(result.0.0, accel2);
        assert_eq!(result.0.1, 2000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0.0, Vec3::zero());
        assert_eq!(flight_plan.0.1, 0);
        assert_eq!(flight_plan.1, None);
    }

    #[test_log::test]
    fn test_ship_set_flight_plan() {
        let initial_position = Vec3::new(0.0, 0.0, 0.0);
        let initial_velocity = Vec3::new(1.0, 1.0, 1.0);
        let initial_plan = FlightPlan::default();
        let initial_usp = "38266C2-30060-B".to_string();

        let mut ship = Ship::new(
            "TestShip".to_string(),
            initial_position,
            initial_velocity,
            initial_plan.clone(),
            initial_usp.clone().into(),
        );

        // Test case 1: Set a valid flight plan
        let valid_plan = FlightPlan::new(
            AccelPair(Vec3::new(2.0, 2.0, 2.0), 5000),
            Some(AccelPair(Vec3::new(-1.0, -1.0, -1.0), 3000)),
        );
        assert!(ship.set_flight_plan(&valid_plan).is_ok());
        assert_eq!(ship.plan, valid_plan);

        // Test case 2: Set a flight plan with acceleration exceeding ship's capabilities
        let invalid_plan = FlightPlan::new(
            AccelPair(Vec3::new(100.0, 100.0, 100.0), 5000),
            None,
        );
        assert!(ship.set_flight_plan(&invalid_plan).is_err());
        assert_eq!(ship.plan, valid_plan); // Plan should not have changed

        // Test case 3: Set a flight plan with only one acceleration
        let single_accel_plan = FlightPlan::new(
            AccelPair(Vec3::new(3.0, 3.0, 3.0), 4000),
            None,
        );
        assert!(ship.set_flight_plan(&single_accel_plan).is_ok());
        assert_eq!(ship.plan, single_accel_plan);

        // Test case 4: Set a flight plan with zero acceleration
        let zero_accel_plan = FlightPlan::new(
            AccelPair(Vec3::zero(), 5000),
            Some(AccelPair(Vec3::zero(), 3000)),
        );
        assert!(ship.set_flight_plan(&zero_accel_plan).is_ok());
        assert_eq!(ship.plan, zero_accel_plan);

        // Test case 5: Set a flight plan with acceleration at the ship's limit
        let max_accel = ship.max_acceleration();
        let max_accel_plan = FlightPlan::new(
            AccelPair(Vec3::new(max_accel, 0.0, 0.0), 5000),
            Some(AccelPair(Vec3::new(0.0, max_accel, 0.0), 3000)),
        );
        assert!(ship.set_flight_plan(&max_accel_plan).is_ok());
        assert_eq!(ship.plan, max_accel_plan);

        // Test case 6: Set a flight plan with a second acceleration exceeding ship's capabilities
        let invalid_plan2 = FlightPlan::new(
            AccelPair(Vec3::new(2.0, 2.0, 2.0), 5000),
            Some(AccelPair(Vec3::new(100.0, 100.0, 100.0), 3000)),
        );
        assert!(ship.set_flight_plan(&invalid_plan2).is_err());
        assert_eq!(ship.plan, max_accel_plan); // Plan should not have changed
    }

    #[test]
    fn test_ship_ordering() {
        let ship1 = Ship::new(
            "ship1".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            EXAMPLE_USP.to_string().into(),
        );
        let ship2 = Ship::new(
            "ship2".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            EXAMPLE_USP.to_string().into(),
        );
        assert!(ship1 < ship2);
        assert!(ship2 > ship1);
        assert!(ship1 <= ship2);
        assert!(ship2 >= ship1);
        assert!(ship1 != ship2);
    }

    #[test]
    fn test_flight_plan_iterator() {
        // Test case 1: FlightPlan with two accelerations
        let accel1 = Vec3::new(1.0, 2.0, 3.0);
        let time1 = 5000;
        let accel2 = Vec3::new(-2.0, -1.0, 0.0);
        let time2 = 3000;
        let flight_plan = FlightPlan::new(
            AccelPair(accel1, time1),
            Some(AccelPair(accel2, time2)),
        );

        let mut iter = flight_plan.iter();
        assert_eq!(iter.next(), Some(AccelPair(accel1, time1)));
        assert_eq!(iter.next(), Some(AccelPair(accel2, time2)));
        assert_eq!(iter.next(), None);

        // Test case 2: FlightPlan with only one acceleration
        let flight_plan = FlightPlan::new(
            AccelPair(accel1, time1),
            None,
        );

        let mut iter = flight_plan.iter();
        assert_eq!(iter.next(), Some(AccelPair(accel1, time1)));
        assert_eq!(iter.next(), None);

        // Test case 3: Empty FlightPlan
        let flight_plan = FlightPlan::default();

        let mut iter = flight_plan.iter();
        assert_eq!(iter.next(), Some(AccelPair(Vec3::zero(), 0)));
        assert_eq!(iter.next(), None);

        // Test case 4: FlightPlan with zero acceleration
        let zero_accel = Vec3::zero();
        let flight_plan = FlightPlan::new(
            AccelPair(zero_accel, time1),
            Some(AccelPair(zero_accel, time2)),
        );

        let mut iter = flight_plan.iter();
        assert_eq!(iter.next(), Some(AccelPair(zero_accel, time1)));
        assert_eq!(iter.next(), Some(AccelPair(zero_accel, time2)));
        assert_eq!(iter.next(), None);

        // Test case 5: Using a for loop with the iterator
        let flight_plan = FlightPlan::new(
            AccelPair(accel1, time1),
            Some(AccelPair(accel2, time2)),
        );

        let mut count = 0;
        for (index, accel_pair) in flight_plan.iter().enumerate() {
            match index {
                0 => assert_eq!(accel_pair, AccelPair(accel1, time1)),
                1 => assert_eq!(accel_pair, AccelPair(accel2, time2)),
                _ => panic!("Unexpected iteration"),
            }
            count += 1;
        }
        assert_eq!(count, 2);
    }
}

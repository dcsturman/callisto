use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::sync::Arc;

use cgmath::{InnerSpace, Zero};
use derivative::Derivative;
use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use strum_macros::FromRepr;

use crate::crew::Crew;
use crate::entity::{Entity, UpdateAction, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME, G};
use crate::payloads::Vec3asVec;
use crate::read_local_or_cloud_file;
use crate::{debug, info, warn};

pub static SHIP_TEMPLATES: OnceCell<HashMap<String, Arc<ShipDesignTemplate>>> = OnceCell::new();

#[skip_serializing_none]
#[serde_as]
#[derive(Derivative)]
#[derivative(PartialEq)]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Ship {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    pub plan: FlightPlan,

    #[serde_as(as = "TemplateNameOnly")]
    #[derivative(PartialEq = "ignore")]
    pub design: Arc<ShipDesignTemplate>,

    #[serde(default)]
    pub current_hull: u32,
    #[serde(default)]
    pub current_armor: u32,
    #[serde(default)]
    pub current_power: u32,
    #[serde(default)]
    pub current_maneuver: u8,
    #[serde(default)]
    pub current_jump: u8,
    #[serde(default)]
    pub current_fuel: u32,
    #[serde(default)]
    pub current_crew: u32,
    #[serde(default)]
    pub current_sensors: Sensors,
    #[serde(default)]
    pub active_weapons: Vec<bool>,

    #[derivative(PartialEq = "ignore")]
    #[serde(default)]
    crew: Crew,

    #[derivative(PartialEq = "ignore")]
    #[serde(default)]
    dodge_thrust: u8,

    #[derivative(PartialEq = "ignore")]
    #[serde(default)]
    assist_gunners: bool,

    // Index by turning ShipSystem enum into usize.
    // Skip these in both serializing and deserializing
    // as we don't expect them when loading from a file and
    // don't intend to send them to the server.
    #[serde(skip)]
    pub crit_level: [u8; 11],
    #[serde(skip)]
    pub attack_dm: i32,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShipDesignTemplate {
    pub name: String,
    pub displacement: u32,
    pub hull: u32,
    pub armor: u32,
    pub maneuver: u8,
    pub jump: u8,
    pub power: u32,
    pub fuel: u32,
    pub crew: u32,
    pub sensors: Sensors,
    pub stealth: Option<Stealth>,
    pub computer: u32,
    pub weapons: Vec<Weapon>,
    pub tl: u8,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
    pub kind: WeaponType,
    pub mount: WeaponMount,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum WeaponMount {
    Turret(u8),
    Barbette,
    Bay(BaySize),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaySize {
    Small,
    Medium,
    Large,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum WeaponType {
    Beam = 0,
    Pulse,
    Missile,
    Sand,
    Particle,
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, PartialOrd, FromRepr)]
pub enum Sensors {
    Basic = 0,
    #[default]
    Civilian,
    Military,
    Improved,
    Advanced,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Stealth {
    Basic,
    Improved,
    Enhanced,
    Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, FromRepr)]
pub enum ShipSystem {
    Sensors = 0,
    Powerplant,
    Fuel,
    Weapon,
    Armor,
    Hull,
    Manuever,
    Cargo,
    Jump,
    Crew,
    Bridge,
}

impl Ship {
    pub fn new(
        name: String,
        position: Vec3,
        velocity: Vec3,
        plan: FlightPlan,
        design: Arc<ShipDesignTemplate>,
        crew: Option<Crew>,
    ) -> Self {
        Ship {
            name,
            position,
            velocity,
            plan,
            design: design.clone(),
            current_hull: design.hull,
            current_armor: design.armor,
            current_power: design.power,
            current_maneuver: design.maneuver,
            current_jump: design.jump,
            current_fuel: design.fuel,
            current_crew: design.crew,
            current_sensors: design.sensors,
            active_weapons: vec![true; design.weapons.len()],
            crit_level: [0; 11],
            attack_dm: 0,
            crew: crew.unwrap_or_default(),
            dodge_thrust: 0,
            assist_gunners: false,
        }
    }

    pub fn fixup_current_values(&mut self) {
        self.current_hull = u32::max(self.current_hull, self.design.hull);
        self.current_armor = u32::max(self.current_armor, self.design.armor);
        self.current_power = u32::max(self.current_power, self.design.power);
        self.current_maneuver = u8::max(self.current_maneuver, self.design.maneuver);
        self.current_jump = u8::max(self.current_jump, self.design.jump);
        self.current_fuel = u32::max(self.current_fuel, self.design.fuel);
        self.current_crew = u32::max(self.current_crew, self.design.crew);
        self.current_sensors = Sensors::max(self.current_sensors, self.design.sensors);
        self.active_weapons = vec![true; self.design.weapons.len()];
        self.crit_level = [0; 11];
        self.attack_dm = 0;
        self.dodge_thrust = 0;
    }

    pub fn set_flight_plan(&mut self, new_plan: &FlightPlan) -> Result<(), String> {
        // First validate the plan to make sure its legal.
        // Its legal as long as the magnitudes in the flight plan are less than the max of the maneuverability rating
        // and the powerplant rating.
        // We use the current maneuverability rating in case the ship took damage
        let max_accel = self.max_acceleration();
        debug!("(Ship.set_flight_plan) ship: {}, max_accel: {} new_plan: {:?} with magnitude on first accel of {}", self.name, max_accel, new_plan, new_plan.0 .0.magnitude());
        if new_plan.0.in_limits(max_accel) {
            if let Some(second) = &new_plan.1 {
                if second.in_limits(max_accel) {
                    self.plan = new_plan.clone();
                    Ok(())
                } else {
                    Err(
                        "Flight plan has second acceleration that exceeds max acceleration"
                            .to_string(),
                    )
                }
            } else {
                self.plan = new_plan.clone();
                Ok(())
            }
        } else {
            Err("Flight plan has first acceleration that exceeds max acceleration".to_string())
        }
    }

    pub fn max_acceleration(&self) -> f64 {
        let power_limit = self.design.best_thrust(self.current_power) as f64;
        let maneuver_limit = self.current_maneuver as f64;
        f64::max(
            f64::min(power_limit, maneuver_limit)
                - self.dodge_thrust as f64
                - if self.assist_gunners { 1.0 } else { 0.0 },
            0.0,
        )
    }

    pub fn get_current_hull_points(&self) -> u32 {
        self.current_hull
    }

    pub fn get_max_hull_points(&self) -> u32 {
        self.design.hull
    }

    pub fn set_hull_points(&mut self, new_hull: u32) {
        self.current_hull = new_hull;
    }

    pub fn get_current_armor(&self) -> u32 {
        self.current_armor
    }

    pub fn get_weapon(&self, weapon_id: u32) -> &Weapon {
        &self.design.weapons[weapon_id as usize]
    }

    pub fn get_crew(&self) -> &Crew {
        &self.crew
    }

    pub fn get_crew_mut(&mut self) -> &mut Crew {
        &mut self.crew
    }

    pub fn set_pilot_actions(
        &mut self,
        thrust: Option<u8>,
        assist: Option<bool>,
    ) -> Result<(), InvalidThrustError> {
        let old_agility = self.dodge_thrust;
        let old_assist = self.assist_gunners;

        self.dodge_thrust = 0;
        self.assist_gunners = false;

        // First see if we can set the dodge thrust
        if thrust.is_some_and(|thrust| thrust > self.max_acceleration() as u8) {
            let thrust = thrust.unwrap();
            let old_max_acceleration = self.max_acceleration();
            warn!(
                "(Ship.set_agility_thrust) thrust {} exceeds max acceleration {}",
                thrust, old_max_acceleration
            );
            self.dodge_thrust = old_agility;
            self.assist_gunners = old_assist;
            Err(InvalidThrustError(format!(
                "Thrust {} exceeds max acceleration {}.",
                thrust, old_max_acceleration
            )))
        } else {
            if let Some(thrust) = thrust {
                self.dodge_thrust = thrust;
            }

            // Second see if we can accommodate assist gunner
            if let Some(assist) = assist {
                if assist && self.max_acceleration() < 1.0 {
                    warn!("(Ship.set_agility_thrust) No thrust available to reserve for assisting gunners.");
                    self.dodge_thrust = old_agility;
                    self.assist_gunners = old_assist;

                    Err(InvalidThrustError(
                        "No thrust available to reserve for assisting gunners".to_string(),
                    ))
                } else {
                    self.assist_gunners = assist;
                    Ok(())
                }
            } else {
                Ok(())
            }
        }
    }

    pub fn decrement_dodge_thrust(&mut self) {
        if self.dodge_thrust == 0 {
            warn!("(Ship.decrement_dodge_thrust) Attempting to decrement a 0 dodge thrust; should never happen.");
        }
        self.dodge_thrust = u8::saturating_sub(self.dodge_thrust, 1);
    }

    pub fn get_assist_gunners(&self) -> bool {
        self.assist_gunners
    }
    pub fn reset_crew_actions(&mut self) {
        self.dodge_thrust = 0;
        self.assist_gunners = false;
    }

    pub fn get_dodge_thrust(&self) -> u8 {
        self.dodge_thrust
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
        if self.current_hull == 0 {
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
            let max_thrust = self.max_acceleration();
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

serde_with::serde_conv!(
    pub TemplateNameOnly,
    Arc<ShipDesignTemplate>,
    |t: &Arc<ShipDesignTemplate>| t.name.clone(),
    |value: String| -> Result<_, std::convert::Infallible> {
        let template: Arc<ShipDesignTemplate> = SHIP_TEMPLATES.get().expect("(Deserializing Ship) Ship templates not loaded").get(&value).unwrap().clone();
        Ok(template)
    }
);

// Load ship templates from a file.
pub async fn load_ship_templates_from_file(
    file_name: &str,
) -> Result<HashMap<String, Arc<ShipDesignTemplate>>, Box<dyn std::error::Error>> {
    let templates: Vec<ShipDesignTemplate> =
        serde_json::from_slice(read_local_or_cloud_file(file_name).await?.as_slice())?;

    // From the list of templates, create a hash table and wrap each in an Arc.
    let table = templates
        .into_iter()
        .map(|template| {
            //template.weapons.sort();
            (template.name.clone(), Arc::new(template))
        })
        .collect();

    Ok(table)
}

// Helper method designed only for use in tests to load templates from a default file.
pub async fn config_test_ship_templates() {
    const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";
    let templates = load_ship_templates_from_file(DEFAULT_SHIP_TEMPLATES_FILE)
        .await
        .expect("Unable to load ship template file.");
    SHIP_TEMPLATES.set(templates).unwrap_or_else(|_e| {
        info!("(config_test_ship_templates) attempting to set SHIP_TEMPLATES twice!");
    });
}

impl ShipDesignTemplate {
    // Making this overly simplistic for now.  Assume for power usage that
    // basic systems and sensors are prioritized, and we ignore weapons.
    pub fn best_thrust(&self, current_power: u32) -> u8 {
        // First take out basic ship systems.
        let power: i32 = current_power as i32 - self.displacement as i32 / 5;
        // Now adjust for sensors.
        let power = power
            - match self.sensors {
                Sensors::Basic => 0,
                Sensors::Civilian => 1,
                Sensors::Military => 2,
                Sensors::Improved => 4,
                Sensors::Advanced => 6,
            };

        if power <= 0 {
            return 0;
        }

        // Power left for thrust is one thrust per 10% of ship displacement in power units.
        let available_thrust = power * 10 / self.displacement as i32;

        u8::min(self.maneuver, available_thrust as u8)
    }
}

impl PartialOrd for Weapon {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Weapon {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match (&self.mount, &other.mount) {
            (WeaponMount::Bay(BaySize::Large), WeaponMount::Bay(BaySize::Large)) => {
                self.kind.cmp(&other.kind)
            }
            (WeaponMount::Bay(BaySize::Large), _) => std::cmp::Ordering::Less,
            (WeaponMount::Bay(BaySize::Medium), WeaponMount::Bay(BaySize::Large)) => {
                std::cmp::Ordering::Greater
            }
            (WeaponMount::Bay(BaySize::Medium), WeaponMount::Bay(BaySize::Medium)) => {
                self.kind.cmp(&other.kind)
            }
            (WeaponMount::Bay(BaySize::Medium), _) => std::cmp::Ordering::Less,
            (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Large)) => {
                std::cmp::Ordering::Greater
            }
            (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Medium)) => {
                std::cmp::Ordering::Greater
            }
            (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Small)) => {
                self.kind.cmp(&other.kind)
            }
            (WeaponMount::Bay(BaySize::Small), _) => std::cmp::Ordering::Less,
            (WeaponMount::Barbette, _) => std::cmp::Ordering::Less,
            (WeaponMount::Turret(_), WeaponMount::Bay(_)) => std::cmp::Ordering::Greater,
            (WeaponMount::Turret(_), WeaponMount::Barbette) => std::cmp::Ordering::Greater,
            (WeaponMount::Turret(_), WeaponMount::Turret(_)) => self.kind.cmp(&other.kind),
        }
    }
}

impl Sensors {
    pub fn max(lhs: Sensors, rhs: Sensors) -> Sensors {
        if lhs > rhs {
            lhs
        } else {
            rhs
        }
    }
}
impl From<Sensors> for i32 {
    fn from(s: Sensors) -> Self {
        match s {
            Sensors::Basic => -4,
            Sensors::Civilian => -2,
            Sensors::Military => 0,
            Sensors::Improved => 1,
            Sensors::Advanced => 2,
        }
    }
}

impl From<Sensors> for String {
    fn from(s: Sensors) -> Self {
        match s {
            Sensors::Basic => "Basic".to_string(),
            Sensors::Civilian => "Civilian".to_string(),
            Sensors::Military => "Military".to_string(),
            Sensors::Improved => "Improved".to_string(),
            Sensors::Advanced => "Advanced".to_string(),
        }
    }
}

impl std::ops::Sub<i32> for Sensors {
    type Output = Sensors;

    fn sub(self, rhs: i32) -> Self::Output {
        let int_rep = self as u32 as i32;
        if int_rep - rhs <= 0 {
            Sensors::Basic
        } else {
            Sensors::from_repr((int_rep - rhs) as usize).unwrap()
        }
    }
}

impl From<Stealth> for i32 {
    fn from(s: Stealth) -> Self {
        match s {
            Stealth::Basic => -2,
            Stealth::Improved => -2,
            Stealth::Enhanced => -4,
            Stealth::Advanced => -6,
        }
    }
}

impl From<Stealth> for String {
    fn from(s: Stealth) -> Self {
        match s {
            Stealth::Basic => "Basic".to_string(),
            Stealth::Improved => "Improved".to_string(),
            Stealth::Enhanced => "Enhanced".to_string(),
            Stealth::Advanced => "Advanced".to_string(),
        }
    }
}

impl From<WeaponType> for String {
    fn from(w: WeaponType) -> Self {
        String::from(&w)
    }
}

impl From<&WeaponType> for String {
    fn from(w: &WeaponType) -> Self {
        match w {
            WeaponType::Beam => "beam laser".to_string(),
            WeaponType::Pulse => "pulse laser".to_string(),
            WeaponType::Missile => "missile".to_string(),
            WeaponType::Sand => "sand".to_string(),
            WeaponType::Particle => "particle beam".to_string(),
        }
    }
}

impl From<&Weapon> for String {
    fn from(w: &Weapon) -> Self {
        match (&w.kind, &w.mount) {
            (kind, WeaponMount::Turret(1)) => format!("{} single turret", String::from(kind)),
            (kind, WeaponMount::Turret(2)) => format!("{} double turret", String::from(kind)),
            (kind, WeaponMount::Turret(3)) => format!("{} triple turret", String::from(kind)),
            (_, WeaponMount::Turret(size)) => {
                panic!("(From<Weapon> for String) illegal turret size {}.", size)
            }
            (kind, WeaponMount::Barbette) => format!("{} barbette", String::from(kind)),
            (kind, WeaponMount::Bay(BaySize::Small)) => format!("{} small bay", String::from(kind)),
            (kind, WeaponMount::Bay(BaySize::Medium)) => {
                format!("{} medium bay", String::from(kind))
            }
            (kind, WeaponMount::Bay(BaySize::Large)) => format!("{} large bay", String::from(kind)),
        }
    }
}

impl WeaponType {
    pub fn is_laser(&self) -> bool {
        matches!(self, WeaponType::Beam | WeaponType::Pulse)
    }
}

impl From<ShipSystem> for String {
    fn from(s: ShipSystem) -> Self {
        match s {
            ShipSystem::Hull => "hull".to_string(),
            ShipSystem::Armor => "armor".to_string(),
            ShipSystem::Jump => "jump drive".to_string(),
            ShipSystem::Manuever => "maneuver drive".to_string(),
            ShipSystem::Powerplant => "power plant".to_string(),
            ShipSystem::Crew => "crew".to_string(),
            ShipSystem::Weapon => "a weapon".to_string(),
            ShipSystem::Sensors => "sensors".to_string(),
            ShipSystem::Fuel => "fuel".to_string(),
            ShipSystem::Bridge => "bridge".to_string(),
            ShipSystem::Cargo => "cargo".to_string(),
        }
    }
}

#[derive(Debug)]
pub struct InvalidThrustError(String);

impl InvalidThrustError {
    pub fn get_msg(&self) -> String {
        self.0.clone()
    }
}

impl Error for InvalidThrustError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        None
    }
}

impl std::fmt::Display for InvalidThrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Invalid thrust attempted: {}", self.0)
    }
}

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

impl AccelPair {
    pub fn in_limits(&self, limit: f64) -> bool {
        self.0.magnitude() <= limit
            || approx::relative_eq!(&self.0.magnitude(), &limit, max_relative = 1e-3)
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
            self.0 = (second.0, second.1 - (time - self.0 .1)).into();
            self.1 = None;
            debug!("(FlightPlan.advance_time) self: {:?} new_first: {:?} second: {:?} time: {} first_time: {}", self, new_first, second, time, first_time);
            FlightPlan::new(
                new_first,
                if time <= first_time {
                    None
                } else {
                    Some((second.0, time - first_time).into())
                },
            )
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

impl Default for ShipDesignTemplate {
    fn default() -> Self {
        ShipDesignTemplate {
            name: "Buccaneer".to_string(),
            displacement: 400,
            hull: 160,
            armor: 5,
            maneuver: 3,
            jump: 2,
            power: 300,
            fuel: 81,
            crew: 11,
            sensors: Sensors::Improved,
            stealth: None,
            computer: 5,
            weapons: vec![
                Weapon {
                    kind: WeaponType::Pulse,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Pulse,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Sand,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Sand,
                    mount: WeaponMount::Turret(2),
                },
            ],
            tl: 15,
        }
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
fn int_to_digit(code: u8) -> char {
    match code {
        x if x <= 9 => (x + b'0') as char,
        x if x <= 17 => (x - 10 + b'A') as char,
        x if x <= 22 => (x - 18 + b'J') as char,
        x if x <= 33 => (x - 23 + b'P') as char,
        _ => panic!("(ship.intToDigit) Unknown code: {}", code),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crew::Skills;
    use cgmath::assert_ulps_eq;

    #[test_log::test]
    fn test_digit_to_int() {
        // Test digits 0-9
        assert_eq!(digit_to_int('0'), 0);
        assert_eq!(digit_to_int('1'), 1);
        assert_eq!(digit_to_int('2'), 2);
        assert_eq!(digit_to_int('3'), 3);
        assert_eq!(digit_to_int('4'), 4);
        assert_eq!(digit_to_int('5'), 5);
        assert_eq!(digit_to_int('6'), 6);
        assert_eq!(digit_to_int('7'), 7);
        assert_eq!(digit_to_int('8'), 8);
        assert_eq!(digit_to_int('9'), 9);

        // Test all valid letters A-Z (excluding I, O)
        assert_eq!(digit_to_int('A'), 10);
        assert_eq!(digit_to_int('B'), 11);
        assert_eq!(digit_to_int('C'), 12);
        assert_eq!(digit_to_int('D'), 13);
        assert_eq!(digit_to_int('E'), 14);
        assert_eq!(digit_to_int('F'), 15);
        assert_eq!(digit_to_int('G'), 16);
        assert_eq!(digit_to_int('H'), 17);
        assert_eq!(digit_to_int('J'), 18);
        assert_eq!(digit_to_int('K'), 19);
        assert_eq!(digit_to_int('L'), 20);
        assert_eq!(digit_to_int('M'), 21);
        assert_eq!(digit_to_int('N'), 22);
        assert_eq!(digit_to_int('P'), 23);
        assert_eq!(digit_to_int('Q'), 24);
        assert_eq!(digit_to_int('R'), 25);
        assert_eq!(digit_to_int('S'), 26);
        assert_eq!(digit_to_int('T'), 27);
        assert_eq!(digit_to_int('U'), 28);
        assert_eq!(digit_to_int('V'), 29);
        assert_eq!(digit_to_int('W'), 30);
        assert_eq!(digit_to_int('X'), 31);
        assert_eq!(digit_to_int('Y'), 32);
        assert_eq!(digit_to_int('Z'), 33);
    }

    #[test_log::test]
    fn test_int_to_digit() {
        // Test integers 0-9
        assert_eq!(int_to_digit(0), '0');
        assert_eq!(int_to_digit(1), '1');
        assert_eq!(int_to_digit(2), '2');
        assert_eq!(int_to_digit(3), '3');
        assert_eq!(int_to_digit(4), '4');
        assert_eq!(int_to_digit(5), '5');
        assert_eq!(int_to_digit(6), '6');
        assert_eq!(int_to_digit(7), '7');
        assert_eq!(int_to_digit(8), '8');
        assert_eq!(int_to_digit(9), '9');

        // Test all valid integers 10-33 (corresponding to A-Z, excluding I, O)
        assert_eq!(int_to_digit(10), 'A');
        assert_eq!(int_to_digit(11), 'B');
        assert_eq!(int_to_digit(12), 'C');
        assert_eq!(int_to_digit(13), 'D');
        assert_eq!(int_to_digit(14), 'E');
        assert_eq!(int_to_digit(15), 'F');
        assert_eq!(int_to_digit(16), 'G');
        assert_eq!(int_to_digit(17), 'H');
        assert_eq!(int_to_digit(18), 'J');
        assert_eq!(int_to_digit(19), 'K');
        assert_eq!(int_to_digit(20), 'L');
        assert_eq!(int_to_digit(21), 'M');
        assert_eq!(int_to_digit(22), 'N');
        assert_eq!(int_to_digit(23), 'P');
        assert_eq!(int_to_digit(24), 'Q');
        assert_eq!(int_to_digit(25), 'R');
        assert_eq!(int_to_digit(26), 'S');
        assert_eq!(int_to_digit(27), 'T');
        assert_eq!(int_to_digit(28), 'U');
        assert_eq!(int_to_digit(29), 'V');
        assert_eq!(int_to_digit(30), 'W');
        assert_eq!(int_to_digit(31), 'X');
        assert_eq!(int_to_digit(32), 'Y');
        assert_eq!(int_to_digit(33), 'Z');
    }

    #[test_log::test]
    fn test_digit_to_int_invalid_cases() {
        let invalid_chars = ['I', 'O', 'a', 'i', 'o', 'z', '#', ' ', '-'];
        for &c in &invalid_chars {
            let result = std::panic::catch_unwind(|| digit_to_int(c));
            assert!(result.is_err(), "Expected panic for character: {}", c);
        }
    }

    #[test_log::test]
    fn test_int_to_digit_invalid_cases() {
        let invalid_ints = [34, 35, 99, 255];
        for &i in &invalid_ints {
            let result = std::panic::catch_unwind(|| int_to_digit(i));
            assert!(result.is_err(), "Expected panic for integer: {}", i);
        }
    }

    #[test_log::test]
    fn test_digit_conversion_roundtrip() {
        // Test roundtrip conversion for all valid values
        for i in 0..34 {
            let digit = int_to_digit(i);
            let num = digit_to_int(digit);
            assert_eq!(i, num, "Roundtrip failed for number {}", i);
        }
    }

    #[test_log::test]
    fn test_ship_setters_and_getters() {
        let initial_position = Vec3::new(0.0, 0.0, 0.0);
        let initial_velocity = Vec3::new(1.0, 1.0, 1.0);
        let initial_plan = FlightPlan::default();

        let mut ship = Ship::new(
            "TestShip".to_string(),
            initial_position,
            initial_velocity,
            initial_plan.clone(),
            Arc::new(ShipDesignTemplate::default()),
            None,
        );

        // Test initial values
        assert_eq!(ship.get_name(), "TestShip");
        assert_eq!(ship.get_position(), initial_position);
        assert_eq!(ship.get_velocity(), initial_velocity);
        assert_eq!(ship.plan, initial_plan);

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
        assert_eq!(ship.current_hull, 160); // 2 * usp.hull (3 for '3' in the USP)

        // Test invalid flight plan
        let invalid_plan = FlightPlan::acceleration(Vec3::new(100.0, 100.0, 100.0)); // Assuming this exceeds max acceleration
        assert!(ship.set_flight_plan(&invalid_plan).is_err());
        assert_eq!(ship.plan, new_plan); // Plan should not have changed
    }
    #[test_log::test]
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

    #[test_log::test]
    fn test_flight_plan_ensure_thrust_limit() {
        let mut flight_plan = FlightPlan::default();

        // Test case 1: Acceleration within limit
        let accel1 = Vec3::new(3.0, 4.0, 0.0); // magnitude 5
        let time1 = 5000;
        flight_plan.set_first(accel1, time1);
        flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0), 3000); // magnitude 3

        flight_plan.ensure_thrust_limit(6.0);

        assert_ulps_eq!(flight_plan.0 .0, accel1);
        assert_eq!(flight_plan.0 .1, time1);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

        // Test case 2: First acceleration exceeds limit
        let accel2 = Vec3::new(6.0, 8.0, 0.0); // magnitude 10
        flight_plan.set_first(accel2, time1);
        flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0), 3000); // magnitude 3

        flight_plan.ensure_thrust_limit(6.0);

        let expected_accel2 = accel2.normalize() * 6.0;
        assert_ulps_eq!(flight_plan.0 .0, expected_accel2);
        assert_eq!(flight_plan.0 .1, time1);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

        // Test case 3: Second acceleration exceeds limit
        flight_plan.set_second(Vec3::new(4.0, 4.0, 4.0), 2000); // magnitude ~6.93

        flight_plan.ensure_thrust_limit(6.0);

        assert_ulps_eq!(flight_plan.0 .0, expected_accel2);
        assert_eq!(flight_plan.0 .1, time1);
        let expected_accel3 = Vec3::new(4.0, 4.0, 4.0).normalize() * 6.0;
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, expected_accel3);
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 2000);

        // Test case 4: Both accelerations exceed limit
        flight_plan.set_first(Vec3::new(10.0, 0.0, 0.0), 1000);
        flight_plan.set_second(Vec3::new(0.0, 8.0, 6.0), 1500);

        flight_plan.ensure_thrust_limit(4.0);

        assert_ulps_eq!(flight_plan.0 .0, Vec3::new(4.0, 0.0, 0.0));
        assert_eq!(flight_plan.0 .1, 1000);
        assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(0.0, 3.2, 2.4));
        assert_eq!(flight_plan.1.as_ref().unwrap().1, 1500);
    }

    #[test_log::test]
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
        assert_eq!(result.0 .0, accel1);
        assert_eq!(result.0 .1, 2000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0 .0, accel1);
        assert_eq!(flight_plan.0 .1, 3000);
        assert_eq!(flight_plan.1, Some(AccelPair(accel2, time2)));

        // Test case 2: Advance time equal to remaining first duration
        let result = flight_plan.advance_time(3000);
        assert_eq!(result.0 .0, accel1);
        assert_eq!(result.0 .1, 3000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0 .0, accel2);
        assert_eq!(flight_plan.0 .1, time2);
        assert_eq!(flight_plan.1, None);

        // Reset flight plan for next test
        flight_plan.set_first(accel1, time1);
        flight_plan.set_second(accel2, time2);

        // Test case 3: Advance time more than first duration but less than total duration
        let result = flight_plan.advance_time(6000);
        assert_eq!(result.0 .0, accel1);
        assert_eq!(result.0 .1, time1);
        assert_eq!(result.1, Some(AccelPair(accel2, 1000)));
        assert_eq!(flight_plan.0 .0, accel2);
        assert_eq!(flight_plan.0 .1, 2000);
        assert_eq!(flight_plan.1, None);

        // Test case 4: Advance time more than total duration
        let result = flight_plan.advance_time(3000);
        assert_eq!(result.0 .0, accel2);
        assert_eq!(result.0 .1, 2000);
        assert_eq!(result.1, None);
        assert_eq!(flight_plan.0 .0, Vec3::zero());
        assert_eq!(flight_plan.0 .1, 0);
        assert_eq!(flight_plan.1, None);
    }

    #[test_log::test]
    fn test_ship_set_flight_plan() {
        let initial_position = Vec3::new(0.0, 0.0, 0.0);
        let initial_velocity = Vec3::new(1.0, 1.0, 1.0);
        let initial_plan = FlightPlan::default();

        let mut ship = Ship::new(
            "TestShip".to_string(),
            initial_position,
            initial_velocity,
            initial_plan.clone(),
            Arc::new(ShipDesignTemplate::default()),
            None,
        );

        // Test case 1: Set a valid flight plan
        let valid_plan = FlightPlan::new(
            AccelPair(Vec3::new(2.0, 2.0, 1.0), 5000),
            Some(AccelPair(Vec3::new(-1.0, -1.0, -1.0), 3000)),
        );
        assert!(ship.set_flight_plan(&valid_plan).is_ok());
        assert_eq!(ship.plan, valid_plan);

        // Test case 2: Set a flight plan with acceleration exceeding ship's capabilities
        let invalid_plan = FlightPlan::new(AccelPair(Vec3::new(100.0, 100.0, 100.0), 5000), None);
        assert!(ship.set_flight_plan(&invalid_plan).is_err());
        assert_eq!(ship.plan, valid_plan); // Plan should not have changed

        // Test case 3: Set a flight plan with only one acceleration
        let single_accel_plan = FlightPlan::new(AccelPair(Vec3::new(2.0, 2.0, 1.0), 4000), None);
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
            AccelPair(Vec3::new(2.0, 2.0, 0.0), 5000),
            Some(AccelPair(Vec3::new(100.0, 100.0, 100.0), 3000)),
        );
        assert!(ship.set_flight_plan(&invalid_plan2).is_err());
        assert_eq!(ship.plan, max_accel_plan); // Plan should not have changed
    }

    #[test_log::test]
    fn test_ship_ordering() {
        let ship1 = Ship::new(
            "ship1".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
            None,
        );
        let ship2 = Ship::new(
            "ship2".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
            None,
        );
        assert!(ship1 < ship2);
        assert!(ship2 > ship1);
        assert!(ship1 <= ship2);
        assert!(ship2 >= ship1);
        assert!(ship1 != ship2);
    }

    #[test_log::test]
    fn test_flight_plan_iterator() {
        // Test case 1: FlightPlan with two accelerations
        let accel1 = Vec3::new(1.0, 2.0, 3.0);
        let time1 = 5000;
        let accel2 = Vec3::new(-2.0, -1.0, 0.0);
        let time2 = 3000;
        let flight_plan = FlightPlan::new(AccelPair(accel1, time1), Some(AccelPair(accel2, time2)));

        let mut iter = flight_plan.iter();
        assert_eq!(iter.next(), Some(AccelPair(accel1, time1)));
        assert_eq!(iter.next(), Some(AccelPair(accel2, time2)));
        assert_eq!(iter.next(), None);

        // Test case 2: FlightPlan with only one acceleration
        let flight_plan = FlightPlan::new(AccelPair(accel1, time1), None);

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
        let flight_plan = FlightPlan::new(AccelPair(accel1, time1), Some(AccelPair(accel2, time2)));

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

    #[test_log::test]
    fn test_set_agility_thrust() {
        let mut ship = Ship::new(
            "TestShip".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
            None,
        );

        // Test setting a valid agility thrust
        assert!(ship.set_pilot_actions(Some(1), None).is_ok());
        assert_eq!(ship.get_dodge_thrust(), 1);

        // Test setting agility thrust to 0
        assert!(ship.set_pilot_actions(Some(0), None).is_ok());
        assert_eq!(ship.get_dodge_thrust(), 0);

        // Test setting agility thrust to max acceleration
        assert!(ship.set_pilot_actions(Some(3), None).is_ok());
        assert_eq!(ship.get_dodge_thrust(), 3);

        // Test setting agility thrust above max acceleration
        let result = ship.set_pilot_actions(Some(11), None);
        assert!(result.is_err());
        assert_eq!(ship.get_dodge_thrust(), 3); // Should remain unchanged

        // Test that the error returned is of type InvalidAgilityError
        assert!(matches!(result, Err(InvalidThrustError(_))));
        let err = result.unwrap_err();
        assert!(err.source().is_none());
        assert!(format!("{}", err).contains("Invalid thrust attempted:"));

        // Test resetting agility
        ship.reset_crew_actions();
        assert_eq!(ship.get_dodge_thrust(), 0);
    }

    #[test_log::test]
    fn test_get_crew() {
        let ship = Ship::new(
            "TestShip".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
            Some(Crew::new()),
        );

        // Test get_crew
        let crew = ship.get_crew();
        assert_eq!(crew.get_pilot(), 0);
        assert_eq!(crew.get_engineering_jump(), 0);
        assert_eq!(crew.get_engineering_power(), 0);
        assert_eq!(crew.get_engineering_maneuver(), 0);
        assert_eq!(crew.get_sensors(), 0);
        assert_eq!(crew.get_gunnery(0), 0);
    }

    #[test_log::test]
    fn test_get_crew_mut() {
        let mut ship = Ship::new(
            "TestShip".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::new(0.0, 0.0, 0.0),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
            Some(Crew::new()),
        );

        // Test get_crew_mut
        {
            let crew_mut = ship.get_crew_mut();
            crew_mut.set_skill(Skills::Pilot, 3);
            crew_mut.set_skill(Skills::EngineeringJump, 2);
            crew_mut.set_skill(Skills::EngineeringPower, 1);
            crew_mut.set_skill(Skills::EngineeringManeuver, 4);
            crew_mut.set_skill(Skills::Sensors, 5);
            crew_mut.add_gunnery(2);
        }

        // Verify changes
        let crew = ship.get_crew();
        assert_eq!(crew.get_pilot(), 3);
        assert_eq!(crew.get_engineering_jump(), 2);
        assert_eq!(crew.get_engineering_power(), 1);
        assert_eq!(crew.get_engineering_maneuver(), 4);
        assert_eq!(crew.get_sensors(), 5);
        assert_eq!(crew.get_gunnery(0), 2);
    }

    #[test_log::test]
    fn test_weapon_ordering() {
        // Create test weapons with different mounts and types
        let large_bay_beam = Weapon {
            kind: WeaponType::Beam,
            mount: WeaponMount::Bay(BaySize::Large),
        };
        let large_bay_pulse = Weapon {
            kind: WeaponType::Pulse,
            mount: WeaponMount::Bay(BaySize::Large),
        };
        let medium_bay = Weapon {
            kind: WeaponType::Beam,
            mount: WeaponMount::Bay(BaySize::Medium),
        };

        let medium_bay_missile = Weapon {
            kind: WeaponType::Missile,
            mount: WeaponMount::Bay(BaySize::Medium),
        };

        let small_bay = Weapon {
            kind: WeaponType::Beam,
            mount: WeaponMount::Bay(BaySize::Small),
        };

        let small_bay_pulse = Weapon {
            kind: WeaponType::Pulse,
            mount: WeaponMount::Bay(BaySize::Small),
        };

        let barbette = Weapon {
            kind: WeaponType::Beam,
            mount: WeaponMount::Barbette,
        };
        let turret = Weapon {
            kind: WeaponType::Beam,
            mount: WeaponMount::Turret(2),
        };
        let turret_pulse = Weapon {
            kind: WeaponType::Pulse,
            mount: WeaponMount::Turret(2),
        };

        // Test ordering between same mount types
        assert!(large_bay_beam < large_bay_pulse); // Same mount, different types
        
        // Test ordering between different mount types
        assert!(large_bay_beam < medium_bay);      // Large bay < Medium bay
        assert!(medium_bay > large_bay_pulse);      // Large bay < Medium bay
        assert!(medium_bay < small_bay);           // Medium bay < Small bay
        assert!(small_bay > medium_bay_missile);     // Medium bay < Small bay
        assert!(medium_bay_missile > medium_bay);     // Medium bay < Small bay
        assert!(small_bay < barbette);             // Small bay < Barbette
        assert!(small_bay_pulse > small_bay);        // Small bay < Barbette
        assert!(small_bay < small_bay_pulse);        // Small bay < Barbette
        assert!(small_bay > large_bay_pulse);
        assert!(barbette < turret);                // Barbette < Turret
        assert!(turret > barbette);                // Barbette < Turret
        assert!(turret < turret_pulse);                // Barbette < Turret

        // Test transitivity
        assert!(large_bay_beam < small_bay);       // Large bay < Small bay
        assert!(medium_bay < barbette);            // Medium bay < Barbette
        assert!(small_bay < turret);               // Small bay < Turret

        // Test turret comparison with bays
        assert!(turret > large_bay_beam);          // Turret > Large bay
        assert!(turret > medium_bay);              // Turret > Medium bay
        assert!(turret > small_bay);               // Turret > Small bay
    }

    #[test_log::test]
    fn test_sensors() {
        // Test Sensors::max
        assert_eq!(Sensors::max(Sensors::Basic, Sensors::Military), Sensors::Military);
        assert_eq!(Sensors::max(Sensors::Advanced, Sensors::Civilian), Sensors::Advanced);
        assert_eq!(Sensors::max(Sensors::Military, Sensors::Military), Sensors::Military);
        
        // Test conversion to i32
        assert_eq!(i32::from(Sensors::Basic), -4);
        assert_eq!(i32::from(Sensors::Civilian), -2);
        assert_eq!(i32::from(Sensors::Military), 0);
        assert_eq!(i32::from(Sensors::Improved), 1);
        assert_eq!(i32::from(Sensors::Advanced), 2);

        // Test ordering
        assert!(Sensors::Basic < Sensors::Civilian);
        assert!(Sensors::Civilian < Sensors::Military);
        assert!(Sensors::Military < Sensors::Improved);
        assert!(Sensors::Improved < Sensors::Advanced);
    }

    #[test_log::test]
    fn test_fixup_current_values() {
        // Create a ship design template with some values
        let design = Arc::new(ShipDesignTemplate {
            name: "Test Ship".to_string(),
            displacement: 400,
            hull: 100,
            armor: 50,
            maneuver: 4,
            jump: 2,
            power: 200,
            fuel: 1000,
            crew: 20,
            sensors: Sensors::Military,
            stealth: None,
            computer: 10,
            weapons: vec![
                Weapon {
                    kind: WeaponType::Beam,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Pulse,
                    mount: WeaponMount::Bay(BaySize::Small),
                },
            ],
            tl: 12,
        });

        // Create a ship with lower current values
        let mut ship = Ship::new(
            "TestShip".to_string(),
            Vec3::zero(),
            Vec3::zero(),
            FlightPlan::default(),
            design.clone(),
            None,
        );

        // Manually set current values to be lower than design values
        ship.current_hull = 50;      // Lower than design.hull (100)
        ship.current_armor = 25;     // Lower than design.armor (50)
        ship.current_power = 100;    // Lower than design.power (200)
        ship.current_maneuver = 2;   // Lower than design.maneuver (4)
        ship.current_jump = 1;       // Lower than design.jump (2)
        ship.current_fuel = 500;     // Lower than design.fuel (1000)
        ship.current_crew = 10;      // Lower than design.crew (20)
        ship.current_sensors = Sensors::Basic; // Lower than design.sensors (Military)
        ship.active_weapons = vec![false, false]; // All false
        ship.crit_level = [1; 11];  // All ones
        ship.attack_dm = -2;        // Negative value
        ship.dodge_thrust = 2;      // Non-zero value

        // Call fixup_current_values
        ship.fixup_current_values();

        // Verify all values are restored to design values
        assert_eq!(ship.current_hull, design.hull);
        assert_eq!(ship.current_armor, design.armor);
        assert_eq!(ship.current_power, design.power);
        assert_eq!(ship.current_maneuver, design.maneuver);
        assert_eq!(ship.current_jump, design.jump);
        assert_eq!(ship.current_fuel, design.fuel);
        assert_eq!(ship.current_crew, design.crew);
        assert_eq!(ship.current_sensors, design.sensors);
        assert_eq!(ship.active_weapons, vec![true, true]);
        assert_eq!(ship.crit_level, [0; 11]);
        assert_eq!(ship.attack_dm, 0);
        assert_eq!(ship.dodge_thrust, 0);

        // Test that values higher than design values are not reduced
        ship.current_hull = 150;     // Higher than design.hull
        ship.current_armor = 75;     // Higher than design.armor
        ship.current_sensors = Sensors::Advanced; // Higher than design.sensors

        ship.fixup_current_values();

        // Verify higher values are preserved
        assert_eq!(ship.current_hull, 150);
        assert_eq!(ship.current_armor, 75);
        assert_eq!(ship.current_sensors, Sensors::Advanced);
    }

    #[test]
    fn test_stealth_to_i32_conversion() {
        assert_eq!(i32::from(Stealth::Basic), -2);
        assert_eq!(i32::from(Stealth::Improved), -2);
        assert_eq!(i32::from(Stealth::Enhanced), -4);
        assert_eq!(i32::from(Stealth::Advanced), -6);
    }

    #[test]
    fn test_stealth_to_string_conversion() {
        assert_eq!(String::from(Stealth::Basic), "Basic");
        assert_eq!(String::from(Stealth::Improved), "Improved");
        assert_eq!(String::from(Stealth::Enhanced), "Enhanced");
        assert_eq!(String::from(Stealth::Advanced), "Advanced");
    }

    #[test]
    fn test_stealth_string_case_sensitivity() {
        // Verify that the strings match exactly, including case
        let basic = String::from(Stealth::Basic);
        assert_ne!(basic, "basic");
        assert_ne!(basic, "BASIC");
        
        let improved = String::from(Stealth::Improved);
        assert_ne!(improved, "improved");
        assert_ne!(improved, "IMPROVED");
    }

    #[test]
    fn test_sensors_to_string_conversion() {
        assert_eq!(String::from(Sensors::Basic), "Basic");
        assert_eq!(String::from(Sensors::Civilian), "Civilian");
        assert_eq!(String::from(Sensors::Military), "Military");
        assert_eq!(String::from(Sensors::Improved), "Improved");
        assert_eq!(String::from(Sensors::Advanced), "Advanced");
    }

    #[test]
    fn test_sensors_string_case_sensitivity() {
        // Verify that the strings match exactly, including case
        let improved = String::from(Sensors::Improved);
        assert_ne!(improved, "improved");
        assert_ne!(improved, "IMPROVED");
        
        let advanced = String::from(Sensors::Advanced);
        assert_ne!(advanced, "advanced");
        assert_ne!(advanced, "ADVANCED");
    }

    #[test]
    fn test_sensors_to_i32_and_string() {
        // Test both conversions for each variant
        let test_cases = vec![
            (Sensors::Basic, -4, "Basic"),
            (Sensors::Civilian, -2, "Civilian"),
            (Sensors::Military, 0, "Military"),
            (Sensors::Improved, 1, "Improved"),
            (Sensors::Advanced, 2, "Advanced"),
        ];

        for (sensor, expected_i32, expected_string) in test_cases {
            assert_eq!(i32::from(sensor), expected_i32);
            assert_eq!(String::from(sensor), expected_string);
        }
    }

    #[test]
    fn test_best_thrust() {
        let design = ShipDesignTemplate {
            name: "Test Ship".to_string(),
            displacement: 400,
            hull: 100,
            armor: 50,
            maneuver: 4,
            jump: 2,
            power: 250,
            fuel: 1000,
            crew: 20,
            sensors: Sensors::Military,
            stealth: None,
            computer: 10,
            weapons: vec![],
            tl: 12,
        };

        // Test normal case
        assert_eq!(design.best_thrust(250), 4);

        // Test with reduced power
        assert_eq!(design.best_thrust(180), 2);

        // Test case where power calculation results in <= 0
        // With displacement 400, basic systems use 80 power (400/5)
        // Military sensors use 2 more power
        // Thrust 1 requires 40 more power
        // So providing 121 or less power should result in 0 thrust
        assert_eq!(design.best_thrust(122), 1);
        assert_eq!(design.best_thrust(121), 0);
        assert_eq!(design.best_thrust(0), 0);
    }

    #[test]
    fn test_weapon_type_is_laser() {
        // Test laser weapons
        assert!(WeaponType::Beam.is_laser());
        assert!(WeaponType::Pulse.is_laser());
        
        // Test non-laser weapons
        assert!(!WeaponType::Missile.is_laser());
        assert!(!WeaponType::Sand.is_laser());
        assert!(!WeaponType::Particle.is_laser());
    }

    #[test]
    fn test_ship_system_to_string() {
        let test_cases = vec![
            (ShipSystem::Hull, "hull"),
            (ShipSystem::Armor, "armor"),
            (ShipSystem::Jump, "jump drive"),
            (ShipSystem::Manuever, "maneuver drive"),
            (ShipSystem::Powerplant, "power plant"),
            (ShipSystem::Crew, "crew"),
            (ShipSystem::Weapon, "a weapon"),
            (ShipSystem::Sensors, "sensors"),
            (ShipSystem::Fuel, "fuel"),
            (ShipSystem::Bridge, "bridge"),
            (ShipSystem::Cargo, "cargo"),
        ];

        for (system, expected) in test_cases {
            assert_eq!(String::from(system), expected);
        }
    }
}

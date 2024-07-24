use cgmath::{ElementWise, InnerSpace, Vector3, Zero};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use derivative::Derivative;

use serde_with::{serde_as, skip_serializing_none};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::computer::{compute_target_path, FlightPathResult, TargetParams};
use crate::payloads::{EffectMsg, Vec3asVec, EXHAUSTED_MISSILE, SHIP_IMPACT};

pub const DELTA_TIME: u64 = 1000;
pub const DEFAULT_ACCEL_DURATION: u64 = 10000;
pub const G: f64 = 9.81;
// Temporary until missiles have actual acceleration built in
const MAX_MISSILE_ACCELERATION: f64 = 6.0 * G;
const IMPACT_DISTANCE: f64 = 2500000.0;

pub type Vec3 = Vector3<f64>;

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

#[derive(Derivative)]
#[derivative(PartialEq)]
#[skip_serializing_none]
#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum EntityKind {
    Ship {
        plan: FlightPlan,
    },
    Planet {
        // Any valid color string OR a string starting with "!" then referring to a special template
        color: String,
        radius: f64,
        mass: f64,
        // The primary is the name of the planet which is the center of the orbit.
        // None means it not orbiting anything. Some("Earth") means its orbiting the planet named Earth.
        #[serde(default)]
        primary: Option<String>,

        // TODO: Ideally this would be in the primary structure rather than also be an Option outside it.  But I right now
        // can't figure out how to skip serde for a portion of a tuple.
        #[serde(skip)]
        #[derivative(PartialEq = "ignore")]
        primary_ptr: Option<Arc<RwLock<Entity>>>,

        // Dependency is used to enforce order of update.  Lower values (e.g. a star with value 0) are updated first.
        // Not needed to be passed in JSON to the client; not needed for comparison operations.
        #[serde(skip)]
        dependency: i32,
    },

    Missile {
        source: String,
        target: String,
        // FIXME: This is dangerous.  Its not clear how to deal with a Missile without a target_ptr.
        #[serde(skip)]
        #[derivative(PartialEq = "ignore")]
        target_ptr: Option<Arc<RwLock<Entity>>>,
        #[serde_as(as = "Vec3asVec")]
        acceleration: Vec3,
        burns: i32,
    },
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Entity {
    name: String,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    velocity: Vec3,
    pub kind: EntityKind,
}

impl PartialEq for Entity {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.position == other.position
            && self.velocity == other.velocity
            && self.kind == other.kind
    }
}

impl Entity {
    // Constructor for a new entity.
    pub fn new_ship(name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) -> Self {
        Entity {
            name,
            position,
            velocity,
            kind: EntityKind::Ship {
                plan: FlightPlan::acceleration(acceleration),
            },
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_planet(
        name: String,
        position: Vec3,
        color: String,
        radius: f64,
        mass: f64,
        primary: Option<String>,
        primary_ptr: Option<Arc<RwLock<Entity>>>,
        dependency: i32,
    ) -> Self {
        Entity {
            name,
            position,
            velocity: Vec3::zero(),
            kind: EntityKind::Planet {
                color,
                radius,
                mass,
                primary,
                primary_ptr,
                dependency,
            },
        }
    }

    pub fn new_missile(
        name: String,
        source: String,
        target: String,
        target_ptr: Arc<RwLock<Entity>>,
        position: Vec3,
        velocity: Vec3,
        burns: i32,
    ) -> Self {
        // We need to construct an initial route for the missile primarily so
        // it can be shown in the UX once creation of the missile returns.
        let target_pos = target_ptr.read().unwrap().position;
        let target_vel = target_ptr.read().unwrap().velocity;

        let params = TargetParams::new(
            position,
            target_pos,
            velocity,
            target_vel,
            MAX_MISSILE_ACCELERATION,
        );

        debug!(
            "Creating initial missile acceleration and calling targeting computer for missile {} with params: {:?}",
            name, params
        );

        let path: FlightPathResult = compute_target_path(&params);
        let acceleration = path.plan.0 .0;
        Entity {
            name,
            position,
            velocity,
            kind: EntityKind::Missile {
                source,
                target,
                target_ptr: Some(target_ptr),
                acceleration,
                burns,
            },
        }
    }

    pub fn get_flight_plan(&self) -> Option<&FlightPlan> {
        match &self.kind {
            EntityKind::Ship { plan } => Some(plan),
            _ => None,
        }
    }

    // Method to get the name of the entity.
    #[allow(dead_code)]
    pub fn get_name(&self) -> String {
        self.name.clone()
    }

    // Method to set the flight plan of the entity.
    pub fn set_flight_plan(&mut self, new_plan: FlightPlan) {
        match &mut self.kind {
            EntityKind::Ship { plan } => {
                *plan = new_plan;
            }
            _ => panic!("Cannot set acceleration on non-ship {:?}", self.name),
        }
    }

    pub fn get_position(&self) -> Vec3 {
        self.position
    }

    pub fn get_velocity(&self) -> Vec3 {
        self.velocity
    }

    // Method to update the position of the entity based on its velocity and acceleration.
    pub fn update(&mut self) -> Option<UpdateAction> {
        debug!("(Entity.update) Updating entity {:?}", self.name);
        match &mut self.kind {
            EntityKind::Ship { plan } => {
                debug!("(Entity.update) Updating ship {:?}", self.name);
                if plan.empty() {
                    // Just move at current velocity
                    self.position += self.velocity * DELTA_TIME as f64;
                    debug!("(Entity.update) No acceleration for {}: move at velocity {:0.0?} for time {}, position now {:0.0?}", self.name, self.velocity, DELTA_TIME, self.position);
                } else {
                    let moves = plan.advance_time(DELTA_TIME);

                    for ap in moves.iter() {
                        let old_velocity: Vec3 = self.velocity;
                        let (accel, duration) = ap.into();
                        self.velocity += accel * G * duration as f64;
                        self.position += (old_velocity + self.velocity) / 2.0 * duration as f64;
                        debug!("(Entity.update) Accelerate at {:0.3?} m/s for time {}", accel*G, duration);
                        debug!("(Entity.update) New velocity: {:0.0?} New position: {:0.0?}", self.velocity, self.position);
                    }
                }
                None
            }
            EntityKind::Planet { primary_ptr, .. } => {
                debug!("Updating planet {:?}", self.name);
                // This is the Gravitational Constant, not the acceleration due to gravity which is defined as G and used
                // more widely in this codebase.
                const G_CONST: f64 = 6.673e-11;

                if let Some(primary) = primary_ptr {
                    let primary = primary.read().unwrap();
                    let primary_mass = if let EntityKind::Planet {
                        mass: primary_mass, ..
                    } = primary.kind
                    {
                        primary_mass
                    } else {
                        unreachable!();
                    };

                    let primary_position = primary.position;
                    let primary_velocity = primary.velocity;

                    // We assume orbits are just on the x, z plane and around the primary.
                    let orbit_radius =
                        Vec3::new(1.0, 0.0, 1.0).mul_element_wise(self.position - primary_position);
                    let speed = (G_CONST * primary_mass / orbit_radius.magnitude()).sqrt();

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

                    self.velocity = tangent * speed + primary_velocity;
                    debug!("Planet {} velocity: {:?}", self.name, self.velocity);

                    // Now that we have velocity, move the planet!
                    //self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64
                    //  + primary_velocity * DELTA_TIME as f64;
                    self.position += (old_velocity + self.velocity) / 2.0 * DELTA_TIME as f64;
                }
                None
            }
            EntityKind::Missile {
                target_ptr,
                acceleration,
                burns,
                ..
            } => {
                debug!("Updating missile {:?}", self.name);
                // Using unwrap() below as it is an error condition if for some reason the target_ptr isn't set.
                let target = target_ptr.as_ref().unwrap().read().unwrap();
                if *burns > 0 {
                    debug!(
                        "Computing path for missile {} targeting {}: End pos: {:?} End vel: {:?}",
                        self.name, target.name, target.position, target.velocity
                    );

                    let params = TargetParams::new(
                        self.position,
                        target.position,
                        self.velocity,
                        target.velocity,
                        MAX_MISSILE_ACCELERATION,
                    );

                    debug!(
                        "Call targeting computer for missile {} with params: {:?}",
                        self.name, params
                    );

                    let mut path: FlightPathResult = compute_target_path(&params);
                    debug!("Computed path: {:?}", path);

                    // The computed path should be an acceleration towards the target.
                    // For a missile, we should always have a single accelertion (towards the target at full thrust).
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
                    *acceleration = accel;

                    let old_velocity: Vec3 = self.velocity;
                    self.velocity += accel * G * time as f64;
                    self.position += (old_velocity + self.velocity) / 2.0 * time as f64;
                    *burns -= 1;

                    // See if we impacted.
                    if (self.position - target.position).magnitude() < IMPACT_DISTANCE {
                        debug!("Missile {} impacted target {}", self.name, target.name);
                        Some(UpdateAction::ShipImpact {
                            ship: target.name.clone(),
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
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateAction {
    ShipImpact { ship: String, missile: String },
    ExhaustedMissile { name: String },
}

#[serde_as]
#[derive(Debug, Default)]
pub struct Entities {
    pub entities: HashMap<String, Arc<RwLock<Entity>>>,
    missile_cnt: u16,
}

impl PartialEq for Entities {
    fn eq(&self, other: &Self) -> bool {
        self.entities.len() == other.entities.len()
            && self.entities.keys().all(|k| other.entities.contains_key(k))
            && self.entities.keys().all(|k| {
                self.entities[k]
                    .read()
                    .unwrap()
                    .eq(&other.entities[k].read().unwrap())
            })
    }
}

impl Entities {
    pub fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(file_name)?;
        let reader = std::io::BufReader::new(file);
        let mut entities: Entities = serde_json::from_reader(reader)?;
        info!("Load scenario file \"{}\".", file_name);

        entities.fixup_pointers();

        for entity in entities.entities.values() {
            debug!("Loaded entity {:?}", entity.read().unwrap());
        }
        assert!(entities.validate(), "Scenario file failed validation");
        Ok(entities)
    }

    pub fn add_ship(&mut self, name: String, position: Vec3, velocity: Vec3, acceleration: Vec3) {
        let entity = Entity::new_ship(name, position, velocity, acceleration);
        self.add_entity(entity);
    }

    pub fn add_planet(
        &mut self,
        name: String,
        position: Vec3,
        color: String,
        primary: Option<String>,
        radius: f64,
        mass: f64,
    ) {
        debug!(
            "Add planet {} with primary {}",
            name,
            primary.as_ref().unwrap_or(&String::from("None"))
        );

        // FIXME: Is there a cleaner way to just assert that the primary exists and it must be of kind Planet?
        let (primary_ptr, dependency) = if let Some(primary_name) = &primary {
            // This is the case where this is a primary as noted by the fact primary (String name of primary) is not None

            // Look up the primary.  We need a pointer to this entity and then look into its dependency value.
            let entity = self.get(primary_name).unwrap_or_else(|| {
                panic!(
                    "Primary planet {} not found for planet {}.",
                    primary_name, name
                )
            });

            // Go get the dependency value.  At this point if this is a Planet its an error (cannot be a primary).
            if let EntityKind::Planet { dependency, .. } = &entity.read().unwrap().kind {
                (Some(entity.clone()), dependency + 1)
            } else {
                unreachable!();
            }
        } else {
            // If there is no primary then this is a "root" planet so has no Primary and dependency value is 0.
            (None, 0)
        };

        // Just a safety check to ensure we never have a pointer without a name of a primary or vis versa.
        assert!(
            primary_ptr.is_some() && primary.is_some()
                || primary_ptr.is_none() && primary.is_none()
        );

        let entity = Entity::new_planet(
            name,
            position,
            color,
            radius,
            mass,
            primary,
            primary_ptr,
            dependency,
        );
        self.add_entity(entity);
    }

    pub fn launch_missile(&mut self, source: String, target: String) {
        const DEFAULT_BURN: i32 = 2;

        // Could use a random number generator here for the name but that makes tests flakey (random)
        // So this counter used to distinguish missiles between the same source and target
        let id = self.missile_cnt;
        self.missile_cnt += 1;
        let name = format!("{}::{}::{:X}", source, target, id);
        let source_ptr = self
            .get(&source)
            .unwrap_or_else(|| panic!("Missile source {} not found for missile {}.", source, name))
            .clone();

        let source_entity = source_ptr.read().unwrap();

        let position = source_entity.position;
        let velocity = source_entity.velocity;

        let target_ptr = self
            .get(&target)
            .unwrap_or_else(|| panic!("Target {} not found for missile {}.", target, name))
            .clone();
        let entity = Entity::new_missile(
            name,
            source,
            target,
            target_ptr,
            position,
            velocity,
            DEFAULT_BURN,
        );
        self.add_entity(entity);
    }

    pub fn add_entity(&mut self, entity: Entity) {
        self.entities
            .insert(entity.name.clone(), Arc::new(RwLock::new(entity)));
    }

    pub fn remove(&mut self, name: &str) {
        self.entities.remove(name);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<RwLock<Entity>>> {
        self.entities.get(name)
    }

    pub fn set_flight_plan(&mut self, name: &str, plan: FlightPlan) {
        if let Some(entity) = self.entities.get_mut(name) {
            entity.write().unwrap().set_flight_plan(plan);
        } else {
            warn!(
                "Could not set acceleration for non-existent entity {}",
                name
            );
        }
    }

    pub fn update_all(&mut self) -> Vec<EffectMsg> {
        let (mut planets, other): (Vec<_>, Vec<_>) = self
            .entities
            .values()
            .partition(|e| matches!(e.read().unwrap().kind, EntityKind::Planet { .. }));

        planets.sort_by(|a, b| {
            let a_ent = a.read().unwrap();
            let b_ent = b.read().unwrap();
            match (&a_ent.kind, &b_ent.kind) {
                (
                    EntityKind::Planet {
                        dependency: dependency1,
                        ..
                    },
                    EntityKind::Planet {
                        dependency: dependency2,
                        ..
                    },
                ) => dependency1.cmp(dependency2),
                _ => unreachable!(),
            }
        });

        debug!("(Entities.update_all) Sorted planets: {:?}", planets);
        debug!("(Entities.update_all) Other: {:?}", other);

        for planet in planets.iter() {
            planet.write().unwrap().update();
        }

        let updates: Vec<_> = other
            .into_iter()
            .filter_map(|entity| entity.write().unwrap().update())
            .collect();
        let mut effects = Vec::<EffectMsg>::new();
        for update in updates.iter() {
            match update {
                UpdateAction::ExhaustedMissile { name } => {
                    debug!("Removing missile {}", name);
                    effects.push(EffectMsg {
                        position: self.get(name).unwrap().read().unwrap().get_position(),
                        kind: EXHAUSTED_MISSILE.to_string(),
                    });
                    self.remove(name);
                }
                UpdateAction::ShipImpact { ship, missile } => {
                    debug!("Missile impact on {} by missile {}.", ship, missile);
                    effects.push(EffectMsg {
                        position: self.get(ship).unwrap().read().unwrap().get_position(),
                        kind: SHIP_IMPACT.to_string(),
                    });
                    self.remove(ship);
                    self.remove(missile);
                }
            }
        }
        effects
    }

    pub fn validate(&self) -> bool {
        for entity in self.entities.values() {
            let ent = entity.read().unwrap();

            // Match any pattern that is invalid and return false.
            // If none match, then we return true at the end.
            match &ent.kind {
                EntityKind::Planet {
                    primary: Some(_),
                    primary_ptr: None,
                    ..
                } => return false,
                EntityKind::Planet {
                    primary: None,
                    primary_ptr: Some(_),
                    ..
                } => return false,
                EntityKind::Planet {
                    primary: Some(primary_name),
                    primary_ptr: Some(primary_ptr),
                    ..
                } => {
                    if primary_ptr.read().unwrap().name != *primary_name {
                        return false;
                    }
                }
                EntityKind::Missile {
                    target: _,
                    target_ptr: None,
                    ..
                } => return false,
                EntityKind::Missile {
                    target: target_name,
                    target_ptr: Some(target_ptr),
                    ..
                } => {
                    if target_ptr.read().unwrap().name != *target_name {
                        return false;
                    }
                }
                _ => {}
            }
        }
        true
    }

    pub fn fixup_pointers(&mut self) {
        for entity in self.entities.values() {
            let mut ent = entity.write().unwrap();
            let name = ent.name.clone();
            match &mut ent.kind {
                EntityKind::Planet {
                    primary: Some(primary),
                    primary_ptr,
                    ..
                } => {
                    let looked_up = self.get(primary).unwrap_or_else(|| {
                        panic!(
                            "Unable to find entity named {} as primary for {}",
                            primary, &name
                        )
                    });
                    primary_ptr.replace(looked_up.clone());
                }
                EntityKind::Missile {
                    target: target_name,
                    target_ptr,
                    ..
                } => {
                    let looked_up = self.get(target_name).unwrap_or_else(|| {
                        panic!(
                            "Unable to find entity named {} as target for {}",
                            target_name, &name
                        )
                    });
                    target_ptr.replace(looked_up.clone());
                }
                _ => {}
            }
        }
    }
}

impl Serialize for Entities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        //TODO: This makes a copy of every entity before serializing.  Not sure if there is a way to avoid it.
        let guts = self
            .entities
            .values()
            .map(|e| e.read().unwrap().clone())
            .collect::<Vec<Entity>>();
        guts.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Entities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let guts = Vec::<Entity>::deserialize(deserializer)?;
        Ok(Entities {
            entities: guts
                .into_iter()
                .map(|e| (e.name.clone(), Arc::new(RwLock::new(e))))
                .collect(),
            missile_cnt: 0,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Vector2, Zero};

    #[test]
    fn test_add_entity() {
        let mut entities = Entities::default();

        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::zero(),
            Vec3::zero(),
        );

        assert_eq!(entities.get("Ship1").unwrap().read().unwrap().name, "Ship1");
        assert_eq!(entities.get("Ship2").unwrap().read().unwrap().name, "Ship2");
        assert_eq!(entities.get("Ship3").unwrap().read().unwrap().name, "Ship3");
    }

    #[test]
    fn test_update_all() {
        let _ = pretty_env_logger::try_init();

        let mut entities = Entities::default();

        // Create entities with random positions and names
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(1.0, 1.0, 1.0);
        let acceleration2 = Vec3::new(2.0, 2.0, -2.0);
        let acceleration3 = Vec3::new(4.0, -1.0, -0.0);
        entities.set_flight_plan("Ship1", FlightPlan((acceleration1, 10000).into(), None));
        entities.set_flight_plan("Ship2", FlightPlan((acceleration2, 10000).into(), None));
        entities.set_flight_plan("Ship3", FlightPlan((acceleration3, 10000).into(), None));

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(44146000.0, 44147000.0, 44148000.0);
        let expected_position2 = Vec3::new(88294000.0, 88295000.0, -88284000.0);
        let expected_position3 = Vec3::new(176587000.0, -44137000.0, 9000.0);
        assert_eq!(
            entities.get("Ship1").unwrap().read().unwrap().position,
            expected_position1
        );
        assert_eq!(
            entities.get("Ship2").unwrap().read().unwrap().position,
            expected_position2
        );
        assert_eq!(
            entities.get("Ship3").unwrap().read().unwrap().position,
            expected_position3
        );
    }

    #[test]
    fn test_sun_update() {
        let _ = pretty_env_logger::try_init();

        let mut entities = Entities::default();

        // Create some planets and see if they move.
        entities.add_planet(
            String::from("Sun"),
            Vec3::zero(),
            String::from("blue"),
            None,
            6.371e6,
            6e24,
        );

        // Update the planet a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the position remains the same
        let expected_position = Vec3::new(0.0, 0.0, 0.0);
        assert_eq!(
            entities.get("Sun").unwrap().read().unwrap().position,
            expected_position
        );
    }
    #[test]
    // TODO: Add test to add a moon.
    fn test_planet_update() {
        let _ = pretty_env_logger::try_init();

        fn check_radius_and_y(
            pos: Vec3,
            primary: Vec3,
            expected_mag: f64,
            expected_y: f64,
        ) -> (bool, bool) {
            const TOLERANCE: f64 = 0.01;
            let radius = pos - primary;
            let radius_2d = Vector2::<f64>::new(radius.x, radius.z);

            debug!(
                "Radius_2d.magnitude(): {:?} vs Expected: {}",
                radius_2d.magnitude(),
                expected_mag
            );
            return (
                (radius_2d.magnitude() - expected_mag).abs() / expected_mag < TOLERANCE,
                radius.y == expected_y,
            );
        }

        let mut entities = Entities::default();

        const EARTH_RADIUS: f64 = 151.25e9;
        // Create some planets and see if they move.
        entities.add_planet(
            String::from("Planet1"),
            Vec3::new(EARTH_RADIUS, 2000000.0, 0.0),
            String::from("blue"),
            None,
            6.371e6,
            6e24,
        );
        entities.add_planet(
            String::from("Planet2"),
            Vec3::new(0.0, 5000000.0, EARTH_RADIUS),
            String::from("red"),
            None,
            3e7,
            3e23,
        );
        entities.add_planet(
            String::from("Planet3"),
            Vec3::new(
                EARTH_RADIUS / (2.0 as f64).sqrt(),
                8000.0,
                EARTH_RADIUS / (2.0 as f64).sqrt(),
            ),
            String::from("green"),
            None,
            4e6,
            1e26,
        );

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // FIXME: This isn't really testing what we want to test.
        // Fix it so we have real primaries and test the distance to those.
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet1").unwrap().read().unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                2000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet2").unwrap().read().unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                5000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities.get("Planet3").unwrap().read().unwrap().position,
                Vec3::zero(),
                EARTH_RADIUS,
                8000.0
            )
        );
    }

    // A test of deserializing a planet string.
    #[test]
    fn test_serialize_planet() {
        let _ = pretty_env_logger::try_init();

        let tst_planet = Entity::new_planet(
            String::from("Sun"),
            Vec3::zero(),
            String::from("yellow"),
            7e8,
            100.0,
            None,
            None,
            0,
        );

        let tst_str = serde_json::to_string(&tst_planet).unwrap();
        assert_eq!(
            tst_str,
            r#"{"name":"Sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"kind":{"Planet":{"color":"yellow","radius":700000000.0,"mass":100.0}}}"#
        );

        let tst_planet_2 = Entity::new_planet(
            String::from("planet2"),
            Vec3::zero(),
            String::from("red"),
            4e6,
            100.0,
            Some(String::from("planet1")),
            Some(Arc::new(RwLock::new(tst_planet))),
            1,
        );

        let tst_str = serde_json::to_string(&tst_planet_2).unwrap();
        assert_eq!(
            tst_str,
            r#"{"name":"planet2","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"kind":{"Planet":{"color":"red","radius":4000000.0,"mass":100.0,"primary":"planet1"}}}"#
        );

        // This is a special case of an planet.  It typically should never have a primary that is Some(...) but a primary_ptr that is None
        // However, the one exception is when it comes off the wire, which is what we are testing here.
        let tst_planet_3 = Entity::new_planet(
            String::from("planet2"),
            Vec3::zero(),
            String::from("red"),
            4e6,
            100.0,
            Some(String::from("planet1")),
            None,
            0,
        );

        let tst_str = r#"{"name":"planet2","position":[0,0,0],"velocity":[0.0,0.0,0.0],
        "kind":{"Planet":{"color":"red","radius":4e6,"mass":100.0,"primary":"planet1"}}}"#;
        let tst_planet_4 = serde_json::from_str::<Entity>(tst_str).unwrap();

        assert_eq!(tst_planet_3, tst_planet_4);
    }

    #[test]
    fn test_unordered_scenario_file() {
        let _ = pretty_env_logger::try_init();

        let entities = Entities::load_from_file("./tests/test-scenario.json").unwrap();
        assert!(entities.validate(), "Scenario file failed validation");
    }
}

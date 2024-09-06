use cgmath::{InnerSpace, Vector3};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::payloads::EffectMsg;
use serde_with::serde_as;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::{Arc, RwLock};

use crate::missile::Missile;
use crate::planet::Planet;
use crate::ship::{FlightPlan, Ship};
use crate::combat::{ attack, Weapon };

pub const DELTA_TIME: u64 = 1000;
pub const DEFAULT_ACCEL_DURATION: u64 = 10000;
// We will use 4 sig figs for every physics constant we import.
// This is the value of 1 (earth) gravity in m/s^2
pub const G: f64 = 9.807;
pub type Vec3 = Vector3<f64>;

pub trait Entity: Debug + PartialEq + Serialize + Send + Sync {
    fn get_name(&self) -> &str;
    fn set_name(&mut self, name: String);
    fn get_position(&self) -> Vec3;
    fn set_position(&mut self, position: Vec3);
    fn get_velocity(&self) -> Vec3;
    fn set_velocity(&mut self, velocity: Vec3);
    fn update(&mut self) -> Option<UpdateAction>;
}

#[derive(Serialize, Deserialize, Debug)]
pub enum UpdateAction {
    ShipImpact { ship: String, missile: String },
    ExhaustedMissile { name: String },
}

#[serde_as]
#[derive(Debug, Default)]
pub struct Entities {
    pub ships: HashMap<String, Arc<RwLock<Ship>>>,
    pub missiles: HashMap<String, Arc<RwLock<Missile>>>,
    pub planets: HashMap<String, Arc<RwLock<Planet>>>,
}

impl PartialEq for Entities {
    fn eq(&self, other: &Self) -> bool {
        self.ships.len() == other.ships.len()
            && self.missiles.len() == other.missiles.len()
            && self.planets.len() == other.planets.len()
            && self.ships.keys().all(|k| other.ships.contains_key(k))
            && self.missiles.keys().all(|k| other.missiles.contains_key(k))
            && self.planets.keys().all(|k| other.planets.contains_key(k))
            && self.ships.keys().all(|k| {
                self.ships[k]
                    .read()
                    .unwrap()
                    .eq(&other.ships[k].read().unwrap())
            })
            && self.missiles.keys().all(|k| {
                self.missiles[k]
                    .read()
                    .unwrap()
                    .eq(&other.missiles[k].read().unwrap())
            })
            && self.planets.keys().all(|k| {
                self.planets[k]
                    .read()
                    .unwrap()
                    .eq(&other.planets[k].read().unwrap())
            })
    }
}

impl Entities {
    pub fn len(&self) -> usize {
        self.ships.len() + self.missiles.len() + self.planets.len()
    }

    pub fn load_from_file(file_name: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let file = std::fs::File::open(file_name)?;
        let reader = std::io::BufReader::new(file);
        let mut entities: Entities = serde_json::from_reader(reader)?;
        info!("Load scenario file \"{}\".", file_name);

        entities.fixup_pointers();
        entities.reset_gravity_wells();

        for ship in entities.ships.values() {
            debug!("Loaded entity {:?}", ship.read().unwrap());
        }

        for planet in entities.planets.values() {
            debug!("Loaded entity {:?}", planet.read().unwrap());
        }

        for missile in entities.missiles.values() {
            debug!("Loaded entity {:?}", missile.read().unwrap());
        }
        assert!(entities.validate(), "Scenario file failed validation");
        Ok(entities)
    }

    pub fn add_ship(&mut self, name: String, position: Vec3, velocity: Vec3, acceleration: Vec3, usp: &str) {
        let ship = Ship::new(
            name.clone(),
            position,
            velocity,
            FlightPlan::acceleration(acceleration),
            usp.to_string().into(),
        );
        self.ships.insert(name, Arc::new(RwLock::new(ship)));
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
            "Add planet {} with position {:?},  color {:?}, primary {}, radius {:?}, mass {:?}, ",
            name,
            position,
            color,
            primary.as_ref().unwrap_or(&String::from("None")),
            radius,
            mass
        );

        let (primary_ptr, dependency) = if let Some(primary_name) = &primary {
            let primary = self.planets.get(primary_name).unwrap_or_else(|| {
                panic!(
                    "Primary planet {} not found for planet {}.",
                    primary_name, name
                )
            });

            (
                Some(primary.clone()),
                primary.read().unwrap().dependency + 1,
            )
        } else {
            (None, 0)
        };

        // A safety check to ensure we never have a pointer without a name of a primary or vis versa.
        assert!(
            primary_ptr.is_some() && primary.is_some()
                || primary_ptr.is_none() && primary.is_none()
        );

        let entity = Planet::new(
            name.clone(),
            position,
            color,
            radius,
            mass,
            primary,
            primary_ptr,
            dependency,
        );

        debug!("Added planet with fixed gravity wells {:?}", entity);
        self.planets.insert(name, Arc::new(RwLock::new(entity)));
    }

    pub fn launch_missile(&mut self, source: &str, target: &str) {
        const DEFAULT_BURN: i32 = 2;

        // Could use a random number generator here for the name but that makes tests flakey (random)
        // So this counter used to distinguish missiles between the same source and target
        let id = self.missiles.len();

        let name = format!("{}::{}::{:X}", source, target, id);
        let source_ptr = self
            .ships
            .get(source)
            .unwrap_or_else(|| panic!("Missile source {} not found for missile {}.", source, name))
            .clone();

        let target_ptr = self
            .ships
            .get(target)
            .unwrap_or_else(|| panic!("Target {} not found for missile {}.", target, name))
            .clone();

        let source_ship = source_ptr.read().unwrap();
        let target_ship = target_ptr.read().unwrap();
        let direction = (target_ship.get_position() - source_ship.get_position()).normalize();
        let offset = 1000000.0 * direction;

        let target_ptr = target_ptr.clone();

        let position = source_ship.get_position() + offset;
        let velocity = source_ship.get_velocity();

        let entity = Missile::new(
            name.clone(),
            source.to_string(),
            target.to_string(),
            target_ptr,
            position,
            velocity,
            DEFAULT_BURN,
        );
        self.missiles.insert(name, Arc::new(RwLock::new(entity)));
    }

    // Set the flight plan. Returns true if it was set. False if the plan was invalid for any reason.
    pub fn set_flight_plan(&mut self, name: &str, plan: &FlightPlan) -> bool {
        if let Some(entity) = self.ships.get_mut(name) {
            return entity.write().unwrap().set_flight_plan(plan);
        } else {
            warn!(
                "Could not set acceleration for non-existent entity {}",
                name
            );
            return false;
        }
    }

    pub fn update_all(&mut self) -> Vec<EffectMsg> {
        let mut planets = self.planets.values_mut().collect::<Vec<_>>();
        planets.sort_by(|a, b| {
            let a_ent = a.read().unwrap();
            let b_ent = b.read().unwrap();
            a_ent.dependency.cmp(&b_ent.dependency)
        });

        debug!("(Entities.update_all) Sorted planets: {:?}", planets);

        // If we have effects from planet updates this has to change and get a bit more complex (like missiles below)
        planets.iter().for_each(|planet| {
            planet.write().unwrap().update();
        });

        // If we have effects from planet updates this has to change and get a bit more complex (like missiles below)
        self.ships.iter().for_each(|(_, ship)| {
            ship.write().unwrap().update();
        });

        let mut cleanup_list = Vec::<String>::new();

        let effects = self
            .missiles
            .values_mut()
            .filter_map(|missile| {
                let mut missile = missile.write().unwrap();
                let update = missile.update();
                let missile_name = missile.get_name();
                let missile_pos = missile.get_position();
                let missile_source: &str = &missile.source;
                match update? {
                    UpdateAction::ShipImpact { ship, missile } => {
                        debug!("Missile impact on {} by missile {}.", ship, missile);
                        let ship_pos = self
                            .ships
                            .get(&ship)
                            .unwrap()
                            .read()
                            .unwrap()
                            .get_position()
                            .clone();

                        let mut target = self.ships.get(&ship).unwrap_or_else(|| panic!("Cannot find target {} for missile.", ship)).write().unwrap();
                        attack(0, 0, missile_source, &mut target, Weapon::Missile);
                        cleanup_list.push(missile);

                        Some(EffectMsg::ShipImpact { position: ship_pos })
                    }
                    UpdateAction::ExhaustedMissile { name } => {
                        assert!(name == missile_name);
                        debug!("Removing missile {}", name);
                        cleanup_list.push(name.clone());
                        Some(EffectMsg::ExhaustedMissile { position: missile_pos })
                    }
                }
            })
            .collect::<Vec<_>>();

        cleanup_list.iter().for_each(|name| {
            debug!("(Entities.update_all) Removing missile {}", name);
            self.missiles.remove(name);
        });

        effects
    }

    pub fn validate(&self) -> bool {
        for planet in self.planets.values() {
            let planet = planet.read().unwrap();

            if planet.dependency < 0 {
                return false;
            }

            match (&planet.primary, planet.primary_ptr.as_ref()) {
                (Some(_), None) => return false,
                (None, Some(_)) => return false,
                (Some(primary), Some(primary_ptr)) => {
                    if primary_ptr.read().unwrap().get_name() != primary {
                        return false;
                    }
                }
                _ => {}
            }
        }

        for missile in self.missiles.values() {
            let missile = missile.read().unwrap();
            if missile.target_ptr.is_none() {
                return false;
            } else {
                if missile
                    .target_ptr
                    .as_ref()
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_name()
                    != missile.target
                {
                    return false;
                }
            }
        }
        true
    }

    pub fn fixup_pointers(&mut self) {
        for planet in self.planets.values() {
            let mut planet = planet.write().unwrap();
            let name = planet.get_name().to_string();
            match &mut planet.primary {
                Some(primary) => {
                    let looked_up = self.planets.get(primary).unwrap_or_else(|| {
                        panic!(
                            "Unable to find entity named {} as primary for {}",
                            primary, &name
                        )
                    });
                    planet.primary_ptr.replace(looked_up.clone());
                }
                None => {}
            }
        }

        for missile in self.missiles.values() {
            let mut missile = missile.write().unwrap();
            let name = missile.get_name();
            let looked_up = self.ships.get(&missile.target).unwrap_or_else(|| {
                panic!(
                    "Unable to find entity named {} as target for {}",
                    missile.target, &name
                )
            });
            missile.target_ptr.replace(looked_up.clone());
        }
    }

    pub fn reset_gravity_wells(&mut self) {
        for planet in self.planets.values() {
            let mut planet = planet.write().unwrap();
            planet.reset_gravity_wells();
        }
    }
}

impl Serialize for Entities {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        struct Entities {
            ships: Vec<Ship>,
            missiles: Vec<Missile>,
            planets: Vec<Planet>,
        }

        let mut entities = Entities {
            ships: self
                .ships
                .values()
                .map(|s| s.read().unwrap().clone())
                .collect::<Vec<Ship>>(),
            missiles: self
                .missiles
                .values()
                .map(|m| m.read().unwrap().clone())
                .collect::<Vec<Missile>>(),
            planets: self
                .planets
                .values()
                .map(|p| p.read().unwrap().clone())
                .collect::<Vec<Planet>>(),
        };

        //The following sort_by is not necessary and adds inefficiency BUT ensures we serialize each item in the same order
        //each time. This makes writing tests a lot easier!
        entities
            .ships
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
        entities
            .missiles
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());
        entities
            .planets
            .sort_by(|a, b| a.get_name().partial_cmp(b.get_name()).unwrap());

        entities.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Entities {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Entities {
            #[serde(default)]
            ships: Vec<Ship>,
            #[serde(default)]
            missiles: Vec<Missile>,
            #[serde(default)]
            planets: Vec<Planet>,
        }

        let guts = Entities::deserialize(deserializer)?;
        Ok(crate::entity::Entities {
            ships: guts
                .ships
                .into_iter()
                .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
                .collect(),
            missiles: guts
                .missiles
                .into_iter()
                .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
                .collect(),
            planets: guts
                .planets
                .into_iter()
                .map(|e| (e.get_name().to_string(), Arc::new(RwLock::new(e))))
                .collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cgmath::{Vector2, Zero};
    use crate::ship::EXAMPLE_USP;
    use serde_json::json;
    use assert_json_diff::assert_json_eq;


    #[test]
    fn test_add_ship() {
        let mut entities = Entities::default();

        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1.0, 2.0, 3.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4.0, 5.0, 6.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7.0, 8.0, 9.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP
        );

        assert_eq!(
            entities
                .ships
                .get("Ship1")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship1"
        );
        assert_eq!(
            entities
                .ships
                .get("Ship2")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship2"
        );
        assert_eq!(
            entities
                .ships
                .get("Ship3")
                .unwrap()
                .read()
                .unwrap()
                .get_name(),
            "Ship3"
        );
    }

    #[test_log::test]
    fn test_update_all() {
        let _ = pretty_env_logger::try_init();

        let mut entities = Entities::default();

        // Create entities with random positions and names
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );

        // Assign random accelerations to entities
        let acceleration1 = Vec3::new(1.0, 1.0, 1.0);
        let acceleration2 = Vec3::new(2.0, 2.0, -2.0);
        let acceleration3 = Vec3::new(4.0, -1.0, -0.0);
        entities.set_flight_plan("Ship1", &FlightPlan((acceleration1, 10000).into(), None));
        entities.set_flight_plan("Ship2", &FlightPlan((acceleration2, 10000).into(), None));
        entities.set_flight_plan("Ship3", &FlightPlan((acceleration3, 10000).into(), None));

        // Update the entities a few times
        entities.update_all();
        entities.update_all();
        entities.update_all();

        // Validate the new positions for each entity
        let expected_position1 = Vec3::new(44132500.0, 44133500.0, 44134500.0);
        let expected_position2 = Vec3::new(88267000.0, 88268000.0, -88257000.0);
        let expected_position3 = Vec3::new(176533000.0, -44123500.0, 9000.0);
        assert_eq!(
            entities
                .ships
                .get("Ship1")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position1
        );
        assert_eq!(
            entities
                .ships
                .get("Ship2")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
            expected_position2
        );
        assert_eq!(
            entities
                .ships
                .get("Ship3")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
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
            entities
                .planets
                .get("Sun")
                .unwrap()
                .read()
                .unwrap()
                .get_position(),
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
                entities
                    .planets
                    .get("Planet1")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
                Vec3::zero(),
                EARTH_RADIUS,
                2000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities
                    .planets
                    .get("Planet2")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
                Vec3::zero(),
                EARTH_RADIUS,
                5000000.0
            )
        );
        assert_eq!(
            (true, true),
            check_radius_and_y(
                entities
                    .planets
                    .get("Planet3")
                    .unwrap()
                    .read()
                    .unwrap()
                    .get_position(),
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

        let tst_planet = Planet::new(
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
            r#"{"name":"Sun","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"yellow","radius":700000000.0,"mass":100.0}"#
        );

        let tst_planet_2 = Planet::new(
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
            r#"{"name":"planet2","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"color":"red","radius":4000000.0,"mass":100.0,"primary":"planet1"}"#
        );

        // This is a special case of an planet.  It typically should never have a primary that is Some(...) but a primary_ptr that is None
        // However, the one exception is when it comes off the wire, which is what we are testing here.
        let tst_planet_3 = Planet::new(
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
        "color":"red","radius":4e6,"mass":100.0,"primary":"planet1"}"#;
        let tst_planet_4 = serde_json::from_str::<Planet>(tst_str).unwrap();

        assert_eq!(tst_planet_3, tst_planet_4);
    }

    #[test_log::test]
    fn test_mixed_entities_serialize() {
        let mut entities = Entities::default();

        // This constant is the radius of the earth's orbit (distance from sun).
        // It is NOT the radius of the earth (6.371e6 m)
        const EARTH_RADIUS: f64 = 151.25e9;
        // Create some planets and see if they move.
        entities.add_planet(
            String::from("Planet1"),
            Vec3::new(EARTH_RADIUS, 2000000.0, 0.0),
            String::from("blue"),
            None,
            6.371e6,
            5.972e24,
        );
        entities.add_planet(
            String::from("Planet2"),
            Vec3::new(0.0, 5000000.0, EARTH_RADIUS),
            String::from("red"),
            None,
            3e7,
            3.00e23,
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

        // Create entities with random positions and names
        entities.add_ship(
            String::from("Ship1"),
            Vec3::new(1000.0, 2000.0, 3000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship2"),
            Vec3::new(4000.0, 5000.0, 6000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );
        entities.add_ship(
            String::from("Ship3"),
            Vec3::new(7000.0, 8000.0, 9000.0),
            Vec3::zero(),
            Vec3::zero(),
            EXAMPLE_USP,
        );

        let cmp = json!({
            "ships":[
                {"name":"Ship1","position":[1000.0,2000.0,3000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"usp":"38266C2-30060-B", "hull":6,"structure":6},
                {"name":"Ship2","position":[4000.0,5000.0,6000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"usp":"38266C2-30060-B", "hull":6,"structure":6},
                {"name":"Ship3","position":[7000.0,8000.0,9000.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"usp":"38266C2-30060-B", "hull":6,"structure":6}],
            "missiles":[],
            "planets":[
                {"name":"Planet1","position":[151250000000.0,2000000.0,0.0],"velocity":[0.0,0.0,0.0],"color":"blue","radius":6371000.0,"mass":5.972e24,
                "gravity_radius_1":6375069.342849095,"gravity_radius_05":9015709.525726125,"gravity_radius_025":12750138.68569819},
                {"name":"Planet2","position":[0.0,5000000.0,151250000000.0],"velocity":[0.0,0.0,0.0],"color":"red","radius":30000000.0,"mass":3.00e23},
                {"name":"Planet3","position":[106949900654.4653,8000.0,106949900654.4653],"velocity":[0.0,0.0,0.0],"color":"green","radius":4000000.0,"mass":1e26,
                "gravity_radius_2":18446331.779326223,"gravity_radius_1":26087052.57835697,"gravity_radius_05":36892663.558652446,"gravity_radius_025":52174105.15671394}
             ]});

        assert_json_eq!(&entities, &cmp);
    }
    #[test]
    fn test_unordered_scenario_file() {
        let _ = pretty_env_logger::try_init();

        let entities = Entities::load_from_file("./tests/test-scenario.json").unwrap();
        assert!(entities.validate(), "Scenario file failed validation");
    }
}

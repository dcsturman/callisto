use std::collections::HashMap;
use std::result::Result;
use std::sync::{Arc, Mutex};

use cgmath::InnerSpace;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use crate::authentication::Authenticator;
use crate::computer::FlightParams;
use crate::entity::{deep_clone, Entities, Entity, G};
use crate::payloads::{
    AddPlanetMsg, AddShipMsg, AuthResponse, ComputePathMsg, FireActionsMsg, LoginMsg,
    RemoveEntityMsg, SetCrewActions, SetPlanMsg,
};
use crate::ship::{Ship, ShipDesignTemplate, SHIP_TEMPLATES};

use crate::{debug, info, warn};

// Struct wrapping an Arc<Mutex<Entities>> (i.e. a multi-threaded safe Entities)
// Add function beyond what Entities does and provides an API to our server.
pub struct Server {
    entities: Arc<Mutex<Entities>>,
    test_mode: bool,
}

impl Server {
    pub fn new(entities: Arc<Mutex<Entities>>, test_mode: bool) -> Self {
        Server {
            entities,
            test_mode,
        }
    }

    pub async fn login(
        &self,
        login: LoginMsg,
        valid_email: &Option<String>,
        authenticator: Arc<Option<Authenticator>>,
    ) -> Result<(String, Option<String>), String> {
        info!(
            "(Server.login) Received and processing login request. {:?}",
            &login
        );

        // Authenticator can only legally be None if we are in test mode.
        if authenticator.is_none() {
            let test_auth_response = crate::payloads::AuthResponse {
                email: "test@test.com".to_string(),
            };
            return Ok((serde_json::to_string(&test_auth_response).unwrap(), None));
        }

        // Three cases. 1) if there's a valid email, just let the client know what it is.
        // 2) If there is a code then we do authentication via Google OAuth2.
        // 3) this isn't authenticated and we need to force reauthentication.  We do that
        // by returning an error and eventually a 401 to the client.
        if let Some(email) = valid_email {
            let auth_response = AuthResponse {
                email: email.clone(),
            };
            Ok((serde_json::to_string(&auth_response).unwrap(), None))
        } else if let Some(code) = login.code {
            // This is a bit weird so explaining:
            // First as_ref() is for Arc<..>, which gives us an Option<Authenticator>.  The second
            // is for the Option.
            let authenticator = authenticator.as_ref().as_ref().unwrap();

            let (session_key, email) = authenticator
                .authenticate_google_user(&code)
                .await
                .unwrap_or_else(|e| panic!("(Server.login) Unable to authenticate user: {:?}", e));
            debug!(
                "(Server.login) Authenticated user {} with session key  {}.",
                email, session_key
            );

            let auth_response = AuthResponse { email };
            Ok((
                serde_json::to_string(&auth_response).unwrap(),
                Some(session_key),
            ))
        } else {
            Err("Must reauthenticate.".to_string())
        }
    }

    pub fn add_ship(&self, ship: AddShipMsg) -> Result<String, String> {
        info!(
            "(Server.add_ship) Received and processing add ship request. {:?}",
            ship
        );

        // Add the ship to the server
        let design = crate::ship::SHIP_TEMPLATES
            .get()
            .expect("(Server.add_ship) Ship templates not loaded")
            .get(&ship.design)
            .unwrap_or_else(|| panic!("(Server.add_ship) Could not find design {}.", ship.design));
        self.entities.lock().unwrap().add_ship(
            ship.name,
            ship.position,
            ship.velocity,
            ship.acceleration,
            design.clone(),
            ship.crew,
        );

        Ok("Add ship action executed".to_string())
    }

    pub fn set_crew_actions(&self, request: SetCrewActions) -> Result<String, String> {
        let entities = self
            .entities
            .lock()
            .map_err(|_e| "Unable to obtain lock on Entities.")?;

        let mut ship = entities
            .ships
            .get(&request.ship_name)
            .ok_or("Unable to find ship to set agility for.".to_string())?
            .write()
            .unwrap();

        // Go through each possible action in SetCrewActions, one by one.
        if request.dodge_thrust.is_some() || request.assist_gunners.is_some() {
            ship.set_pilot_actions(request.dodge_thrust, request.assist_gunners)
                .map_err(|e| e.get_msg())?;
        }
        Ok("Set crew action executed".to_string())
    }

    pub fn get_entities(&self) -> Result<Entities, String> {
        Ok(self.entities.lock().unwrap().clone())
    }

    pub fn get_designs(&self) -> Result<String, String> {
        // Strip the Arc, etc. from the ShipTemplates before marshalling back.
        let clean_templates: HashMap<String, ShipDesignTemplate> = SHIP_TEMPLATES
            .get()
            .expect("(Server.get_designs) Ship templates not loaded")
            .iter()
            .map(|(key, value)| (key.clone(), (*value.clone()).clone()))
            .collect();

        Ok(serde_json::to_string(&clean_templates).unwrap())
    }

    pub fn add_planet(&self, planet: AddPlanetMsg) -> Result<String, String> {
        // Add the planet to the server
        self.entities.lock().unwrap().add_planet(
            planet.name,
            planet.position,
            planet.color,
            planet.primary,
            planet.radius,
            planet.mass,
        );

        Ok("Add planet action executed".to_string())
    }

    pub fn remove(&self, name: RemoveEntityMsg) -> Result<String, String> {
        // Remove the entity from the server
        let mut entities = self.entities.lock().unwrap();
        if entities.ships.remove(&name).is_none()
            && entities.planets.remove(&name).is_none()
            && entities.missiles.remove(&name).is_none()
        {
            warn!("Unable to find entity named {} to remove", name);
            let err_msg = format!("Unable to find entity named {} to remove", name);
            return Err(err_msg);
        }

        Ok("Remove action executed".to_string())
    }

    pub fn set_plan(&self, plan_msg: SetPlanMsg) -> Result<(), String> {
        // Change the acceleration of the entity
        self.entities
            .lock()
            .unwrap()
            .set_flight_plan(&plan_msg.name, &plan_msg.plan)
    }

    pub fn update(&mut self, fire_actions: FireActionsMsg) -> Result<String, String> {
        let mut rng = get_rng(self.test_mode);

        // Grab the lock on entities
        let mut entities = self
            .entities
            .lock()
            .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {}", e));

        // Take a snapshot of all the ships.  We'll use this for attackers while
        // damage goes directly onto the "official" ships.  But it means if they are damaged
        // or destroyed they still get to take their actions.
        let ship_snapshot: HashMap<String, Ship> = deep_clone(&entities.ships);

        // 1. This method will make a clone of all ships to use as attacker while impacting damage on the primary copy of ships.  This way ships still get ot attack
        // even when damaged.  This gives us a "simultaneous" attack semantics.
        // 2. Add all new missiles into the entities structure.
        // 3. Then update all the entities.  Note this means ship movement is after combat so a ship with degraded maneuver might not move as much as expected.
        // Its not clear to me if this is the right order - or should they move then take damage - but we'll do it this way for now.
        // 3. Return a set of effects

        let mut effects = entities.fire_actions(fire_actions, &ship_snapshot, &mut rng);

        // 4. Update all entities (ships, planets, missiles) and gather in their effects.
        effects.append(&mut entities.update_all(&ship_snapshot, &mut rng));

        debug!("(/update) Effects: {:?}", effects);

        // 5. Marshall the events and reply with them back to the user.
        let json = match serde_json::to_string(&effects) {
            Ok(json) => json,
            Err(_) => return Err("Error converting update actions to JSON".to_string()),
        };

        // 6. Reset all ship agility setting as the round is over.
        for ship in entities.ships.values() {
            ship.write().unwrap().reset_crew_actions();
        }

        Ok(json)
    }

    pub fn compute_path(&self, msg: ComputePathMsg) -> Result<String, String> {
        info!(
            "(/compute_path) Received and processing compute path request. {:?}",
            msg
        );

        debug!(
            "(/compute_path) Computing path for entity: {} End pos: {:?} End vel: {:?}",
            msg.entity_name, msg.end_pos, msg.end_vel
        );
        // Do this in a block to clean up the lock as soon as possible.
        let (start_pos, start_vel, max_accel) = {
            let entities = self.entities.lock().unwrap();
            let entity = entities
                .ships
                .get(&msg.entity_name)
                .unwrap()
                .read()
                .unwrap();
            (
                entity.get_position(),
                entity.get_velocity(),
                entity.max_acceleration() * G,
            )
        };

        let adjusted_end_pos = if msg.standoff_distance > 0.0 {
            msg.end_pos - (msg.end_pos - start_pos).normalize() * msg.standoff_distance
        } else {
            msg.end_pos
        };

        if msg.standoff_distance > 0.0 {
            debug!("(/compute_path) Standoff distance: {:0.0?} Adjusted end pos: {:0.0?} Original end pos {:0.0?}Difference {:0.0?}", msg.standoff_distance, adjusted_end_pos, msg.end_pos, 
                    (adjusted_end_pos - msg.end_pos).magnitude());
        }

        let params = FlightParams::new(
            start_pos,
            adjusted_end_pos,
            start_vel,
            msg.end_vel,
            msg.target_velocity,
            max_accel,
        );

        debug!("(/compute_path)Call computer with params: {:?}", params);

        let plan = if let Some(plan) = params.compute_flight_path() {
            debug!("(/compute_path) Plan: {:?}", plan);
            plan
        } else {
            return Err("Unable to compute flight path".to_string());
        };

        debug!(
            "(/compute_path) Plan has real acceleration of {} vs max_accel of {}",
            plan.plan.0 .0.magnitude(),
            max_accel / G
        );

        let json = match serde_json::to_string(&plan) {
            Ok(json) => json,
            Err(_) => return Err("Error converting flight path to JSON".to_string()),
        };

        debug!("(/compute_path) Flight path response: {}", json);

        Ok(json)
    }

    pub fn get(&self) -> Result<String, String> {
        info!("Received and processing get request.");
        match serde_json::to_string::<Entities>(&self.entities.lock().unwrap()) {
            Ok(json) => {
                info!("(/) Entities: {:?}", json);
                Ok(json)
            }
            Err(_) => Err("Error converting entities to JSON".to_string()),
        }
    }
}

fn get_rng(test_mode: bool) -> Box<SmallRng> {
    if test_mode {
        info!("(lib.get_rng) Server in TEST mode for random numbers (constant seed of 0).");
        // Use 0 to seed all test case random number generators.
        Box::new(SmallRng::seed_from_u64(0))
    } else {
        debug!("(lib.get_rng) Server in standard mode for random numbers.");
        Box::new(SmallRng::from_entropy())
    }
}

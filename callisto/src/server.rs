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
    AddPlanetMsg, AddShipMsg, AuthResponse, ComputePathMsg, FireActionsMsg, LoadScenarioMsg,
    LoginMsg, RemoveEntityMsg, SetCrewActions, SetPlanMsg, SimpleMsg,
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
        authenticator: Arc<Box<dyn Authenticator>>,
    ) -> Result<(AuthResponse, Option<String>), String> {
        info!("(Server.login) Received and processing login request.",);

        // Three cases. 1) if there's a valid email, just let the client know what it is.
        // 2) If there is a code then we do authentication via Google OAuth2.
        // 3) this isn't authenticated and we need to force reauthentication.  We do that
        // by returning an error and eventually a 401 to the client.
        if let Some(email) = valid_email {
            let auth_response = AuthResponse {
                email: email.clone(),
            };
            Ok((auth_response, None))
        } else if let Some(code) = login.code {
            let (session_key, email) = authenticator
                .authenticate_user(&code)
                .await
                .map_err(|e| format!("(Server.login) Unable to authenticate user: {:?}", e))?;
            debug!(
                "(Server.login) Authenticated user {} with session key.",
                email
            );

            let auth_response = AuthResponse { email };
            Ok((auth_response, Some(session_key)))
        } else {
            Err("Must reauthenticate.".to_string())
        }
    }

    /// Adds a ship to the entities.
    /// 
    /// # Arguments
    /// * `ship` - The message containing the parameters for the ship.
    /// 
    /// # Errors
    /// Returns an error if the ship design cannot be found.
    /// 
    /// # Panics
    /// Panics if the ship templates are not loaded or if the lock on entities cannot be obtained.
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
            .ok_or_else(|| format!("(Server.add_ship) Could not find design {}.", ship.design))?;

        self.entities.lock().unwrap().add_ship(
            ship.name,
            ship.position,
            ship.velocity,
            ship.acceleration,
            design.clone(),
            ship.crew,
        );

        Ok(msg_json("Add ship action executed"))
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
        Ok(msg_json("Set crew action executed"))
    }

    pub fn get_entities(&self) -> Result<Entities, String> {
        Ok(self.entities.lock().unwrap().clone())
    }

    pub fn get_designs(&self) -> String {
        // Strip the Arc, etc. from the ShipTemplates before marshalling back.
        let clean_templates: HashMap<String, ShipDesignTemplate> = SHIP_TEMPLATES
            .get()
            .expect("(Server.get_designs) Ship templates not loaded")
            .iter()
            .map(|(key, value)| (key.clone(), (*value.clone()).clone()))
            .collect();

        serde_json::to_string(&clean_templates).unwrap()
    }

    /// Adds a planet to the entities.
    /// 
    /// # Arguments
    /// * `planet` - The message containing the parameters for the planet.
    /// 
    /// # Errors
    /// Returns an error if the planet already exists in the entities.
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to write the entities.
    pub fn add_planet(&self, planet: AddPlanetMsg) -> Result<String, String> {
        // Add the planet to the server
        self.entities.lock().unwrap().add_planet(
            planet.name,
            planet.position,
            planet.color,
            planet.primary,
            planet.radius,
            planet.mass,
        )?;

        Ok(msg_json("Add planet action executed"))
    }

    /// Removes an entity from the entities.
    /// 
    /// # Arguments
    /// * `name` - The name of the entity to remove.
    /// 
    /// # Errors
    /// Returns an error if the name entity does not exist in the list of entities.
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to write the entities.
    pub fn remove(&self, name: &RemoveEntityMsg) -> Result<String, String> {
        // Remove the entity from the server
        let mut entities = self.entities.lock().unwrap();
        if entities.ships.remove(name).is_none()
            && entities.planets.remove(name).is_none()
            && entities.missiles.remove(name).is_none()
        {
            warn!("Unable to find entity named {} to remove", name);
            let err_msg = format!("Unable to find entity named {} to remove", name);
            return Err(err_msg);
        }

        Ok(msg_json("Remove action executed"))
    }

    /// Sets the flight plan for a ship.
    /// 
    /// # Arguments
    /// * `plan_msg` - The message containing the parameters for the flight plan.
    /// 
    /// # Errors
    /// Returns an error if the flight plan is one that is not legal for this ship (e.g. acceleration is too high)
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities.
    pub fn set_plan(&self, plan_msg: &SetPlanMsg) -> Result<String, String> {
        // Change the acceleration of the entity
        self.entities
            .lock()
            .unwrap()
            .set_flight_plan(&plan_msg.name, &plan_msg.plan)
            .map(|()| msg_json("Set acceleration action executed"))
    }

    /// Update all the entities by having actions occur.  This includes all the innate actions for each entity
    /// (e.g. move a ship, planet or missile) as well as new fire actions.
    /// 
    /// # Arguments
    /// * `fire_actions` - The fire actions to execute.
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities.
    pub fn update(&mut self, fire_actions: FireActionsMsg) -> String {
        let mut rng = get_rng(self.test_mode);

        // Grab the lock on entities
        let mut entities = self
            .entities
            .lock()
            .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

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
        let json = serde_json::to_string(&effects)
            .unwrap_or_else(|_| panic!("Unable to serialize `effects` {:?}.", effects));

        // 6. Reset all ship agility setting as the round is over.
        for ship in entities.ships.values() {
            ship.write().unwrap().reset_crew_actions();
        }

        json
    }

    /// Computes a flight path for a ship.
    /// 
    /// # Arguments
    /// * `msg` - The message containing the parameters for the flight path.
    /// 
    /// # Errors
    /// Returns an error if the computer cannot find a valid flight path (solve the non-linear equations)
    /// or if we cannot marshall the flight path into JSON (should never happen).
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities.
    pub fn compute_path(&self, msg: &ComputePathMsg) -> Result<String, String> {
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
                G * f64::from(entity.max_acceleration()),
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

        let Some(plan) = params.compute_flight_path() else {
            return Err(format!("Unable to compute flight path: {:?}", params));
        };

        debug!("(/compute_path) Plan: {:?}", plan);
        debug!(
            "(/compute_path) Plan has real acceleration of {} vs max_accel of {}",
            plan.plan.0 .0.magnitude(),
            max_accel / G
        );

        let Ok(json) = serde_json::to_string(&plan) else {
            return Err("Error converting flight path to JSON".to_string());
        };

        debug!("(/compute_path) Flight path response: {}", json);

        Ok(json)
    }

    /// Loads a scenario file.      
    /// 
    /// # Errors
    /// Returns an error if the scenario file cannot be loaded (e.g. doesn't exist)
    /// 
    /// # Panics
    /// Panics if the lock cannot be obtained to write the entities.  Not clear when this might happen,
    /// especially given this routine is run only on server initialization.
    pub async fn load_scenario(&self, msg: &LoadScenarioMsg) -> Result<String, String> {
        info!(
            "(/load_scenario) Received and processing load scenario request. {:?}",
            msg
        );

        let entities = Entities::load_from_file(&msg.scenario_name)
            .await
            .map_err(|e| e.to_string())?;
        *self.entities.lock().unwrap() = entities;

        Ok(msg_json("Load scenario action executed"))
    }

    /// Gets the current entities and serializes it to JSON.
    /// 
    /// # Panics
    /// Panics if for some reason it cannot serialize the entities correctly.
    #[must_use] pub fn get_entities_json(&self) -> String {
        info!("Received and processing get entities request.");
        let json = serde_json::to_string::<Entities>(&self.entities.lock().unwrap())
            .expect("(server.get) Unable to serialize entities");

        info!("(/) Entities: {:?}", json);
        json
    }
}

fn get_rng(test_mode: bool) -> SmallRng {
    if test_mode {
        info!("(lib.get_rng) Server in TEST mode for random numbers (constant seed of 0).");
        // Use 0 to seed all test case random number generators.
        SmallRng::seed_from_u64(0)
    } else {
        debug!("(lib.get_rng) Server in standard mode for random numbers.");
        SmallRng::from_entropy()
    }
}

pub(crate) fn msg_json(msg: &str) -> String {
    serde_json::to_string(&SimpleMsg {
        msg: msg.to_string(),
    })
    .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authentication::MockAuthenticator;
    use crate::payloads::LoginMsg;
    use std::sync::Arc;

    #[test_log::test(tokio::test)]
    async fn test_login() {
        let server = Server::new(Arc::new(Mutex::new(Entities::new())), false);
        let mock_auth = MockAuthenticator::new(
            "http://test.com",
            "secret".to_string(),
            "users.txt",
            "http://web.test.com".to_string(),
        );

        let authenticator = Arc::new(Box::new(mock_auth) as Box<dyn Authenticator>);

        // Test case 1: Already valid email
        let valid_email = Some("existing@example.com".to_string());
        let login_msg = LoginMsg { code: None };
        let (auth_response, session_key) = server
            .login(login_msg, &valid_email, authenticator.clone())
            .await
            .expect("Login should succeed with valid email");

        assert_eq!(auth_response.email, "existing@example.com");
        assert!(session_key.is_none());

        // Test case 2: New login with auth code
        let valid_email = None;
        let login_msg = LoginMsg {
            code: Some("test_code".to_string()),
        };
        let (auth_response, session_key) = server
            .login(login_msg, &valid_email, authenticator.clone())
            .await
            .expect("Login should succeed with auth code");
        assert_eq!(auth_response.email, "test@example.com");
        assert_eq!(session_key.unwrap(), "TeSt_KeY");

        // Test case 3: No valid email and no auth code
        let valid_email = None;
        let login_msg = LoginMsg { code: None };
        let result = server
            .login(login_msg, &valid_email, authenticator.clone())
            .await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Must reauthenticate.".to_string());
    }
}

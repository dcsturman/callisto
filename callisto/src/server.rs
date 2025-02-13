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
    AddPlanetMsg, AddShipMsg, AuthResponse, ComputePathMsg, EffectMsg, FireActionsMsg,
    FlightPathMsg, LoadScenarioMsg, LoginMsg, RemoveEntityMsg, SetCrewActions, SetPlanMsg,
    ShipDesignTemplateMsg,
};
use crate::ship::{Ship, ShipDesignTemplate, SHIP_TEMPLATES};

use crate::{debug, info, warn};

// Struct wrapping an Arc<Mutex<Entities>> (i.e. a multi-threaded safe Entities)
// Add function beyond what Entities does and provides an API to our server.
pub struct Server {
    entities: Arc<Mutex<Entities>>,
    authenticator: Box<dyn Authenticator>,
    test_mode: bool,
}

impl Server {
    pub fn new(
        entities: Arc<Mutex<Entities>>,
        authenticator: Box<dyn Authenticator>,
        test_mode: bool,
    ) -> Self {
        Server {
            entities,
            authenticator,
            test_mode,
        }
    }

    #[must_use]
    pub fn in_test_mode(&self) -> bool {
        self.test_mode
    }

    /// Returns a clone of the entities.
    /// # Panics
    /// Panics if the lock on entities cannot be obtained.
    #[must_use]
    pub fn clone_entities(&self) -> Entities {
        self.entities.lock().unwrap().clone()
    }

    /// Authenticates a user.
    ///
    /// This function handles the login process, including authentication and session key management.
    /// In the common case, it checks that the user has previously been authenticated and has a valid
    /// session key.  It then returns the user's email. The session key is returned only if its is newly created
    /// (so not in this case).
    ///
    /// Otherwise it looks for a valid referral code (from Google `OAuth2`) and uses that to build a session
    /// key.  It then returns the user's email and the newly minted session key.
    ///
    /// # Arguments
    /// * `login` - The login message, possibly containing the referral code.
    /// * `valid_email` - The session cookie, if one exists.
    ///
    /// # Errors
    /// Returns an error if the user cannot be authenticated.
    pub async fn login(
        &mut self,
        login: LoginMsg,
        session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    ) -> Result<AuthResponse, String> {
        info!("(Server.login) Received and processing login request.",);

        let email = self
            .authenticator
            .authenticate_user(&login.code, session_keys)
            .await
            .map_err(|e| format!("(Server.login) Unable to authenticate user: {e:?}"))?;

        debug!(
            "(Server.login) Authenticated user {} with session key.",
            email
        );

        Ok(AuthResponse { email })
    }

    /// Logs a user out by clearing the session key and email.
    ///
    /// # Arguments
    /// * `session_keys` - The session keys for all connections.  This is a map of session keys to email addresses.  Used here when a user logs out (to remove the session key).
    ///
    /// # Panics
    /// Panics if the lock on `session_keys` cannot be obtained.
    pub fn logout(&mut self, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>) {
        info!("(Server.logout) Received and processing logout request.",);
        self.authenticator.set_email(None);
        let mut keys = session_keys.lock().unwrap();
        if let Some(session_key) = self.authenticator.get_session_key() {
            keys.remove(&session_key);
        }
    }

    /// Returns true if the user has been validated.
    #[must_use]
    pub fn validated_user(&self) -> bool {
        self.authenticator.validated_user()
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
            design,
            ship.crew,
        );

        Ok("Add ship action executed".to_string())
    }

    /// Sets the crew actions for a ship.
    ///
    /// # Arguments
    /// * `request` - The message containing the parameters for the ship.
    ///
    /// # Errors
    /// Returns an error if the ship cannot be found.
    ///
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities or we cannot obtain a write
    /// lock on the ship in question.
    pub fn set_crew_actions(&self, request: &SetCrewActions) -> Result<String, String> {
        let entities = self
            .entities
            .lock()
            .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

        let mut ship = entities
            .ships
            .get(&request.ship_name)
            .ok_or("Unable to find ship to set agility for.".to_string())?
            .write()
            .unwrap_or_else(|e| panic!("Unable to obtain write lock on ship: {e}"));

        // Go through each possible action in SetCrewActions, one by one.
        if request.dodge_thrust.is_some() || request.assist_gunners.is_some() {
            ship.set_pilot_actions(request.dodge_thrust, request.assist_gunners)
                .map_err(|e| e.get_msg())?;
        }
        Ok("Set crew action executed".to_string())
    }

    /// Gets the current entities and returns them in a `Result`.
    ///
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities.
    #[must_use]
    pub fn get_entities(&self) -> Entities {
        self.entities.lock().unwrap().clone()
    }

    /// Get the entities marshalled into JSON
    ///
    /// # Panics
    /// Panics if entities cannot be converted.
    #[must_use]
    pub fn get_entities_json(&self) -> String {
        serde_json::to_string(&*self.entities.lock().unwrap()).unwrap()
    }

    /// Gets the ship designs and serializes it to JSON.
    ///
    /// # Panics
    /// Panics if the ship templates have not been loaded.
    pub fn get_designs(&self) -> ShipDesignTemplateMsg {
        // Strip the Arc, etc. from the ShipTemplates before marshalling back.
        let clean_templates: HashMap<String, ShipDesignTemplate> = SHIP_TEMPLATES
            .get()
            .expect("(Server.get_designs) Ship templates not loaded")
            .iter()
            .map(|(key, value)| (key.clone(), (*value.clone()).clone()))
            .collect();

        clean_templates
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

        Ok("Add planet action executed".to_string())
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
            let err_msg = format!("Unable to find entity named {name} to remove");
            return Err(err_msg);
        }

        Ok("Remove action executed".to_string())
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
            .map(|()| "Set acceleration action executed".to_string())
    }

    /// Update all the entities by having actions occur.  This includes all the innate actions for each entity
    /// (e.g. move a ship, planet or missile) as well as new fire actions.
    ///
    /// # Arguments
    /// * `fire_actions` - The fire actions to execute.
    ///
    /// # Panics
    /// Panics if the lock cannot be obtained to read the entities.
    pub fn update(&mut self, fire_actions: &FireActionsMsg) -> Vec<EffectMsg> {
        let mut rng = get_rng(self.test_mode);

        debug!("(/update) Fire actions: {:?}", fire_actions);

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

        // 5. Reset all ship agility setting as the round is over.
        for ship in entities.ships.values() {
            ship.write().unwrap().reset_crew_actions();
        }

        effects
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
    pub fn compute_path(&self, msg: &ComputePathMsg) -> Result<FlightPathMsg, String> {
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
                .ok_or_else(|| {
                    format!(
                        "Cannot compute flightpath for unknown ship named '{}'",
                        msg.entity_name
                    )
                })?
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

        debug!("(/compute_path) Call computer with params: {:?}", params);

        let Some(plan) = params.compute_flight_path() else {
            return Err(format!("Unable to compute flight path: {params:?}"));
        };

        debug!("(/compute_path) Plan: {:?}", plan);
        debug!(
            "(/compute_path) Plan has real acceleration of {} vs max_accel of {}",
            plan.plan.0 .0.magnitude(),
            max_accel / G
        );

        Ok(plan)
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

        Ok("Load scenario action executed".to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authentication::MockAuthenticator;
    use crate::payloads::LoginMsg;
    use std::sync::Arc;

    #[test_log::test(tokio::test)]
    async fn test_login() {
        let mock_auth = MockAuthenticator::new("http://web.test.com");
        let authenticator = Box::new(mock_auth) as Box<dyn Authenticator>;

        let mut server = Server::new(Arc::new(Mutex::new(Entities::new())), authenticator, false);

        // Try a login
        let login_msg = LoginMsg {
            code: MockAuthenticator::mock_valid_code(),
        };

        let session_keys = Arc::new(Mutex::new(HashMap::new()));
        let auth_response = server
            .login(login_msg, &session_keys)
            .await
            .expect("Login should succeed with valid email");

        assert_eq!(auth_response.email, "test@example.com");

        // No connection established in this test, so there should be no session keys.
        assert_eq!(session_keys.lock().unwrap().len(), 0);
    }
}

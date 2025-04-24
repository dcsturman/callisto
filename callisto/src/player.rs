use std::collections::HashMap;
use std::result::Result;
use std::sync::{Arc, Mutex};

use cgmath::InnerSpace;
use itertools::multiunzip;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use crate::action::{merge, ShipAction};
use crate::authentication::Authenticator;
use crate::computer::FlightParams;
use crate::entity::{Entities, Entity, G};
use crate::payloads::{
  AddPlanetMsg, AddShipMsg, AuthResponse, ChangeRole, ComputePathMsg, EffectMsg, FlightPathMsg, LoadScenarioMsg,
  LoginMsg, RemoveEntityMsg, Role, SetPilotActions, SetPlanMsg, ShipActionMsg, ShipDesignTemplateMsg
};
use crate::server::Server;
use crate::ship::{Ship, ShipDesignTemplate, SHIP_TEMPLATES};
use crate::{debug, info, warn};

// Struct wrapping an Arc<Mutex<Entities>> (i.e. a multi-threaded safe Entities)
// Add function beyond what Entities does and provides an API to our server.
pub struct PlayerManager {
  unique_id: u64,
  // Server holding most importantly the state of the server, shared between all players.
  // The state is entities, if we're in tutorial mode, and the initial state of the server so we can revert.
  // `server` is an [`Option`] because it may not be initialized until later in the server's lifecycle (via a client message).
  pub server: Option<Arc<Server>>,
  // Authenticator for this player.  It contains the session key and email identity of the player.
  authenticator: Box<dyn Authenticator>,
  // Role this player might have assumed
  role: Role,
  // Ship this player may have assumed a crew position on.
  ship: Option<String>,
  test_mode: bool,
}

impl PlayerManager {
  /// Create a new player manager.
  #[must_use]
  pub fn new(unique_id: u64, server: Option<Arc<Server>>, authenticator: Box<dyn Authenticator>, test_mode: bool) -> Self {
    PlayerManager {
      unique_id,
      server,
      authenticator,
      test_mode,
      role: Role::General,
      ship: None,
    }
  }

  pub fn set_server(&mut self, server: Arc<Server>) {
    self.server = Some(server);
  }

  #[must_use]
  pub fn in_test_mode(&self) -> bool {
    self.test_mode
  }

  #[must_use]
  pub fn get_id(&self) -> u64 {
    self.unique_id
  }

  /// Returns a clone of the entities.
  /// # Panics
  /// Panics if the lock on entities cannot be obtained or if the server hasn't
  /// been initialized.
  #[must_use]
  pub fn clone_entities(&self) -> Entities {
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().clone()
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
    &mut self, login: LoginMsg, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<AuthResponse, String> {
    info!("(PlayerManager.login) Received and processing login request.",);

    let email = self
      .authenticator
      .authenticate_user(&login.code, session_keys)
      .await
      .map_err(|e| format!("(PlayerManager.login) Unable to authenticate user: {e:?}"))?;

    debug!("(PlayerManager.login) Authenticated user {} with session key.", email);

    Ok(AuthResponse { email })
  }

  /// Reset a server to its initial configuration.
  ///
  /// # Errors
  /// Returns an error if the user is not in the General role.
  ///
  /// # Panics
  /// Panics if the lock on entities cannot be obtained or if the server has never been initialized.
  pub fn reset(&self) -> Result<String, String> {
    if self.role == Role::General && self.ship.is_none() {
      info!("(PlayerManager.reset) Received and processing reset request: Resetting server!");
      self
        .server
        .as_ref()
        .unwrap()
        .initial_scenario
        .deep_copy_into(&mut self.server.as_ref().unwrap().get_unlocked_entities().unwrap());
      Ok("Server reset.".to_string())
    } else {
      warn!(
        "(PlayerManager.reset) Received and processing reset request: Ignoring reset request as not in General role."
      );
      Err("Not GM. Cannot reset server!".to_string())
    }
  }

  /// Logs a user out by clearing the session key and email.
  ///
  /// # Arguments
  /// * `session_keys` - The session keys for all connections.  This is a map of session keys to email addresses.  Used here when a user logs out (to remove the session key).
  ///
  /// # Panics
  /// Panics if the lock on `session_keys` cannot be obtained.
  pub fn logout(&mut self, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>) {
    info!("(PlayerManager.logout) Received and processing logout request.",);
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
  /// Panics if the ship templates are not loaded, if the ship design cannot be found, if the lock on entities cannot be obtained,
  /// or if the server hasn't yet been initialized.
  pub fn add_ship(&self, ship: AddShipMsg) -> Result<String, String> {
    info!("(PlayerManager.add_ship) Received and processing add ship request. {:?}", ship);

    // Add the ship to the server
    let design = crate::ship::SHIP_TEMPLATES
      .get()
      .expect("(PlayerManager.add_ship) Ship templates not loaded")
      .get(&ship.design)
      .ok_or_else(|| format!("(PlayerManager.add_ship) Could not find design {}.", ship.design))?;

    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().add_ship(
      ship.name,
      ship.position,
      ship.velocity,
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
  /// Panics if the lock cannot be obtained to read the entities, if we cannot obtain a write
  /// lock on the ship in question, or if the server has not yet been initialized.
  pub fn set_pilot_actions(&self, request: &SetPilotActions) -> Result<String, String> {
    let entities = self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

    let mut ship = entities
      .ships
      .get(&request.ship_name)
      .ok_or("Unable to find ship to set agility for.".to_string())?
      .write()
      .unwrap_or_else(|e| panic!("Unable to obtain write lock on ship: {e}"));

    // Go through each possible action in SetCrewActions, one by one.
    if request.dodge_thrust.is_some() || request.assist_gunners.is_some() {
      ship
        .set_pilot_actions(request.dodge_thrust, request.assist_gunners)
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
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().clone()
  }

  /// Get the entities marshalled into JSON
  ///
  /// # Panics
  /// Panics if entities cannot be converted.
  #[must_use]
  pub fn get_entities_json(&self) -> String {
    serde_json::to_string(&*self.server.as_ref().unwrap().get_unlocked_entities().unwrap()).unwrap()
  }

  /// Gets the ship designs and serializes it to JSON.
  ///
  /// # Panics
  /// Panics if the ship templates have not been loaded.
  pub fn get_designs() -> ShipDesignTemplateMsg {
    // Strip the Arc, etc. from the ShipTemplates before marshalling back.
    let clean_templates: HashMap<String, ShipDesignTemplate> = SHIP_TEMPLATES
      .get()
      .expect("(PlayerManager.get_designs) Ship templates not loaded")
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
  /// Panics if the lock cannot be obtained to write the entities or if the
  /// server has not yet been initialized.
  pub fn add_planet(&self, planet: AddPlanetMsg) -> Result<String, String> {
    // Add the planet to the server
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().add_planet(
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
  /// Panics if the lock cannot be obtained to write the entities or if the server
  /// has not yet been initialized.
  pub fn remove(&self, name: &RemoveEntityMsg) -> Result<String, String> {
    // Remove the entity from the server
    let mut entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
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
  /// Panics if the lock cannot be obtained to read the entities or if the server
  /// has not yet been initialized.
  pub fn set_plan(&self, plan_msg: &SetPlanMsg) -> Result<String, String> {
    // Change the acceleration of the entity
    self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap()
      .set_flight_plan(&plan_msg.name, &plan_msg.plan)
      .map(|()| "Set acceleration action executed".to_string())
  }

  /// Merge in new actions (orders) for ships in the next round.  These may come for the same ship from
  /// different clients depending on how the clients are being used.  We save these till the next update action.
  ///
  /// # Returns
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read the entities or if the server
  /// has not yet been initialized.
  #[allow(clippy::must_use_candidate)]
  pub fn merge_actions(&self, actions: ShipActionMsg) -> String {
    let mut entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
    merge(&mut entities, actions);
    "Actions added.".to_string()
  }

  /// Update all the entities by having actions occur.  This includes all the innate actions for each entity
  /// (e.g. move a ship, planet or missile) as well as new fire actions.
  ///
  /// # Panics  
  /// Panics if the lock cannot be obtained to read the entities or if the server
  /// has not yet been initialized.
  #[must_use]
  pub fn update(&self) -> Vec<EffectMsg> {
    let mut rng = get_rng(self.test_mode);

    // Grab the lock on entities
    let mut entities = self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

    let actions = &entities.actions;
    debug!("(/update) Ship actions: {:?}", actions);

    // Sort all the actions by type.  This big messy functional action sorts through all the ship actions and creates
    // three vectors with all the fire actions for each ship in the first, the sensor actions for each ship in the second, and
    // ships making jump attempts in the third
    // Was very explicit with types here (more than necessary) to make it easier to read and understand.
    #[allow(clippy::type_complexity)]
    let (fire_actions, sensor_actions, jump_actions): (
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
    ) = multiunzip(actions.iter().filter_map(|(ship_name, actions)| {
      if !entities.ships.contains_key(ship_name) {
        warn!("(update) Cannot find ship {} for actions.", ship_name);
        return None;
      }
      let (f_actions, s_actions, j_actions): (
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
      ) = multiunzip(actions.iter().map(|action| match action {
        ShipAction::FireAction { .. } | ShipAction::DeleteFireAction { .. } => (Some(action.clone()), None, None),
        ShipAction::JamMissiles
        | ShipAction::BreakSensorLock { .. }
        | ShipAction::SensorLock { .. }
        | ShipAction::JamComms { .. } => (None, Some(action.clone()), None),
        ShipAction::Jump => (None, None, Some(action.clone())),
      }));
      Some((
        (ship_name.clone(), f_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), s_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), j_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
      ))
    }));

    // Take a snapshot of all the ships.  We'll use this for attackers while
    // damage goes directly onto the "official" ships.  But it means if they are damaged
    // or destroyed they still get to take their actions.
    let ship_snapshot: HashMap<String, Ship> = entities.ship_deep_copy();

    // First process all sensor actions. They can remove missiles and change modifiers for ship combat.
    let mut effects = entities.sensor_actions(&sensor_actions, &mut rng);

    // 1. This method will make a clone of all ships to use as attacker while impacting damage on the primary copy of ships.  This way ships still get ot attack
    // even when damaged.  This gives us a "simultaneous" attack semantics.
    // 2. Add all new missiles into the entities structure.
    // 3. Then update all the entities.  Note this means ship movement is after combat so a ship with degraded maneuver might not move as much as expected.
    // Its not clear to me if this is the right order - or should they move then take damage - but we'll do it this way for now.
    // 3. Return a set of effects
    effects.append(&mut entities.fire_actions(&fire_actions, &ship_snapshot, &mut rng));

    // 4. Update all entities (ships, planets, missiles) and gather in their effects.
    effects.append(&mut entities.update_all(&ship_snapshot, &mut rng));

    // 5. Attempt jumps at end of round.
    effects.append(&mut entities.attempt_jumps(&jump_actions, &mut rng));

    // 6. Reset all ship agility setting as the round is over.
    for ship in entities.ships.values() {
      ship.write().unwrap().reset_pilot_actions();
    }
    entities.actions.clear();

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
  /// Panics if the lock cannot be obtained to read the entities or if the server
  /// has not yet been initialized.
  pub fn compute_path(&self, msg: &ComputePathMsg) -> Result<FlightPathMsg, String> {
    info!("(/compute_path) Received and processing compute path request. {:?}", msg);

    info!(
      "(/compute_path) Computing path for entity: {} End pos: {:?} End vel: {:?} Target vel: {:?} Target accel: {:?}",
      msg.entity_name, msg.end_pos, msg.end_vel, msg.target_velocity, msg.target_acceleration
    );
    // Do this in a block to clean up the lock as soon as possible.
    let (start_pos, start_vel, max_accel) = {
      let entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
      let entity = entities
        .ships
        .get(&msg.entity_name)
        .ok_or_else(|| format!("Cannot compute flightpath for unknown ship named '{}'", msg.entity_name))?
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

    let mut params = FlightParams::new(
      start_pos,
      adjusted_end_pos,
      start_vel,
      msg.end_vel,
      msg.target_velocity,
      msg.target_acceleration,
      max_accel,
    );

    debug!("(/compute_path) Call computer with params: {:?}", params);

    let Ok(plan) = params.compute_flight_path() else {
      return Err(format!("Unable to compute flight path: {params:?}"));
    };

    debug!("(/compute_path) Plan: {:?}", plan);
    debug!(
      "(/compute_path) Plan has real acceleration of {} vs max_accel of {}",
      plan.plan.0 .0.magnitude(),
      max_accel
    );

    Ok(plan)
  }

  // TODO: Get rid of this.  Can be replaced by choosing a Tutorial scenario.
  /// Loads a scenario file into an existing server.      
  ///
  /// # Errors
  /// Returns an error if the scenario file cannot be loaded (e.g. doesn't exist)
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write the entities.  Not clear when this might happen,
  /// especially given this routine is run only on server initialization.
  pub async fn load_scenario(&self, msg: &LoadScenarioMsg) -> Result<String, String> {
    info!("(/load_scenario) Received and processing load scenario request. {:?}", msg);

    let entities = Entities::load_from_file(&msg.scenario_name).await.map_err(|e| e.to_string())?;

    // HACK: but this is going away.
    *self.server.as_ref().unwrap().get_unlocked_entities().unwrap() = entities;

    Ok("Load scenario action executed".to_string())
  }

  #[must_use]
  pub fn get_email(&self) -> Option<String> {
    self.authenticator.get_email()
  }

  #[must_use]
  pub fn get_role(&self) -> (Role, Option<String>) {
    (self.role, self.ship.clone())
  }

  pub fn set_role(&mut self, msg: &ChangeRole) -> String {
    self.role = msg.role;
    self.ship.clone_from(&msg.ship);
    "Role set".to_string()
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

    let mut server = PlayerManager::new(0, None, authenticator, false);

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

use std::collections::{HashMap, HashSet};
use std::result::Result;
use std::sync::{Arc, Mutex};

use cgmath::InnerSpace;
use itertools::multiunzip;
use rand::rngs::SmallRng;
use rand::SeedableRng;

use crate::action::{boost_target_alive, boost_target_sort_key, merge, BoostMap, BoostTarget, ShipAction};
use crate::authentication::Authenticator;
use crate::computer::FlightParams;
use crate::entity::{Entities, Entity, G};
use crate::payloads::{
  AddPlanetMsg, AddShipMsg, AuthResponse, CaptainActionMsg, CaptainActionResult, ChangeRole, ComputePathMsg, EffectMsg,
  FlightPathMsg, LoginMsg, RemoveEntityMsg, RenameEntityMsg, Role, SetPilotActions, SetPlanMsg, SetShipEmissions,
  SetShipTeam, ShipActionMsg, ShipDesignTemplateMsg,
};
use crate::server::Server;
use crate::ship::{get_ship_templates_snapshot, Ship, ShipDesignTemplate, Weapon, WeaponMount};
use crate::{debug, info, warn};

/// Most weapons we will accept on a single ship.  Generous compared to any real
/// design; it exists only so a client cannot ask us to allocate without bound.
const MAX_SHIP_WEAPONS: usize = 64;

/// Check client-supplied armament before it reaches the entity table.
///
/// # Errors
/// Returns an error if the list is too long or holds a turret that isn't
/// single, double or triple.  Anything else panics later in combat resolution
/// when the weapon is named (see `impl From<&Weapon> for String`).
fn validate_weapons(weapons: &[Weapon]) -> Result<(), String> {
  if weapons.len() > MAX_SHIP_WEAPONS {
    return Err(format!(
      "(PlayerManager.add_ship) Ship has {} weapons, more than the limit of {MAX_SHIP_WEAPONS}.",
      weapons.len()
    ));
  }

  weapons
    .iter()
    .find_map(|weapon| match weapon.mount {
      // A turret's size is now the number of guns in it.
      WeaponMount::Turret if !(1..=3).contains(&weapon.guns.len()) => Some(weapon.guns.len()),
      // Every other mount holds exactly one weapon.
      WeaponMount::Barbette | WeaponMount::Bay(_) | WeaponMount::FixedMount | WeaponMount::Battery(_)
        if weapon.guns.len() != 1 =>
      {
        Some(weapon.guns.len())
      }
      _ => None,
    })
    .map_or(Ok(()), |size| {
      Err(format!(
        "(PlayerManager.add_ship) Illegal mount holding {size} weapons; a turret holds 1 to 3 and every other mount exactly 1."
      ))
    })
}

/// `PlayerManager` represents a distinct user connected to the server.
/// It can belong to a single `Server` at a time, or to none.
pub struct PlayerManager {
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
  pub fn new(server: Option<Arc<Server>>, authenticator: Box<dyn Authenticator>, test_mode: bool) -> Self {
    PlayerManager {
      server,
      authenticator,
      test_mode,
      role: Role::General,
      ship: None,
    }
  }

  pub fn set_role_ship(&mut self, role: Role, ship: Option<String>) {
    self.role = role;
    self.ship = ship;
  }

  pub fn set_server(&mut self, server: Arc<Server>) {
    self.server = Some(server);
  }

  #[must_use]
  pub fn in_test_mode(&self) -> bool {
    self.test_mode
  }

  #[must_use]
  pub fn get_session_key(&self) -> Option<String> {
    self.authenticator.get_session_key()
  }

  /// Returns a deep clone of the entities, propagating any
  /// `fixup_pointers` failure to the caller instead of panicking.
  ///
  /// # Errors
  /// Returns an error if the cloned entities have dangling references
  /// (e.g. a missile targeting a removed ship, or a planet whose primary
  /// no longer exists). Live state should never reach that condition if
  /// `Player::remove` is used correctly, but propagation keeps a buggy
  /// state from crashing the tokio worker.
  ///
  /// # Panics
  /// Panics if the lock on entities cannot be obtained or if the server
  /// hasn't been initialized.
  pub fn clone_entities(&self) -> Result<Entities, String> {
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().deep_copy()
  }

  /// Authenticates a user.
  ///
  /// This function handles the login process by checking the code passed from
  /// Google Authentication. If one doesn't exist it generates a session key.
  /// # Arguments
  /// * `login` - The login message, possibly containing the referral code.
  /// * `session_keys` - The session keys for all connections.  This is a map of session keys to email addresses.  Used here when a user logs in (to update this info)
  ///
  /// # Errors
  /// Returns an error if the user cannot be authenticated. For blacklisted
  /// emails the error string is `"NOT_AUTHORIZED"` (the wire-pinned tag the
  /// FE branches on); other failures use the legacy "Unable to authenticate"
  /// wording.
  pub async fn login(
    &mut self, login: LoginMsg, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<AuthResponse, String> {
    info!("(PlayerManager.login) Received and processing login request.",);

    let email = self
      .authenticator
      .authenticate_user(&login.code, session_keys)
      .await
      .map_err(|e| {
        // BlacklistedUserError's Display IS the wire contract: "NOT_AUTHORIZED".
        // Keep it pinned by checking the type rather than relying on the
        // stringified Debug below.
        if e.is::<crate::authentication::BlacklistedUserError>() {
          "NOT_AUTHORIZED".to_string()
        } else {
          format!("(PlayerManager.login) Unable to authenticate user: {e:?}")
        }
      })?;

    debug!("(PlayerManager.login) Authenticated user {} with session key.", email);

    Ok(AuthResponse {
      email,
      scenario: None,
      role: None,
      ship: None,
    })
  }

  /// Register a new user. Mirrors `login` but reaches `register_user` on the
  /// authenticator so it can persist the email into the authorized-users
  /// directory. The `Err(String)` carries one of the wire-pinned tags
  /// `NOT_AUTHORIZED`, `ALREADY_REGISTERED`, `AUTH_FAILED`, or
  /// `REGISTRATION_FAILED` — `RegisterError`'s `Display` is the contract.
  ///
  /// # Errors
  /// Returns an error if the user cannot be registered (see above).
  pub async fn register(
    &mut self, msg: LoginMsg, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<AuthResponse, String> {
    info!("(PlayerManager.register) Received and processing register request.");

    let email = self
      .authenticator
      .register_user(&msg.code, session_keys)
      .await
      .map_err(|e| e.to_string())?;

    Ok(AuthResponse {
      email,
      scenario: None,
      role: None,
      ship: None,
    })
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
      // initial_scenario was validated at scenario-load time, so this
      // shouldn't fail in practice. Propagate the error rather than
      // panicking so a malformed-on-disk scenario surfaces as a clean
      // reset-failure response instead of a worker crash.
      self
        .server
        .as_ref()
        .unwrap()
        .initial_scenario
        .deep_copy_into(&mut self.server.as_ref().unwrap().get_unlocked_entities().unwrap())?;
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
  /// Returns an error if the ship design cannot be found or if the requested armament is illegal.
  ///
  /// # Panics
  /// Panics if the ship templates are not loaded, if the ship design cannot be found, if the lock on entities cannot be obtained,
  /// or if the server hasn't yet been initialized.
  pub fn add_ship(&self, ship: AddShipMsg) -> Result<String, String> {
    info!("(PlayerManager.add_ship) Received and processing add ship request. {:?}", ship);

    // Add the ship to the server
    let design = self
      .server
      .as_ref()
      .unwrap()
      .get_ship_template(&ship.design)
      .ok_or_else(|| format!("(PlayerManager.add_ship) Could not find design {}.", ship.design))?;

    if let Some(weapons) = &ship.weapons {
      validate_weapons(weapons)?;
    }

    let mut entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
    let name = ship.name.clone();
    entities.add_ship(ship.name, ship.position, ship.velocity, &design, ship.crew, ship.weapons);

    // Applied after creation rather than threaded through `add_ship`, which
    // already carries six arguments. Absent leaves the normal running state a
    // new ship is built with.
    if ship.active_sensors.is_some() || ship.transmitting.is_some() || ship.team.is_some() || ship.contacts.is_some() {
      if let Some(added) = entities.ships.get(&name) {
        let mut added = added.write().unwrap();
        added.set_emissions(ship.active_sensors, ship.transmitting);
        if ship.team.is_some() {
          added.team = ship.team;
        }
        if let Some(contacts) = ship.contacts {
          added.contacts = contacts;
          added.contacts.sort();
        }
      }
    }

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

  /// Set whether a ship runs its active sensors.
  ///
  /// # Errors
  /// Returns an error if the ship cannot be found.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained on the entities or the ship, or if
  /// the server has not yet been initialized.
  pub fn set_ship_emissions(&self, request: &SetShipEmissions) -> Result<String, String> {
    let entities = self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

    let mut ship = entities
      .ships
      .get(&request.ship_name)
      .ok_or_else(|| format!("Unable to find ship {} to set emissions for.", request.ship_name))?
      .write()
      .unwrap_or_else(|e| panic!("Unable to obtain write lock on ship: {e}"));

    // Hand-off first: it can force transmitting on, and set_emissions honours
    // that, so applying them the other way round would let a request turn
    // hand-off on and transmitting off in the same breath.
    if let Some(handoff) = request.handoff_sensors {
      ship.set_handoff_sensors(handoff);
    }
    let locks_dropped = ship.set_emissions(request.active_sensors, request.transmitting);

    info!(
      "(PlayerManager.set_ship_emissions) {} now has active sensors {}, transmitting {}, hand-off {}.",
      request.ship_name, ship.active_sensors, ship.transmitting, ship.handoff_sensors
    );

    if locks_dropped {
      Ok(format!("{} went dark; its sensor locks were dropped.", request.ship_name))
    } else {
      Ok("Set ship emissions executed".to_string())
    }
  }

  /// Mark every ship as having found every other one.
  ///
  /// Scenarios open with no contacts: ships have to find each other, and the
  /// first detection pass runs at the end of the opening round. This is the
  /// hook for authoring a scenario that begins already engaged rather than as
  /// an approach, and for tests that are about something other than
  /// acquisition.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained on the entities, or if the server
  /// has not yet been initialized.
  pub fn establish_initial_contacts(&self) {
    self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"))
      .establish_initial_contacts();
  }

  /// Set which side a ship is on.
  ///
  /// # Errors
  /// Returns an error if the ship cannot be found.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained on the entities or the ship, or if
  /// the server has not yet been initialized.
  pub fn set_ship_team(&self, request: &SetShipTeam) -> Result<String, String> {
    let entities = self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

    let mut ship = entities
      .ships
      .get(&request.ship_name)
      .ok_or_else(|| format!("Unable to find ship {} to set team for.", request.ship_name))?
      .write()
      .unwrap_or_else(|e| panic!("Unable to obtain write lock on ship: {e}"));

    ship.team = request.team;
    info!(
      "(PlayerManager.set_ship_team) {} is now on team {:?}.",
      request.ship_name, request.team
    );
    Ok("Set ship team executed".to_string())
  }

  /// Gets the current entities and returns them in a `Result`.
  ///
  /// # Errors
  /// Returns an error if the cloned entities have dangling references —
  /// see [`Self::clone_entities`].
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read the entities.
  pub fn get_entities(&self) -> Result<Entities, String> {
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().deep_copy()
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
  /// Always returns the LIVE global registry, not the scenario-server's
  /// frozen snapshot. The wire payload feeds the client's design dropdown,
  /// which must reflect the latest set after a watcher reload (e.g. a new
  /// design file added to the bucket). The per-server snapshot is kept for
  /// scenario-internal lookups where stability matters; the wire path does
  /// not need that stability.
  ///
  /// # Panics
  /// Panics if the ship templates have not been loaded.
  #[must_use]
  pub fn get_designs(&self) -> ShipDesignTemplateMsg {
    let templates = get_ship_templates_snapshot();

    // Strip the Arc, etc. from the ShipTemplates before marshalling back.
    let clean_templates: HashMap<String, ShipDesignTemplate> = templates
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
  /// Returns an error if the requested primary planet cannot be found.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write the entities or if the
  /// server has not yet been initialized.
  pub fn add_planet(&self, planet: AddPlanetMsg) -> Result<String, String> {
    info!(
      "(PlayerManager.add_planet) Received and processing add planet request. {:?}",
      planet
    );

    // Add the planet to the server
    self.server.as_ref().unwrap().get_unlocked_entities().unwrap().add_planet(
      planet.name,
      planet.position,
      planet.color,
      planet.primary,
      planet.radius,
      planet.mass,
      planet.visual_effects,
    )?;

    Ok("Add planet action executed".to_string())
  }

  /// Removes an entity from the entities, keeping internal references
  /// consistent so a subsequent deep-clone doesn't blow up in
  /// `fixup_pointers`.
  ///
  /// * Planets: refused if any other planet's `primary` field references
  ///   this planet. The operator must re-parent (rename) or remove the
  ///   children first. Cascade-orphaning would leave moons floating in
  ///   space silently — preferring an actionable error.
  /// * Ships: also drops any in-flight missile whose `target` was this
  ///   ship. Those missiles can no longer hit anything, and leaving them
  ///   in `entities.missiles` with a dangling target name would panic the
  ///   next deep-clone (`target_ptr` fixup fails).
  /// * Missiles: removed directly; nothing else references them by name.
  ///
  /// # Arguments
  /// * `name` - The name of the entity to remove.
  ///
  /// # Errors
  /// Returns an error if the entity does not exist, or if the entity is a
  /// planet that is the primary of another planet.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to write the entities, if the
  /// server has not yet been initialized, or if a planet/missile `RwLock`
  /// is poisoned.
  pub fn remove(&self, name: &RemoveEntityMsg) -> Result<String, String> {
    let mut entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();

    if entities.planets.contains_key(name) {
      // Find any planet (excluding the target itself) that orbits this one.
      let dependent = entities
        .planets
        .iter()
        .find(|(other_name, planet)| {
          other_name.as_str() != name.as_str() && planet.read().unwrap().primary.as_deref() == Some(name.as_str())
        })
        .map(|(other, _)| other.clone());
      if let Some(child) = dependent {
        let err_msg =
          format!("Cannot remove planet {name}: {child} orbits it as primary. Re-parent or remove children first.");
        warn!("{err_msg}");
        return Err(err_msg);
      }
      entities.planets.remove(name);
      return Ok("Remove action executed".to_string());
    }

    if entities.ships.remove(name).is_some() {
      // Drop any missile that was targeting this ship — its target_ptr
      // would otherwise be unresolvable on next deep-clone. Shared with the
      // combat and jump paths, which used to miss this.
      entities.prune_orphaned_missiles();
      // Nobody keeps a contact or a lock on a ship that is no longer here.
      entities.prune_ship_references();
      return Ok("Remove action executed".to_string());
    }

    if entities.missiles.remove(name).is_some() {
      return Ok("Remove action executed".to_string());
    }

    warn!("Unable to find entity named {} to remove", name);
    Err(format!("Unable to find entity named {name} to remove"))
  }

  /// Rename a ship or planet in the active scenario in place. See
  /// [`Entities::rename`] for the semantics and constraints.
  ///
  /// # Errors
  /// Returns an error if the rename fails (collision, empty name,
  /// missing target, missile target, etc.).
  ///
  /// # Panics
  /// Panics if the server has not yet been initialized or the entities
  /// lock cannot be obtained.
  pub fn rename(&self, msg: &RenameEntityMsg) -> Result<String, String> {
    let mut entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
    entities.rename(&msg.current, &msg.new_name)
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
    debug!("(/merge_actions) Merging actions: {:?}", actions);
    merge(&mut entities, actions);
    debug!("(/merge_actions) Resulting actions after merge: {:?}", entities.actions);
    "Actions added.".to_string()
  }

  /// Roll the captain's leadership check immediately. Stores the resulting
  /// points on the ship until `reset_temporary_bonuses` clears them at end
  /// of turn. The captain has up to N boosts to apply; assigning more than
  /// N is allowed but only the first N (per `boost_target_sort_key`) take
  /// effect at end of turn.
  ///
  /// If the ship is missing, returns a result with `points: 0` and an error
  /// message — the FE renders that the same way as a failed roll.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained on `Entities` or if the server
  /// is not initialized.
  #[must_use]
  pub fn captain_action(&self, msg: &CaptainActionMsg) -> CaptainActionResult {
    let mut rng = get_rng(self.test_mode);
    let entities = self.server.as_ref().unwrap().get_unlocked_entities().unwrap();
    let Some(ship) = entities.ships.get(&msg.ship_name) else {
      return CaptainActionResult {
        ship_name: msg.ship_name.clone(),
        points: 0,
        message: format!("Ship {} not found.", msg.ship_name),
      };
    };
    let leadership = i16::from(ship.read().unwrap().get_crew().get_leadership());
    let roll = i16::from(crate::combat::roll_dice(2, &mut rng));
    let points = roll + leadership - 8;
    ship.write().unwrap().set_leadership_points(points);

    let message = if points > 0 {
      format!(
        "Captain on {} rolled {points}: can inspire {points} task{}.",
        msg.ship_name,
        if points == 1 { "" } else { "s" },
      )
    } else {
      format!("Captain on {} rolled {points}: cannot boost tasks this turn.", msg.ship_name)
    };

    CaptainActionResult {
      ship_name: msg.ship_name.clone(),
      points,
      message,
    }
  }

  /// Update all the entities by having actions occur.  This includes all the innate actions for each entity
  /// (e.g. move a ship, planet or missile) as well as new fire actions.
  ///
  /// # Panics
  /// Panics if the lock cannot be obtained to read the entities or if the server
  /// has not yet been initialized.
  #[must_use]
  #[allow(clippy::too_many_lines)]
  pub fn update(&self) -> Vec<EffectMsg> {
    let mut rng = get_rng(self.test_mode);

    // Grab the lock on entities
    let mut entities = self
      .server
      .as_ref()
      .unwrap()
      .get_unlocked_entities()
      .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {e}"));

    // Phase 0: Resolve queued LeadershipCheck actions to build the BoostMap
    // for this turn. The leadership dice roll itself happens BEFORE end-of-turn
    // (when the captain hits the "Captain Action" button — see
    // `PlayerManager::captain_action`). The rolled effect lives on
    // `ship.leadership_points` until reset_temporary_bonuses clears it. Here
    // we just truncate the captain's queued boost list to the cached N and
    // emit a summary `EffectMsg::LeadershipAction`.
    //
    // Stacking semantics: multiple captains pool boosts via the underlying
    // HashSet — duplicate (same target from two captains) collapses to one
    // +1. Multi-captain stacking is intentionally not supported.
    let mut boost_map: BoostMap = BoostMap::default();
    let mut leadership_effects: Vec<EffectMsg> = Vec::new();
    {
      // Snapshot of the action queue to use for "is this target action still
      // live?" checks. Cloning is cheap relative to a turn cycle.
      let queue_snapshot = entities.actions.clone();
      for (ship_name, ship_actions) in &entities.actions {
        // Defensive: skip leadership checks for ships that don't exist.
        let Some(ship_lock) = entities.ships.get(ship_name) else {
          continue;
        };
        for action in ship_actions {
          let ShipAction::LeadershipCheck { boosts } = action else {
            continue;
          };
          // Pull the pre-rolled leadership effect off the captain's ship. If
          // the captain never hit the button this turn, n is 0 and no boosts
          // apply.
          let n = ship_lock.read().unwrap().get_leadership_points();
          let take = if n > 0 { usize::try_from(n).unwrap_or(0) } else { 0 };

          // Deterministic order: by ship asc, kind ordinal, then weapon_id.
          let mut sorted: Vec<BoostTarget> = boosts.clone();
          sorted.sort_by_key(boost_target_sort_key);
          // Drop boosts whose target action no longer exists in the queue.
          sorted.retain(|t| boost_target_alive(t, &queue_snapshot, &entities.ships));

          let truncated: Vec<BoostTarget> = sorted.into_iter().take(take).collect();
          for t in &truncated {
            boost_map.insert(t.clone());
          }

          leadership_effects.push(EffectMsg::LeadershipAction {
            ship_name: ship_name.clone(),
            points: n,
            boosts_applied: truncated,
          });
        }
      }
    }

    let actions = &entities.actions;
    debug!("(/update) Ship actions: {:?}", actions);

    // Sort all the actions by type.  Slice into fire / sensor / jam-missile /
    // point-defense / engineer (Jump is an engineer action, so it lands in
    // the engineer slice).
    //
    // `sensor_actions` covers SensorLock / BreakSensorLock / JamComms — those
    // are inputs to `fire_actions` (lock state feeds to-hit modifiers; comms
    // jam feeds leadership) so they must run pre-fire as today.
    //
    // `jam_missile_actions` is split out because `JamMissiles` operates
    // directly on the live missile pool. It must run AFTER `fire_actions`
    // so it can target missiles launched this same round (close-range
    // engagements where the missile would otherwise impact in the same
    // round it was launched, with no defensive opportunity).
    #[allow(clippy::type_complexity)]
    let (fire_actions, sensor_actions, jam_missile_actions, point_defense_actions, engineer_actions): (
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
      Vec<(String, Vec<ShipAction>)>,
    ) = multiunzip(actions.iter().filter_map(|(ship_name, actions)| {
      if !entities.ships.contains_key(ship_name) {
        warn!("(update) Cannot find ship {} for actions.", ship_name);
        return None;
      }
      let (f_actions, s_actions, j_actions, p_actions, e_actions): (
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
        Vec<Option<ShipAction>>,
      ) = multiunzip(actions.iter().map(|action| match action {
        ShipAction::FireAction { .. } | ShipAction::DeleteFireAction { .. } => {
          (Some(action.clone()), None, None, None, None)
        }
        ShipAction::PointDefenseAction { .. } => (None, None, None, Some(action.clone()), None),
        ShipAction::JamMissiles => (None, None, Some(action.clone()), None, None),
        ShipAction::BreakSensorLock { .. } | ShipAction::SensorLock { .. } | ShipAction::JamComms { .. } => {
          (None, Some(action.clone()), None, None, None)
        }
        // Engineer actions (including Jump) are deferred to end-of-turn evaluation.
        ShipAction::OverloadDrive | ShipAction::OverloadPlant | ShipAction::Repair { .. } | ShipAction::Jump => {
          (None, None, None, None, Some(action.clone()))
        }
        // LeadershipCheck is consumed in Phase 0 below; it does not flow into
        // any of the per-category slices.
        ShipAction::LeadershipCheck { .. } => (None, None, None, None, None),
        // Anti-actions are consumed by `merge` and should never reach the queue.
        // If one slips through, drop it from every slice.
        ShipAction::ClearSensorAction | ShipAction::ClearEngineerAction | ShipAction::ClearLeadershipCheck => {
          (None, None, None, None, None)
        }
      }));
      Some((
        (ship_name.clone(), f_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), s_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), j_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), p_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
        (ship_name.clone(), e_actions.into_iter().flatten().collect::<Vec<ShipAction>>()),
      ))
    }));

    // Take a snapshot of all the ships.  We'll use this for attackers while
    // damage goes directly onto the "official" ships.  But it means if they are damaged
    // or destroyed they still get to take their actions.
    let ship_snapshot: HashMap<String, Ship> = entities.ship_deep_copy();

    // First emit the leadership-action effects so the FE knows the +1
    // assignments before any sensor/fire results land.
    let mut effects = leadership_effects;

    // First process the targeting/comms sensor actions (SensorLock,
    // BreakSensorLock, JamComms). These feed into fire to-hit modifiers and
    // leadership bonuses, so they must run pre-fire.
    effects.append(&mut entities.sensor_actions(&sensor_actions, &boost_map, &mut rng));

    // 1. This method will make a clone of all ships to use as attacker while impacting damage on the primary copy of ships.  This way ships still get ot attack
    // even when damaged.  This gives us a "simultaneous" attack semantics.
    // 2. Add all new missiles into the entities structure.
    // 3. Then update all the entities.  Note this means ship movement is after combat so a ship with degraded maneuver might not move as much as expected.
    // Its not clear to me if this is the right order - or should they move then take damage - but we'll do it this way for now.
    // 3. Return a set of effects
    effects.append(&mut entities.fire_actions(
      &fire_actions,
      &point_defense_actions,
      &ship_snapshot,
      &boost_map,
      &mut rng,
    ));

    // Now process JamMissiles. This runs AFTER fire_actions so the jam can
    // target missiles launched in this same round (close-range engagements
    // where the missile would otherwise impact in update_all without ever
    // having been visible to a pre-fire jam check). Long-range missiles still
    // get jam checks on subsequent rounds because they sit in self.missiles
    // until they reach their target.
    effects.append(&mut entities.sensor_actions(&jam_missile_actions, &boost_map, &mut rng));

    // 4. Update all entities (ships, planets, missiles) and gather in their effects.
    effects.append(&mut entities.update_all(&ship_snapshot, &boost_map, &mut rng));

    // Jumps are now resolved as part of `engineer_actions` below (Jump is an
    // engineer-class action) — no separate phase here.

    // Decided we don't want to reset this - default should be to keep the same actions.
    // 6. Reset all ship agility setting as the round is over.
    /* for ship in entities.ships.values() {
      ship.write().unwrap().reset_pilot_actions();
    }
    */

    // Reset temporary bonuses (from engineer overload actions) at end of turn.
    // Must run BEFORE engineer_actions so the new bonuses applied this turn
    // survive into the next turn.
    for ship in entities.ships.values() {
      ship.write().unwrap().reset_temporary_bonuses();
    }

    // Evaluate queued engineer actions at end-of-turn. Effects ride the
    // existing Effects channel.
    effects.append(&mut entities.engineer_actions(&engineer_actions, &boost_map, &mut rng));

    // Detection runs last, after movement and after any ship has jumped out.
    // The trigger for losing a stealthed ship is the range opening, which needs
    // both the start-of-round positions in `ship_snapshot` and the end-of-round
    // ones; running here also means a player sees a new contact before queueing
    // the orders that would use it.
    let fired: HashSet<String> = fire_actions
      .iter()
      .filter(|(_, actions)| actions.iter().any(|a| matches!(a, ShipAction::FireAction { .. })))
      .map(|(ship_name, _)| ship_name.clone())
      .collect();
    effects.append(&mut entities.detection_pass(&ship_snapshot, &fired, &mut rng));

    // Hand-offs run immediately after, so a contact acquired this round is
    // shared this round. Automatic in RAW — no check, no action, only Bandwidth.
    effects.append(&mut entities.sensor_handoff_pass());

    // Jamming lasts the round it was made in.
    entities.clear_comms_jamming();

    entities.reset_actions();

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

    let mut server = PlayerManager::new(None, authenticator, false);

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

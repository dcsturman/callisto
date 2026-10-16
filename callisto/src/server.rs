//! This module contains the server state and related types.
//! `Server` is the state of all running scenarios (servers), including all entities and their intial state
//! (for reverting).
//! `ServerMembersTable` holds membership indexed by the same unique id as used in `Server`, and stores
//! the details for each current player in that server.
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use crate::entity::Entities;
use crate::payloads::{email_to_display_name, Role, UserData};
use crate::ship::{get_ship_templates_snapshot, Controller, ShipDesignTemplate};
use crate::{error, warn, LOG_SCENARIO_ACTIVITY};
use tracing::{event, Level};

// Time in seconds for an unused scenario to exist before it is removed.
const SCENARIO_EXPIRATION_TIME: u64 = 300;

/// Represents a distinct server created for a running scenario.
/// It holds a unique ID for the server (generated randomly)
/// as well as the state of the server - the entities - as the
/// initial state of the server - a static version of entities on creation.
pub struct Server {
  // Unique random ID for this server
  pub id: String,
  pub entities: Mutex<Entities>,
  pub initial_scenario: Entities,
  ship_templates: Arc<HashMap<String, Arc<ShipDesignTemplate>>>,
  /// This game's record, when games are being recorded.
  log: std::sync::OnceLock<crate::game_log::GameLog>,
  /// Whether AI crews can be used in this game. Chosen when it is created,
  /// by a user the users file allows; see `with_ai`.
  ai_enabled: bool,
}

/// Maps a server ID to a server table that contains
/// the membership table for a given server.
pub struct ServerMembersTable {
  server_members: HashMap<String, MembershipTable>,
  scenario_definition: HashMap<String, String>,
}

/// Represents the membership table for a given server, mapping
/// current players (by session key) to their email, session key, role, and ship.
struct MembershipTable {
  /// Map of session key (unique player ID) to player information.
  table: HashMap<String, MemberEntry>,
  /// Unix timestamp of the last exit from this server.  
  last_exit: u64,
}

/// Whether this player is refereeing: holding no ship of their own and not
/// merely watching. Matches `isReferee` on the client.
///
/// The ship decides, not the role. A referee may take a single station to cut
/// their screen down and is still the referee.
fn is_referee(entry: &MemberEntry) -> bool {
  !entry.holds_ships() && !entry.roles.contains(&Role::Observer)
}

/// Represents a player's entry in the server membership table.
/// Note two players could have the same email (same account) but
/// would then have different session keys.
struct MemberEntry {
  email: String,
  roles: Vec<Role>,
  ship: Option<String>,
  /// Further ships this player answers for. Their one ready flag covers all
  /// of them.
  ships: Vec<String>,
  /// Set by the player, cleared for everyone when the round advances.
  ready: bool,
}

impl MemberEntry {
  fn holds_ships(&self) -> bool {
    self.ship.is_some() || !self.ships.is_empty()
  }
}

impl PartialEq for Server {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id
  }
}

/// Represents a distinct server created for a scenario.
/// It holds a unique ID for the server (generated randomly)
/// as well as the state of the server - the entities - as the
/// initial state of the server - a static version of entities on creation.
impl Server {
  /// Create a new server with a id (usually random but created at the client) and a scenario name.
  ///
  /// # Panics
  /// Panics if the scenario file cannot be loaded (doesn't exist, etc.).
  #[must_use]
  pub async fn new(id: &str, scenario_name: &str) -> Self {
    let ship_templates = get_ship_templates_snapshot();
    let initial_scenario = if scenario_name.is_empty() {
      Entities::new()
    } else {
      Entities::load_from_file_with_ship_templates(scenario_name, ship_templates.clone())
        .await
        .unwrap_or_else(|e| {
          warn!("Issue loading scenario file {scenario_name}: {e}");
          Entities::new()
        })
    };

    // `deep_copy` returns an error on dangling references inside the
    // scenario file (missile target / planet primary). At Server::new
    // time this is a hard-failure: the file is malformed and the scenario
    // wouldn't function. Panic with the error so the bad file is obvious
    // — the higher-level scenario load already swallows broken files
    // and logs them; this expect path is only reachable in tests with
    // hand-built `Entities`.
    let live_copy = initial_scenario
      .deep_copy()
      .expect("Server::new: initial scenario has dangling references; check the scenario file");
    Server {
      id: id.to_string(),
      entities: Mutex::new(live_copy),
      initial_scenario,
      ship_templates,
      log: std::sync::OnceLock::new(),
      ai_enabled: false,
    }
  }

  /// Make AI crews usable in this game, or not. Without them, a ship the
  /// scenario file hands to the AI is crewed by people instead -- in the game
  /// and in the copy a reset goes back to -- so no AI is ever asked to play.
  #[must_use]
  pub fn with_ai(mut self, enabled: bool) -> Self {
    self.ai_enabled = enabled;
    if !enabled {
      let to_people = |entities: &Entities| {
        for ship in entities.ships.values() {
          let mut ship = ship.write().unwrap_or_else(std::sync::PoisonError::into_inner);
          ship.controller = Controller::Human;
          ship.ai_report = None;
        }
      };
      to_people(&self.initial_scenario);
      to_people(self.entities.get_mut().unwrap_or_else(std::sync::PoisonError::into_inner));
    }
    self
  }

  /// Whether AI crews can be used in this game.
  #[must_use]
  pub fn ai_enabled(&self) -> bool {
    self.ai_enabled
  }

  /// Start recording this game in `dir`, from the world as it is now. Does
  /// nothing if it is already being recorded or the record cannot be opened.
  pub fn start_log(&self, dir: &str, scenario: &str) {
    if self.log.get().is_some() {
      return;
    }
    let Ok(world) = self.entities.lock() else {
      return;
    };
    match crate::game_log::GameLog::start(dir, &self.id, scenario, &world) {
      Ok(log) => {
        let _ = self.log.set(log);
      }
      Err(e) => tracing::warn!("(Server.start_log) Not recording {}: {e}", self.id),
    }
  }

  /// This game's record, if it is being recorded.
  #[must_use]
  pub fn log(&self) -> Option<&crate::game_log::GameLog> {
    self.log.get()
  }

  /// Get the ID of the server.
  #[must_use]
  pub fn get_id(&self) -> &str {
    self.id.as_str()
  }

  /// Reset the server to its initial state.
  ///
  /// # Panics
  /// Panics if the lock on entities cannot be obtained.
  pub fn reset(&self) {
    *self.entities.lock().unwrap() = self.initial_scenario.clone();
  }

  /// Get the entities of the server, unlocked.  This is a convenience routine that
  /// allows the caller to avoid having to deal with the lock.
  ///
  /// # Errors
  /// Returns an error if the lock on entities cannot be obtained.
  pub fn get_unlocked_entities(
    &self,
  ) -> Result<MutexGuard<'_, Entities>, std::sync::PoisonError<MutexGuard<'_, Entities>>> {
    self.entities.lock()
  }

  /// Look up a ship design by name.
  ///
  /// Checks the per-Server snapshot first (so designs known at scenario
  /// load time keep working even if a watcher reload mutates the global
  /// registry), then falls back to the live global registry. The fallback
  /// is what lets the user place a ship from a design that was uploaded
  /// AFTER the scenario was created — without it, `add_ship` would fail
  /// with "Could not find design X" for any post-creation upload, even
  /// though X appears in the live dropdown.
  #[must_use]
  pub fn get_ship_template(&self, design_name: &str) -> Option<Arc<ShipDesignTemplate>> {
    self
      .ship_templates
      .get(design_name)
      .cloned()
      .or_else(|| get_ship_templates_snapshot().get(design_name).cloned())
  }
}

impl ServerMembersTable {
  #[must_use]
  pub fn new() -> Self {
    ServerMembersTable {
      server_members: HashMap::new(),
      scenario_definition: HashMap::new(),
    }
  }

  pub fn register(&mut self, scenario_name: &str, template_name: &str) {
    self
      .scenario_definition
      .insert(scenario_name.to_string(), template_name.to_string());
  }

  pub fn update(&mut self, server_id: &str, session_key: &str, email: &str, roles: Vec<Role>, ship: Option<String>) {
    if !self.scenario_definition.contains_key(server_id) {
      error!("Server {server_id} is not registered with a scenario description.");
      return;
    }

    let server_table = self.server_members.entry(server_id.to_string()).or_default();
    // Changing station is not un-readying: a player who has said they are done
    // and then swaps seats is still done.
    let existing = server_table.table.get(session_key);
    let ready = existing.is_some_and(|entry| entry.ready);
    // Nor is it giving up the other ships you answer for; `set_ships` does that.
    let ships = existing.map(|entry| entry.ships.clone()).unwrap_or_default();
    server_table.table.insert(
      session_key.to_string(),
      MemberEntry {
        email: email.to_string(),
        roles,
        ship,
        ships,
        ready,
      },
    );
  }

  /// Set the further ships a player answers for, beyond their own console's.
  pub fn set_ships(&mut self, server_id: &str, session_key: &str, ships: Vec<String>) {
    if let Some(entry) = self
      .server_members
      .get_mut(server_id)
      .and_then(|members| members.table.get_mut(session_key))
    {
      entry.ships = ships;
    }
  }

  /// Look for a user with a given session key already on this server.  If so, get the id as well as other existing
  /// player information so that we don't recreate a shadow user on a second login by the same user.
  ///
  /// # Returns
  /// Returns a tuple of the server id, the email, the role, and the ship.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  #[must_use]
  pub fn find_scenario_info_by_session_key(&self, key: &str) -> Option<(String, String, Vec<Role>, Option<String>)> {
    self.server_members.iter().find_map(|(server_id, members_table)| {
      members_table
        .table
        .iter()
        .find(|(session_key, _entry)| session_key.as_str() == key)
        .map(|(_session_key, entry)| (server_id.clone(), entry.email.clone(), entry.roles.clone(), entry.ship.clone()))
    })
  }

  /// Remove a given user from a given server.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  /// Also panics if for some reason current system clock is before the unix epoch.
  pub fn remove(&mut self, server_id: &str, session_key: &str) {
    let result = self.server_members.get_mut(server_id).unwrap().table.remove(session_key);
    if result.is_some() {
      // Set the last exit time to the current time.
      self.server_members.get_mut(server_id).unwrap().last_exit =
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    }
  }

  /// Builds the user context for a given server.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  #[must_use]
  pub fn get_user_context(&self, server_id: &str) -> Vec<UserData> {
    self
      .server_members
      .get(server_id)
      .unwrap()
      .table
      .values()
      .map(|entry| UserData {
        display_name: email_to_display_name(&entry.email),
        roles: entry.roles.clone(),
        ship: entry.ship.clone(),
        ships: entry.ships.clone(),
        ready: entry.ready,
      })
      .collect()
  }

  /// Mark one player ready, or not. Returns false if they are not in the
  /// server, which means the caller has nothing to tell anyone about.
  pub fn set_ready(&mut self, server_id: &str, session_key: &str, ready: bool) -> bool {
    self
      .server_members
      .get_mut(server_id)
      .and_then(|members| members.table.get_mut(session_key))
      .is_some_and(|entry| {
        entry.ready = ready;
        true
      })
  }

  /// Clear everyone's ready flag, for the start of a new round.
  pub fn clear_ready(&mut self, server_id: &str) {
    if let Some(members) = self.server_members.get_mut(server_id) {
      for entry in members.table.values_mut() {
        entry.ready = false;
      }
    }
  }

  /// Whether the round should advance on its own: nobody is refereeing, at
  /// least one player holds a ship, and every player holding a ship has said
  /// Ready.
  ///
  /// Observers are not asked -- they have no orders to give. With a referee
  /// this is always false: the referee presses Next Round, as before.
  #[must_use]
  pub fn everyone_ready(&self, server_id: &str) -> bool {
    self.server_members.get(server_id).is_some_and(|members| {
      let mut crews = members.table.values().filter(|entry| entry.holds_ships()).peekable();
      !members.table.values().any(is_referee) && crews.peek().is_some() && crews.all(|entry| entry.ready)
    })
  }

  /// Whether anyone in this server is refereeing.
  #[must_use]
  pub fn has_referee(&self, server_id: &str) -> bool {
    self
      .server_members
      .get(server_id)
      .is_some_and(|members| members.table.values().any(is_referee))
  }

  #[must_use]
  pub fn current_scenario_list(&self) -> Vec<(String, String)> {
    self
      .server_members
      .keys()
      .filter_map(|key| {
        self
          .scenario_definition
          .get(key)
          .map(|scenario_name| (key.clone(), scenario_name.clone()))
      })
      .collect()
  }

  /// Find and remove any scenarios that have been empty for more than 5 minutes.
  ///
  /// # Returns
  /// Returns true if any scenarios were removed.
  ///
  /// # Panics
  /// Panics if the current system clock is before the unix epoch.
  pub fn clean_expired_scenarios(&mut self) -> bool {
    let now = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs();
    let initial_size = self.server_members.len();
    self.server_members.retain(|scenario_name, server_table| {
      // Need to log the event when deleting the scenario, thus the use of a somewhat empty if statement.
      if server_table.table.is_empty() && now - server_table.last_exit > SCENARIO_EXPIRATION_TIME {
        event!(
          target: LOG_SCENARIO_ACTIVITY,
          Level::INFO,
          scenario = scenario_name,
          action = "expire"
        );
        false
      } else {
        true
      }
    });

    initial_size != self.server_members.len()
  }
}

impl Default for MembershipTable {
  fn default() -> Self {
    MembershipTable {
      table: HashMap::new(),
      last_exit: u64::MAX,
    }
  }
}

impl Default for ServerMembersTable {
  fn default() -> Self {
    Self::new()
  }
}

#[cfg(test)]
mod ai_tests {
  use super::Server;
  use crate::entity::{Entities, Vec3};
  use crate::ship::{config_test_ship_templates, get_ship_template, Controller};
  use cgmath::Zero;

  fn with_an_ai_ship(entities: &mut Entities) {
    let design = get_ship_template("Buccaneer").unwrap();
    entities.add_ship("Pirate".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    entities.ships["Pirate"].write().unwrap().controller = Controller::Ai;
  }

  /// A game started without AI crews hands any ship its scenario gave the AI
  /// to people instead, in play and in what a reset goes back to.
  #[tokio::test]
  async fn a_game_without_ai_crews_has_no_ai_ships() {
    config_test_ship_templates().await;
    let mut server = Server::new("game", "").await;
    with_an_ai_ship(&mut server.initial_scenario);
    with_an_ai_ship(server.entities.get_mut().unwrap());
    let server = server.with_ai(false);
    assert!(!server.ai_enabled());
    assert_eq!(
      server.initial_scenario.ships["Pirate"].read().unwrap().controller,
      Controller::Human
    );
    assert_eq!(
      server.get_unlocked_entities().unwrap().ships["Pirate"]
        .read()
        .unwrap()
        .controller,
      Controller::Human
    );
  }

  #[tokio::test]
  async fn a_game_with_ai_crews_keeps_them() {
    config_test_ship_templates().await;
    let mut server = Server::new("game", "").await;
    with_an_ai_ship(server.entities.get_mut().unwrap());
    let server = server.with_ai(true);
    assert!(server.ai_enabled());
    assert_eq!(
      server.get_unlocked_entities().unwrap().ships["Pirate"]
        .read()
        .unwrap()
        .controller,
      Controller::Ai
    );
  }
}

#[cfg(test)]
mod membership_tests {
  use super::{Role, ServerMembersTable};

  fn table_with_two() -> ServerMembersTable {
    let mut members = ServerMembersTable::new();
    members.register("scenario-1", "Treasure 1");
    members.update("scenario-1", "key-gm", "gm@example.com", vec![Role::General], None);
    members.update(
      "scenario-1",
      "key-pilot",
      "pilot@example.com",
      vec![Role::Pilot],
      Some("Executor".to_string()),
    );
    members
  }

  #[test]
  fn ready_is_per_player_and_cleared_for_the_round() {
    let mut members = table_with_two();
    assert!(members.set_ready("scenario-1", "key-pilot", true));
    let readies: Vec<bool> = members.get_user_context("scenario-1").iter().map(|user| user.ready).collect();
    assert_eq!(readies.iter().filter(|ready| **ready).count(), 1);

    members.clear_ready("scenario-1");
    assert!(members.get_user_context("scenario-1").iter().all(|user| !user.ready));
  }

  /// Changing station mid-round does not undo saying you are done.
  #[test]
  fn ready_survives_a_role_change() {
    let mut members = table_with_two();
    members.set_ready("scenario-1", "key-pilot", true);
    members.update(
      "scenario-1",
      "key-pilot",
      "pilot@example.com",
      vec![Role::Gunner],
      Some("Executor".to_string()),
    );
    assert!(members
      .get_user_context("scenario-1")
      .iter()
      .any(|user| user.ready && user.ship == Some("Executor".to_string())));
  }

  /// A player with no ship and every station is the referee; a crew is not.
  #[test]
  fn a_referee_is_someone_without_a_ship() {
    let members = table_with_two();
    assert!(members.has_referee("scenario-1"));

    let mut crew_only = ServerMembersTable::new();
    crew_only.register("scenario-2", "Treasure 1");
    crew_only.update(
      "scenario-2",
      "key-pilot",
      "pilot@example.com",
      vec![Role::General],
      Some("Executor".to_string()),
    );
    assert!(!crew_only.has_referee("scenario-2"));
  }

  /// With no referee, the round advances once every crew is ready. Observers
  /// are not waited for; a table with nobody holding a ship never advances
  /// itself; and with a referee present it never does either.
  #[test]
  fn everyone_ready_waits_for_every_crew_and_only_without_a_referee() {
    let mut members = ServerMembersTable::new();
    members.register("s", "Thebus");
    members.update("s", "key-a", "a@example.com", vec![Role::General], Some("Executor".to_string()));
    members.update(
      "s",
      "key-b",
      "b@example.com",
      vec![Role::General],
      Some("Invigilator".to_string()),
    );
    members.update("s", "key-watch", "w@example.com", vec![Role::Observer], None);
    assert!(!members.everyone_ready("s"));

    members.set_ready("s", "key-a", true);
    assert!(!members.everyone_ready("s"), "one crew still giving orders");
    members.set_ready("s", "key-b", true);
    assert!(members.everyone_ready("s"), "the observer is not waited for");

    members.update("s", "key-gm", "gm@example.com", vec![Role::General], None);
    assert!(!members.everyone_ready("s"), "a referee presses Next Round themselves");

    let mut watchers = ServerMembersTable::new();
    watchers.register("w", "Thebus");
    watchers.update("w", "key-watch", "w@example.com", vec![Role::Observer], None);
    assert!(!watchers.everyone_ready("w"), "nobody holds a ship, so nobody is ready");
  }

  /// A connection answering for a whole side -- the AI service, or a human
  /// commanding a team -- is crew, not referee, even holding no console of
  /// its own. Its one ready flag is the whole side's, and a later change of
  /// station does not drop the ships it answers for.
  #[test]
  fn a_connection_can_answer_for_several_ships() {
    let mut members = ServerMembersTable::new();
    members.register("s", "Thebus");
    members.update("s", "key-ai", "ai@example.com", vec![Role::General], None);
    assert!(members.has_referee("s"), "holding nothing yet, it looks like a referee");

    members.set_ships("s", "key-ai", vec!["Invigilator".to_string(), "Cicada".to_string()]);
    assert!(!members.has_referee("s"));
    members.update("s", "key-ai", "ai@example.com", vec![Role::Gunner], None);
    let users = members.get_user_context("s");
    assert_eq!(users[0].ships, vec!["Invigilator".to_string(), "Cicada".to_string()]);

    members.update(
      "s",
      "key-crew",
      "c@example.com",
      vec![Role::General],
      Some("Executor".to_string()),
    );
    members.set_ready("s", "key-crew", true);
    assert!(!members.everyone_ready("s"), "the pirates have not said ready");
    members.set_ready("s", "key-ai", true);
    assert!(members.everyone_ready("s"));
  }

  #[test]
  fn setting_ready_for_a_stranger_says_so() {
    let mut members = table_with_two();
    assert!(!members.set_ready("scenario-1", "key-nobody", true));
  }
}

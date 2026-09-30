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
use crate::ship::{get_ship_templates_snapshot, ShipDesignTemplate};
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

/// Whether this player is refereeing: no ship of their own, and every station,
/// which is what the client calls GM. Matches `isReferee` on the client.
fn is_referee(entry: &MemberEntry) -> bool {
  entry.ship.is_none() && entry.roles.contains(&Role::General)
}

/// Represents a player's entry in the server membership table.
/// Note two players could have the same email (same account) but
/// would then have different session keys.
struct MemberEntry {
  email: String,
  roles: Vec<Role>,
  ship: Option<String>,
  /// Set by the player, cleared for everyone when the round advances.
  ready: bool,
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
    }
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
    let ready = server_table.table.get(session_key).is_some_and(|entry| entry.ready);
    server_table.table.insert(
      session_key.to_string(),
      MemberEntry {
        email: email.to_string(),
        roles,
        ship,
        ready,
      },
    );
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

  #[test]
  fn setting_ready_for_a_stranger_says_so() {
    let mut members = table_with_two();
    assert!(!members.set_ready("scenario-1", "key-nobody", true));
  }
}

//! This module contains the server state and related types.
//! `Server` is the state of all running scenarios (servers), including all entities and their intial state
//! (for reverting).
//! `ServerMembersTable` holds membership indexed by the same unique id as used in `Server`, and stores
//! the details for each current player in that server.
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use crate::entity::Entities;
use crate::payloads::{Role, UserData};
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

/// Represents a player's entry in the server membership table.
/// Note two players could have the same email (same account) but
/// would then have different session keys.
struct MemberEntry {
  email: String,
  role: Role,
  ship: Option<String>,
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
    let initial_scenario = if scenario_name.is_empty() {
      Entities::new()
    } else {
      Entities::load_from_file(scenario_name).await.unwrap_or_else(|e| {
        warn!("Issue loading scenario file {scenario_name}: {e}");
        Entities::new()
      })
    };

    Server {
      id: id.to_string(),
      entities: Mutex::new(initial_scenario.deep_copy()),
      initial_scenario,
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

  pub fn update(&mut self, server_id: &str, session_key: &str, email: &str, role: Role, ship: Option<String>) {
    if !self.scenario_definition.contains_key(server_id) {
      error!("Server {server_id} is not registered with a scenario description.");
      return;
    }

    let server_table = self.server_members.entry(server_id.to_string()).or_default();
    server_table.table.insert(
      session_key.to_string(),
      MemberEntry {
        email: email.to_string(),
        role,
        ship,
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
  pub fn find_scenario_info_by_session_key(&self, key: &str) -> Option<(String, String, Role, Option<String>)> {
    self.server_members.iter().find_map(|(server_id, members_table)| {
      members_table
        .table
        .iter()
        .find(|(session_key, _entry)| session_key.as_str() == key)
        .map(|(_session_key, entry)| (server_id.clone(), entry.email.clone(), entry.role, entry.ship.clone()))
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
        email: entry.email.clone(),
        role: entry.role,
        ship: entry.ship.clone(),
      })
      .collect()
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

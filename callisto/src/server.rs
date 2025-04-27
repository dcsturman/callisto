use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};
use std::time::SystemTime;

use crate::entity::Entities;
use crate::payloads::{Role, UserData};
use crate::warn;
pub struct Server {
  pub id: String,
  pub entities: Mutex<Entities>,
  pub initial_scenario: Entities,
}

pub struct ServerMembersTable {
  members: HashMap<String, ServerTable>,
}

struct ServerTable {
  table: HashMap<u64, ServerMemberEntry>,
  last_exit: u64,
}
struct ServerMemberEntry {
  email: String,
  role: Role,
  ship: Option<String>,
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
      members: HashMap::new(),
    }
  }

  pub fn update(&mut self, server_id: &str, unique_id: u64, email: &str, role: Role, ship: Option<String>) {
    let server_table = self.members.entry(server_id.to_string()).or_default();
    server_table.table.insert(
      unique_id,
      ServerMemberEntry {
        email: email.to_string(),
        role,
        ship,
      },
    );
  }

  /// Look for a user with a given email already on this server.  If so, get the id as well as other existing
  /// player information so that we don't recreate a shadow user on a second login by the same user.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  #[must_use]
  pub fn find_scenario_info_by_email(&self, email: &str) -> Option<(String, u64, Role, Option<String>)> {
    let server_id = self
      .members
      .iter()
      .find(|(_, server_table)| server_table.table.values().any(|entry| entry.email == email))
      .map(|(server_id, _)| server_id)?;

    self
      .members
      .get(server_id)
      .unwrap()
      .table
      .iter()
      .find(|(_, entry)| entry.email == email)
      .map(|(id, entry)| (server_id.clone(), *id, entry.role, entry.ship.clone()))
  }

  /// Remove a given user from a given server.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  /// Also panics if for some reason current system clock is before the unix epoch.
  pub fn remove(&mut self, server_id: &str, unique_id: u64) {
    let result = self.members.get_mut(server_id).unwrap().table.remove(&unique_id);
    if result.is_some() {
      // Set the last exit time to the current time.
      self.members.get_mut(server_id).unwrap().last_exit =
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
      .members
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
  pub fn current_scenario_list(&self) -> Vec<String> {
    self.members.keys().cloned().collect()
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
    let initial_size = self.members.len();
    self
      .members
      .retain(|_, server_table| !(server_table.table.is_empty() && now - server_table.last_exit > 300));

    initial_size != self.members.len()
  }
}

impl Default for ServerTable {
  fn default() -> Self {
    ServerTable {
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

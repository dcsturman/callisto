use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard};

use crate::warn;
use crate::entity::Entities;
use crate::payloads::{Role, UserData};
pub struct Server {
  pub id: String,
  pub entities: Mutex<Entities>,
  pub initial_scenario: Entities,
}

pub struct ServerMembersTable {
  members: HashMap<String, HashMap<u64, ServerMemberEntry>>,
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
      Entities::load_from_file(scenario_name)
        .await
        .unwrap_or_else(|e| { warn!("Issue loading scenario file {scenario_name}: {e}"); Entities::new() })
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
    server_table.insert(
      unique_id,
      ServerMemberEntry {
        email: email.to_string(),
        role,
        ship,
      },
    );
  }

  /// Remove a given user from a given server.
  ///
  /// # Panics
  /// Panics if the server does not exist.
  pub fn remove(&mut self, server_id: &str, unique_id: u64) {
    self.members.get_mut(server_id).unwrap().remove(&unique_id);
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
}

impl Default for ServerMembersTable {
  fn default() -> Self {
    Self::new()
  }
}

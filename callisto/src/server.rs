use crate::entity::Entities;
use std::sync::{Arc, Mutex, MutexGuard};

pub struct Server {
  pub id: String,
  pub entities: Arc<Mutex<Entities>>,
  pub initial_scenario: Entities,
}

/// Represents a distinct server created for a scenario.
/// It holds a unique ID for the server (generated randomly)
/// as well as the state of the server - the entities - as the
/// initial state of the server - a static version of entities on creation.
impl Server {
  #[must_use]
  /// Create a new server based on an initial state (entities).
  pub fn new(id: &str, initial_scenario: Entities) -> Self {
    Server {
      id: id.to_string(),
      entities: Arc::new(Mutex::new(initial_scenario.deep_copy())),
      initial_scenario,
    }
  }

  /// Get the ID of the server.
  #[must_use]
  pub fn get_id(&self) -> String {
    self.id.clone()
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

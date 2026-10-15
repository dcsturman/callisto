//! Every side this service is crewing.
//!
//! The backend states a side's whole ship list in every `/crew` call, so the
//! same call serves to start a crew, to tell a running one its ships changed,
//! and to restart one that has finished. Repeating a call is harmless.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tokio::sync::watch;
use tokio::task::JoinHandle;

use crate::commander::Commander;
use crate::crew::{self, CrewConfig, Ending};

/// A scenario and a side within it.
pub type SideKey = (String, String);

/// Makes the commander for a new crew.
/// Makes the commander for a new crew, given the game and the side it crews.
pub type CommanderFactory = Arc<dyn Fn(&str, &str) -> Box<dyn Commander> + Send + Sync>;

struct Running {
  ships: watch::Sender<Vec<String>>,
  task: JoinHandle<Ending>,
}

/// One side's state, for `/status`.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct CrewStatus {
  pub scenario_id: String,
  pub side: String,
  pub ships: Vec<String>,
  pub running: bool,
}

pub struct Crews {
  config: Arc<CrewConfig>,
  commander: CommanderFactory,
  running: Mutex<BTreeMap<SideKey, Running>>,
}

impl Crews {
  #[must_use]
  pub fn new(config: CrewConfig, commander: CommanderFactory) -> Self {
    Crews {
      config: Arc::new(config),
      commander,
      running: Mutex::new(BTreeMap::new()),
    }
  }

  /// Crew this side with these ships: start it, update it, or restart it.
  ///
  /// # Panics
  /// Panics if the crew table's lock is poisoned.
  pub fn crew(&self, scenario_id: &str, side: &str, ships: Vec<String>) {
    let key = (scenario_id.to_string(), side.to_string());
    let mut running = self.running.lock().unwrap();
    if let Some(crew) = running.get(&key).filter(|crew| !crew.task.is_finished()) {
      crew.ships.send_replace(ships);
      return;
    }
    let (sender, receiver) = watch::channel(ships);
    let task = tokio::spawn(crew::run(
      self.config.clone(),
      scenario_id.to_string(),
      side.to_string(),
      receiver,
      (self.commander)(scenario_id, side),
    ));
    running.insert(key, Running { ships: sender, task });
  }

  /// Stop crewing this side. Releasing a side we are not crewing is not an
  /// error: the backend may be repeating itself.
  ///
  /// # Panics
  /// Panics if the crew table's lock is poisoned.
  pub fn release(&self, scenario_id: &str, side: &str) {
    // Dropping the sender is the signal: the crew closes its connection.
    self
      .running
      .lock()
      .unwrap()
      .remove(&(scenario_id.to_string(), side.to_string()));
  }

  /// What this service is crewing.
  ///
  /// # Panics
  /// Panics if the crew table's lock is poisoned.
  #[must_use]
  pub fn status(&self) -> Vec<CrewStatus> {
    self
      .running
      .lock()
      .unwrap()
      .iter()
      .map(|((scenario_id, side), crew)| CrewStatus {
        scenario_id: scenario_id.clone(),
        side: side.clone(),
        ships: crew.ships.borrow().clone(),
        running: !crew.task.is_finished(),
      })
      .collect()
  }
}

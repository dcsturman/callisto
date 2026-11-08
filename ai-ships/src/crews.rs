//! Every side this service is crewing.
//!
//! The backend states a side's whole ship list in every message on a side's
//! `/crew` connection, so the same message serves to start a crew, to tell a
//! running one its ships changed, and to restart one that has finished.
//! Repeating one is harmless. The table also counts the connections holding
//! each side, so a side whose connection dropped can be let go once nothing
//! has held it again for a while.

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
  /// Becomes true when the crew finishes.
  done: watch::Receiver<bool>,
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
  /// How many connections hold each side.
  holders: Mutex<BTreeMap<SideKey, usize>>,
}

impl Crews {
  #[must_use]
  pub fn new(config: CrewConfig, commander: CommanderFactory) -> Self {
    Crews {
      config: Arc::new(config),
      commander,
      running: Mutex::new(BTreeMap::new()),
      holders: Mutex::new(BTreeMap::new()),
    }
  }

  /// Crew this side with these ships: start it, update it, or restart it.
  /// What comes back becomes true when the crew finishes.
  ///
  /// # Panics
  /// Panics if the crew table's lock is poisoned.
  pub fn crew(&self, scenario_id: &str, side: &str, ships: Vec<String>) -> watch::Receiver<bool> {
    let key = (scenario_id.to_string(), side.to_string());
    let mut running = self.running.lock().unwrap();
    if let Some(crew) = running.get(&key).filter(|crew| !crew.task.is_finished()) {
      crew.ships.send_replace(ships);
      return crew.done.clone();
    }
    let (sender, receiver) = watch::channel(ships);
    let (finished, done) = watch::channel(false);
    let crew = crew::run(
      self.config.clone(),
      scenario_id.to_string(),
      side.to_string(),
      receiver,
      (self.commander)(scenario_id, side),
    );
    let task = tokio::spawn(async move {
      let ending = crew.await;
      finished.send_replace(true);
      ending
    });
    running.insert(
      key,
      Running {
        ships: sender,
        task,
        done: done.clone(),
      },
    );
    done
  }

  /// A connection now holds this side.
  ///
  /// # Panics
  /// Panics if the holders' lock is poisoned.
  pub fn hold(&self, scenario_id: &str, side: &str) {
    *self
      .holders
      .lock()
      .unwrap()
      .entry((scenario_id.to_string(), side.to_string()))
      .or_default() += 1;
  }

  /// A connection no longer holds this side. Whether any still do.
  ///
  /// # Panics
  /// Panics if the holders' lock is poisoned.
  pub fn unhold(&self, scenario_id: &str, side: &str) -> bool {
    let key = (scenario_id.to_string(), side.to_string());
    let mut holders = self.holders.lock().unwrap();
    let count = holders.get_mut(&key).map_or(0, |count| {
      *count = count.saturating_sub(1);
      *count
    });
    if count == 0 {
      holders.remove(&key);
    }
    count > 0
  }

  /// Whether any connection holds this side.
  ///
  /// # Panics
  /// Panics if the holders' lock is poisoned.
  #[must_use]
  pub fn held(&self, scenario_id: &str, side: &str) -> bool {
    self
      .holders
      .lock()
      .unwrap()
      .contains_key(&(scenario_id.to_string(), side.to_string()))
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

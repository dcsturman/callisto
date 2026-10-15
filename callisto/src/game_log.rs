//! A record of every game played, for learning from later.
//!
//! One JSON-lines file per game, written as it is played: the world when it
//! starts, every order given to a ship and who gave it (a human crew, the AI,
//! or the GM), and at the end of each round what happened and the world after.
//! That is everything needed to replay a game, and to rebuild any side's view
//! of any round with the same function the AI uses, so views are not stored.
//!
//! Nobody is named. An order says which ship it was for and whether a human,
//! the AI or the GM gave it, nothing more: the point is to learn how ships
//! were fought, not who fought them.
//!
//! Turned on by a directory (`--game-log-dir`, or `CALLISTO_GAME_LOG_DIR`);
//! without one nothing is written. A failure to write is logged and the game
//! carries on: the record is never worth a game.

use std::fs::{create_dir_all, File};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use serde_json::Value;
use tracing::{info, warn};

use crate::entity::Entities;
use crate::payloads::{EffectMsg, RequestMsg};

/// The record format. Bumped when a record changes shape.
pub const FORMAT_VERSION: u32 = 1;

/// Who gave an order.
#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
  /// A person at the ship's consoles.
  Human,
  /// The AI service crewing the ship.
  Ai,
  /// The referee, who answers for every ship.
  Gm,
}

#[derive(Serialize)]
#[serde(tag = "event", rename_all = "snake_case")]
enum Record<'a> {
  /// The game as it opened.
  Start {
    version: u32,
    game: &'a str,
    scenario: &'a str,
    started_at: u64,
    world: &'a Entities,
  },
  /// An order given to one or more ships during a round.
  Order {
    round: u32,
    ships: &'a [String],
    by: By,
    /// Whether the server took it: a refused order changed nothing.
    accepted: bool,
    request: &'a Value,
  },
  /// A round resolved: what happened, and the world after it.
  Round {
    round: u32,
    results: &'a [EffectMsg],
    world: &'a Entities,
  },
  /// The GM put the scenario back to its start.
  Reset { world: &'a Entities },
}

/// One game's record, open for writing.
pub struct GameLog {
  out: Mutex<BufWriter<File>>,
  path: String,
}

impl std::fmt::Debug for GameLog {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "GameLog({})", self.path)
  }
}

fn now() -> u64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |elapsed| elapsed.as_secs())
}

/// A file name from a game's name: letters, digits, `-` and `_` only.
fn safe(name: &str) -> String {
  name
    .chars()
    .map(|c| {
      if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
        c
      } else {
        '_'
      }
    })
    .collect()
}

impl GameLog {
  /// Open a new record for `game`, a running scenario, in `dir`, and write its
  /// opening world.
  ///
  /// # Errors
  /// Returns an error if the directory or file cannot be created.
  pub fn start(dir: &str, game: &str, scenario: &str, world: &Entities) -> std::io::Result<Self> {
    create_dir_all(dir)?;
    let started_at = now();
    let path = Path::new(dir).join(format!("{started_at}-{}.jsonl", safe(game)));
    let log = GameLog {
      out: Mutex::new(BufWriter::new(File::create(&path)?)),
      path: path.display().to_string(),
    };
    log.write(&Record::Start {
      version: FORMAT_VERSION,
      game,
      scenario,
      started_at,
      world,
    });
    info!("(GameLog) Recording {game} to {}.", log.path);
    Ok(log)
  }

  /// Where the record is being written.
  #[must_use]
  pub fn path(&self) -> &str {
    &self.path
  }

  fn write(&self, record: &Record<'_>) {
    let Ok(mut out) = self.out.lock() else {
      return;
    };
    let written = serde_json::to_writer(&mut *out, record)
      .map_err(std::io::Error::other)
      .and_then(|()| out.write_all(b"\n"))
      .and_then(|()| out.flush());
    if let Err(e) = written {
      warn!("(GameLog) Could not write to {}: {e}", self.path);
    }
  }

  /// An order given during `round`.
  pub fn order(&self, round: u32, ships: &[String], by: By, accepted: bool, request: &Value) {
    self.write(&Record::Order {
      round,
      ships,
      by,
      accepted,
      request,
    });
  }

  /// A round resolved.
  pub fn round(&self, round: u32, results: &[EffectMsg], world: &Entities) {
    self.write(&Record::Round { round, results, world });
  }

  /// The scenario was reset to its start.
  pub fn reset(&self, world: &Entities) {
    self.write(&Record::Reset { world });
  }
}

/// The ships a request gives orders to, if it is an order: something that
/// changes what a ship does or is. Everything else -- logins, joining,
/// readiness, saving, asking for a course to be worked out -- is not.
#[must_use]
pub fn ships_ordered(request: &RequestMsg) -> Option<Vec<String>> {
  let one = |name: &String| Some(vec![name.clone()]);
  match request {
    RequestMsg::SetPlan(plan) => one(&plan.name),
    RequestMsg::ModifyActions(list) => Some(list.iter().map(|(ship, _)| ship.clone()).collect()),
    RequestMsg::SetPilotActions(pilot) => one(&pilot.ship_name),
    RequestMsg::SetShipEmissions(emissions) => one(&emissions.ship_name),
    RequestMsg::SetSoftwareRunning(software) => one(&software.ship_name),
    RequestMsg::CaptainAction(captain) => one(&captain.ship_name),
    RequestMsg::SetShipController(controller) => one(&controller.ship_name),
    RequestMsg::SetOrders(orders) => one(&orders.ship_name),
    RequestMsg::SetAiReport(report) => one(&report.ship_name),
    RequestMsg::SetShipTeam(team) => one(&team.ship_name),
    RequestMsg::AddShip(ship) => one(&ship.name),
    RequestMsg::Remove(name) => one(name),
    RequestMsg::RenameEntity(rename) => Some(vec![rename.current.clone(), rename.new_name.clone()]),
    _ => None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::payloads::SetOrders;

  #[test]
  fn a_game_is_written_as_it_is_played() {
    let dir = std::env::temp_dir().join(format!("callisto-gamelog-{}", std::process::id()));
    let dir = dir.to_str().unwrap();
    let world = Entities::new();
    let log = GameLog::start(dir, "Thebus 1/2", "Thebus.json", &world).unwrap();
    assert!(log.path().ends_with("-Thebus_1_2.jsonl"), "{}", log.path());

    let request = RequestMsg::SetOrders(SetOrders {
      ship_name: "Invigilator".to_string(),
      orders: Some("Close and board.".to_string()),
    });
    let ships = ships_ordered(&request).unwrap();
    log.order(1, &ships, By::Ai, true, &serde_json::to_value(&request).unwrap());
    log.round(1, &[], &world);

    let lines: Vec<Value> = std::fs::read_to_string(log.path())
      .unwrap()
      .lines()
      .map(|line| serde_json::from_str(line).unwrap())
      .collect();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0]["event"], "start");
    assert_eq!(lines[0]["version"], FORMAT_VERSION);
    assert_eq!(lines[1]["event"], "order");
    assert_eq!(lines[1]["by"], "Ai");
    assert_eq!(lines[1]["ships"][0], "Invigilator");
    assert_eq!(lines[2]["event"], "round");
    let _ = std::fs::remove_dir_all(dir);
  }

  #[test]
  fn only_orders_are_orders() {
    assert!(ships_ordered(&RequestMsg::SetReady(true)).is_none());
    assert!(ships_ordered(&RequestMsg::Ping).is_none());
  }
}

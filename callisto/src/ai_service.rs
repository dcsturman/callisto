//! Telling the AI service which ships to crew.
//!
//! The backend does not decide anything for an AI ship; the AI service does,
//! connecting as a player. What the backend does is notice when ships become
//! AI-controlled -- from a scenario file, or because someone pressed "Switch
//! to AI" -- and ask the service to crew them, and notice when they stop.
//!
//! Ships are crewed by **side**: every AI ship on one team is one side, and
//! the service makes one decision for the side each round. An AI ship on no
//! team is a side of its own, since unaligned ships are not allies.
//!
//! The backend keeps what it last told the service for each scenario and,
//! after every request, works out what it should say now. A side whose ships
//! changed gets `POST /crew` with its full ship list; a side that is gone gets
//! `POST /release`. Because every call states the whole side, a call that was
//! missed or repeated is healed by the next one.
//!
//! Calls run on their own task and never hold up the game. If the service is
//! down the failure is logged and the ships sit there, as a ship with no crew
//! does.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use serde::Serialize;

use crate::entity::{Entities, Entity};
use crate::ship::{Controller, Team};
use crate::{info, warn};

const CALL_TIMEOUT: Duration = Duration::from_secs(10);
const METADATA_IDENTITY_URL: &str =
  "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity";

/// The AI ships of one scenario that share a decision.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AiSide {
  pub team: Option<Team>,
  pub ships: BTreeSet<String>,
}

/// A scenario's sides, keyed by a stable id: the team's name, or `ship:` and
/// the ship's name for an unaligned one.
pub type Sides = BTreeMap<String, AiSide>;

/// Which ships in this world the AI should be crewing, and on which sides.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn sides_of(entities: &Entities) -> Sides {
  let mut sides = Sides::new();
  for ship in entities.ships.values() {
    let ship = ship.read().unwrap();
    if ship.controller != Controller::Ai {
      continue;
    }
    let side = ship
      .team
      .map_or_else(|| format!("ship:{}", ship.get_name()), |team| format!("{team:?}"));
    sides
      .entry(side)
      .or_insert_with(|| AiSide {
        team: ship.team,
        ships: BTreeSet::new(),
      })
      .ships
      .insert(ship.get_name().to_string());
  }
  sides
}

/// One thing to tell the AI service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum AiCall {
  /// Crew these ships, as one side: the whole side, not a change to it.
  Crew {
    scenario_id: String,
    side: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    team: Option<Team>,
    ships: Vec<String>,
  },
  /// Stop crewing this side.
  Release { scenario_id: String, side: String },
}

impl AiCall {
  fn path(&self) -> &'static str {
    match self {
      AiCall::Crew { .. } => "crew",
      AiCall::Release { .. } => "release",
    }
  }
}

/// What to tell the service to get from what it was last told to what is
/// true now.
#[must_use]
pub fn plan_calls(scenario_id: &str, before: &Sides, after: &Sides) -> Vec<AiCall> {
  let crew = after
    .iter()
    .filter(|(side, now)| before.get(*side) != Some(*now))
    .map(|(side, now)| AiCall::Crew {
      scenario_id: scenario_id.to_string(),
      side: side.clone(),
      team: now.team,
      ships: now.ships.iter().cloned().collect(),
    });
  let release = before
    .keys()
    .filter(|side| !after.contains_key(*side))
    .map(|side| AiCall::Release {
      scenario_id: scenario_id.to_string(),
      side: side.clone(),
    });
  crew.chain(release).collect()
}

/// Where the AI service is, and how to prove to it that we are the backend.
#[derive(Clone)]
pub struct AiServiceClient {
  url: String,
  /// The shared secret, for local Compose. Without it we ask the Cloud Run
  /// metadata server for an identity token naming the service as audience.
  shared_secret: Option<String>,
  http: reqwest::Client,
}

impl std::fmt::Debug for AiServiceClient {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("AiServiceClient")
      .field("url", &self.url)
      .field("shared_secret", &self.shared_secret.as_ref().map(|_| "<set>"))
      .finish_non_exhaustive()
  }
}

impl AiServiceClient {
  /// # Panics
  /// Panics if the HTTP client cannot be built, which only a broken TLS setup
  /// at startup can cause.
  #[must_use]
  pub fn new(url: &str, shared_secret: Option<String>) -> Self {
    AiServiceClient {
      url: url.trim_end_matches('/').to_string(),
      shared_secret: shared_secret.filter(|secret| !secret.is_empty()),
      http: reqwest::Client::builder()
        .timeout(CALL_TIMEOUT)
        .build()
        .expect("(AiServiceClient::new) Unable to build an HTTP client"),
    }
  }

  /// Send a call without waiting for it.
  pub fn send(&self, call: AiCall) {
    let client = self.clone();
    tokio::spawn(async move {
      if let Err(e) = client.post(&call).await {
        warn!("(AiServiceClient) Could not reach the AI service for {call:?}: {e}");
      }
    });
  }

  /// Send a call and wait for the answer.
  ///
  /// # Errors
  /// Returns an error if no credential can be had, the service cannot be
  /// reached, or it answers with anything but success.
  pub async fn post(&self, call: &AiCall) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let bearer = match &self.shared_secret {
      Some(secret) => secret.clone(),
      None => self.identity_token().await?,
    };
    let url = format!("{}/{}", self.url, call.path());
    self
      .http
      .post(&url)
      .bearer_auth(bearer)
      .json(call)
      .send()
      .await?
      .error_for_status()?;
    info!("(AiServiceClient) Told the AI service: {call:?}");
    Ok(())
  }

  /// A Google identity token for this backend's service account, naming the
  /// AI service as audience: the ordinary Cloud Run service-to-service
  /// arrangement. Fresh on every call; the metadata server caches.
  async fn identity_token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    Ok(
      self
        .http
        .get(METADATA_IDENTITY_URL)
        .query(&[("audience", self.url.as_str())])
        .header("Metadata-Flavor", "Google")
        .send()
        .await?
        .error_for_status()?
        .text()
        .await?,
    )
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ship::{config_test_ship_templates, get_ship_template, Controller};
  use cgmath::Zero;
  use tokio::io::{AsyncReadExt, AsyncWriteExt};

  fn side(team: Option<Team>, ships: &[&str]) -> AiSide {
    AiSide {
      team,
      ships: ships.iter().map(ToString::to_string).collect(),
    }
  }

  #[tokio::test]
  async fn sides_group_ai_ships_by_team_and_leave_people_alone() {
    config_test_ship_templates().await;
    let design = get_ship_template("Buccaneer").unwrap();
    let mut entities = Entities::new();
    for (name, team, ai) in [
      ("Executor", Some(Team::Blue), false),
      ("Invigilator", Some(Team::Red), true),
      ("Cicada", Some(Team::Red), true),
      ("Drifter", None, true),
    ] {
      entities.add_ship(
        name.to_string(),
        crate::entity::Vec3::zero(),
        crate::entity::Vec3::zero(),
        &design,
        None,
        None,
      );
      let mut ship = entities.ships.get(name).unwrap().write().unwrap();
      ship.team = team;
      if ai {
        ship.controller = Controller::Ai;
      }
    }
    let sides = sides_of(&entities);
    assert_eq!(sides.len(), 2);
    assert_eq!(sides["Red"], side(Some(Team::Red), &["Cicada", "Invigilator"]));
    assert_eq!(sides["ship:Drifter"], side(None, &["Drifter"]));
  }

  #[test]
  fn only_changed_sides_are_called_and_gone_ones_released() {
    let before = Sides::from([
      ("Red".to_string(), side(Some(Team::Red), &["Cicada", "Invigilator"])),
      ("Gold".to_string(), side(Some(Team::Gold), &["Lurker"])),
      ("Green".to_string(), side(Some(Team::Green), &["Picket"])),
    ]);
    let after = Sides::from([
      // Cicada was destroyed: Red is re-stated without it.
      ("Red".to_string(), side(Some(Team::Red), &["Invigilator"])),
      // Green is unchanged, so nothing is said about it.
      ("Green".to_string(), side(Some(Team::Green), &["Picket"])),
      // Someone switched a Blue ship to AI.
      ("Blue".to_string(), side(Some(Team::Blue), &["Executor"])),
    ]);
    let calls = plan_calls("s", &before, &after);
    assert_eq!(
      calls,
      vec![
        AiCall::Crew {
          scenario_id: "s".to_string(),
          side: "Blue".to_string(),
          team: Some(Team::Blue),
          ships: vec!["Executor".to_string()],
        },
        AiCall::Crew {
          scenario_id: "s".to_string(),
          side: "Red".to_string(),
          team: Some(Team::Red),
          ships: vec!["Invigilator".to_string()],
        },
        AiCall::Release {
          scenario_id: "s".to_string(),
          side: "Gold".to_string(),
        },
      ]
    );
    assert!(plan_calls("s", &after, &after).is_empty(), "nothing changed, nothing said");
  }

  /// The client posts the call as JSON to the right path, with the shared
  /// secret as its bearer token.
  #[tokio::test]
  async fn the_client_posts_the_call_with_its_credential() {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
      let (mut socket, _) = listener.accept().await.unwrap();
      let mut request = Vec::new();
      let mut buffer = [0u8; 4096];
      // Read until the body has arrived: the request is small and ends with
      // the closing brace of its JSON.
      while !request.ends_with(b"}") {
        let read = socket.read(&mut buffer).await.unwrap();
        if read == 0 {
          break;
        }
        request.extend_from_slice(&buffer[..read]);
      }
      socket
        .write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
        .await
        .unwrap();
      String::from_utf8(request).unwrap()
    });

    let client = AiServiceClient::new(&format!("{url}/"), Some("local-secret".to_string()));
    let call = AiCall::Crew {
      scenario_id: "thebus-1".to_string(),
      side: "Red".to_string(),
      team: Some(Team::Red),
      ships: vec!["Invigilator".to_string()],
    };
    client.post(&call).await.unwrap();

    let request = server.await.unwrap();
    assert!(request.starts_with("POST /crew "), "{request}");
    assert!(
      request.to_lowercase().contains("authorization: bearer local-secret"),
      "{request}"
    );
    assert!(
      request.ends_with(r#"{"scenario_id":"thebus-1","side":"Red","team":"Red","ships":["Invigilator"]}"#),
      "{request}"
    );
    assert!(!format!("{client:?}").contains("local-secret"));
  }
}

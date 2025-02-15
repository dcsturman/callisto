/**
 * All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all
 * here for completeness.
 */
use std::collections::HashMap;

use super::computer::FlightPathResult;
use super::crew::Crew;
use super::entity::Entities;
use super::ship::{ShipDesignTemplate, ShipSystem};
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};
use std::fmt::Display;

use super::entity::Vec3;
use super::ship::FlightPlan;

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct LoginMsg {
  pub code: String,
}

#[derive(Serialize, Deserialize, Debug, PartialEq)]
pub struct AuthResponse {
  pub email: String,
}

#[serde_as]
#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct AddShipMsg {
  pub name: String,
  #[serde_as(as = "Vec3asVec")]
  pub position: Vec3,
  #[serde_as(as = "Vec3asVec")]
  pub velocity: Vec3,
  #[serde_as(as = "Vec3asVec")]
  pub acceleration: Vec3,
  pub design: String,
  pub crew: Option<Crew>,
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug)]
pub struct SetCrewActions {
  pub ship_name: String,
  pub dodge_thrust: Option<u8>,
  pub assist_gunners: Option<bool>,
}

impl SetCrewActions {
  #[must_use]
  pub fn new(ship_name: &str) -> Self {
    SetCrewActions {
      ship_name: ship_name.to_string(),
      dodge_thrust: None,
      assist_gunners: None,
    }
  }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct LaunchMissileMsg {
  pub source: String,
  pub target: String,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct AddPlanetMsg {
  pub name: String,
  #[serde_as(as = "Vec3asVec")]
  pub position: Vec3,
  pub color: String,
  pub primary: Option<String>,
  pub radius: f64,
  pub mass: f64,
}

pub type RemoveEntityMsg = String;

#[derive(Serialize, Deserialize, Debug)]
pub struct SetPlanMsg {
  pub name: String,
  pub plan: FlightPlan,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComputePathMsg {
  pub entity_name: String,
  #[serde_as(as = "Vec3asVec")]
  pub end_pos: Vec3,
  #[serde_as(as = "Vec3asVec")]
  pub end_vel: Vec3,
  #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        //with = "::serde_with::rust::unwrap_or_skip"
        with = "::serde_with:: As :: < Option < Vec3asVec > >"
    )]
  pub target_velocity: Option<Vec3>,
  pub standoff_distance: f64,
}

pub type FlightPathMsg = FlightPathResult;

#[derive(Serialize, Deserialize, Debug)]
pub struct FireAction {
  pub weapon_id: u32,
  pub target: String,
  #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        //with = "::serde_with::rust::unwrap_or_skip"
    )]
  pub called_shot_system: Option<ShipSystem>,
}

pub type FireActionsMsg = Vec<(String, Vec<FireAction>)>;

pub const EMPTY_FIRE_ACTIONS_MSG: FireActionsMsg = vec![];

// We don't currently need this explicit type to document the response to a ListEntities (GET) request
// So including here as a comment for completeness.
// pub type ListEntitiesMsg = Entities;

#[serde_as]
#[derive(Serialize, Deserialize, PartialEq, Debug, Clone)]
#[serde(tag = "kind")]
pub enum EffectMsg {
  ShipImpact {
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
  },
  ExhaustedMissile {
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
  },
  ShipDestroyed {
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
  },
  BeamHit {
    #[serde_as(as = "Vec3asVec")]
    origin: Vec3,
    #[serde_as(as = "Vec3asVec")]
    position: Vec3,
  },
  Message {
    content: String,
  },
}
impl EffectMsg {
  #[must_use]
  pub fn message(content: String) -> EffectMsg {
    EffectMsg::Message { content }
  }
}

impl Display for EffectMsg {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "{self:?}")
  }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct LoadScenarioMsg {
  pub scenario_name: String,
}

pub type ShipDesignTemplateMsg = HashMap<String, ShipDesignTemplate>;

/*
 * Vec3asVec exists to allow us to serialize and deserialize Vec3 consistently with Javascript.  That is, as a \[f64;3\] rather than as a struct
 * with named elements x, y, and z.  i.e. [0.0, 0.0, 0.0] instead of [x: 0.0, y:0.0, z:0.0]
 */
serde_with::serde_conv!(
    pub Vec3asVec,
    Vec3,
    |v: &Vec3| [v.x, v.y, v.z],
    |value: [f64; 3]| -> Result<_, std::convert::Infallible> {
        Ok(Vec3 {
            x: value[0],
            y: value[1],
            z: value[2],
        })
    }
);

#[derive(Serialize, Deserialize, Debug)]
pub enum RequestMsg {
  Login(LoginMsg),
  AddShip(AddShipMsg),
  AddPlanet(AddPlanetMsg),
  Remove(RemoveEntityMsg),
  SetPlan(SetPlanMsg),
  ComputePath(ComputePathMsg),
  SetCrewActions(SetCrewActions),
  Update(FireActionsMsg),
  LoadScenario(LoadScenarioMsg),
  EntitiesRequest,
  DesignTemplateRequest,
  Logout,
  Quit,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ResponseMsg {
  AuthResponse(AuthResponse),
  DesignTemplateResponse(ShipDesignTemplateMsg),
  EntityResponse(Entities),
  FlightPath(FlightPathMsg),
  Effects(Vec<EffectMsg>),
  Users(Vec<String>),
  LaunchMissile(LaunchMissileMsg),
  SimpleMsg(String),
  // LogoutResponse is a faux message never sent back.  However,
  // it allows us to signal between the message handling layer and the connection
  // layer that we just dropped this connection.
  LogoutResponse,
  PleaseLogin,
  Error(String),
}

#[cfg(test)]
mod tests {
  use crate::ship::ShipDesignTemplate;

  use super::*;
  use crate::crew::Skills;
  use cgmath::Zero;
  use serde_json::json;
  use test_log::test;

  #[test(tokio::test)]
  async fn test_add_ship_msg() {
    let default_template_name = ShipDesignTemplate::default().name;

    let msg = AddShipMsg {
      name: "ship1".to_string(),
      position: Vec3::zero(),
      velocity: Vec3::zero(),
      acceleration: Vec3::zero(),
      design: default_template_name.clone(),
      crew: None,
    };
    let json = json!({
        "name": "ship1",
        "position": [0.0, 0.0, 0.0],
        "velocity": [0.0, 0.0, 0.0],
        "acceleration": [0.0, 0.0, 0.0],
        "design": "Buccaneer"
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());
  }

  #[test(tokio::test)]
  async fn test_add_ship_with_crew_msg() {
    let default_template_name = ShipDesignTemplate::default().name;
    let mut crew = Crew::new();
    crew.set_skill(Skills::Pilot, 2);
    crew.set_skill(Skills::EngineeringJump, 3);
    let msg = AddShipMsg {
      name: "ship1".to_string(),
      position: Vec3::zero(),
      velocity: Vec3::zero(),
      acceleration: Vec3::zero(),
      design: default_template_name.clone(),
      crew: Some(crew),
    };
    let json = json!({
        "name": "ship1",
        "position": [0.0, 0.0, 0.0],
        "velocity": [0.0, 0.0, 0.0],
        "acceleration": [0.0, 0.0, 0.0],
        "design": "Buccaneer",
        "crew": {
            "pilot": 2,
            "engineering_jump": 3,
            "engineering_power": 0,
            "engineering_maneuver": 0,
            "sensors": 0,
            "gunnery": []
        }
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());
  }

  #[test_log::test]
  fn test_compute_path_msg() {
    let msg = ComputePathMsg {
      entity_name: "ship1".to_string(),
      end_pos: Vec3::zero(),
      end_vel: Vec3::zero(),
      target_velocity: None,
      standoff_distance: 0.0,
    };

    let json = json!({
        "entity_name": "ship1",
        "end_pos": [0.0, 0.0, 0.0],
        "end_vel": [0.0, 0.0, 0.0],
        "standoff_distance": 0.0
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());

    let _response_msg = serde_json::from_str::<ComputePathMsg>(json_str.as_str()).unwrap();

    let msg2 = ComputePathMsg {
      entity_name: "ship1".to_string(),
      end_pos: Vec3::zero(),
      end_vel: Vec3::zero(),
      target_velocity: Some(Vec3 {
        x: 10.0,
        y: 20.0,
        z: 30.0,
      }),
      standoff_distance: 100.0,
    };

    let json2 = json!({
        "entity_name": "ship1",
        "end_pos": [0.0, 0.0, 0.0],
        "end_vel": [0.0, 0.0, 0.0],
        "target_velocity": [10.0, 20.0, 30.0],
        "standoff_distance": 100.0,
    });

    let json_str2 = serde_json::to_string(&msg2).unwrap();
    assert_eq!(json_str2, json2.to_string());

    let _response_msg2 = serde_json::from_str::<ComputePathMsg>(json_str2.as_str()).unwrap();
  }

  #[test_log::test]
  fn test_serialize_effect_msg() {
    let msg = EffectMsg::ShipImpact { position: Vec3::zero() };
    let json = json!({
        "kind" : "ShipImpact",
        "position": [0.0, 0.0, 0.0]
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());

    let msg = EffectMsg::Message {
      content: "2 points to the hull".to_string(),
    };
    let json = json!({
        "kind" : "Message",
        "content" : "2 points to the hull"
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());

    let msg = EffectMsg::ExhaustedMissile { position: Vec3::zero() };
    let json = json!({
        "kind" : "ExhaustedMissile",
        "position": [0.0, 0.0, 0.0]
    });

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());
  }

  #[test_log::test]
  fn test_serialize_fire_actions_msg() {
    let msg = vec![
      (
        "ship1".to_string(),
        vec![FireAction {
          weapon_id: 0,
          target: "ship2".to_string(),
          called_shot_system: None,
        }],
      ),
      (
        "ship2".to_string(),
        vec![FireAction {
          weapon_id: 1,
          target: "ship1".to_string(),
          called_shot_system: None,
        }],
      ),
    ];
    let json = json!([
        [
            "ship1", [
                {
                    "weapon_id": 0,
                    "target": "ship2"
                }
            ]
        ],
        [
            "ship2", [
                {
                    "weapon_id": 1,
                    "target": "ship1"
                }
            ]
        ]
    ]);

    let json_str = serde_json::to_string(&msg).unwrap();
    assert_eq!(json_str, json.to_string());
  }

  #[test]
  fn test_load_scenario_msg() {
    // Test serialization
    let msg = LoadScenarioMsg {
      scenario_name: "./scenarios/sol.json".to_string(),
    };

    let expected_json = json!({
        "scenario_name": "./scenarios/sol.json"
    });

    let serialized = serde_json::to_string(&msg).unwrap();
    assert_eq!(serialized, expected_json.to_string());

    // Test deserialization
    let json_str = r#"{"scenario_name": "./scenarios/sol.json"}"#;
    let deserialized: LoadScenarioMsg = serde_json::from_str(json_str).unwrap();
    assert_eq!(deserialized.scenario_name, "./scenarios/sol.json");

    // Test deserialization with invalid JSON
    let invalid_json = r#"{"wrong_field": "value"}"#;
    let result = serde_json::from_str::<LoadScenarioMsg>(invalid_json);
    assert!(result.is_err());
  }

  #[test]
  fn test_login_msg() {
    // Test with code present
    let msg_with_code = LoginMsg {
      code: "auth_code_123".to_string(),
    };

    let expected_json_with_code = json!({
        "code": "auth_code_123"
    });

    let serialized = serde_json::to_string(&msg_with_code).unwrap();
    assert_eq!(serialized, expected_json_with_code.to_string());

    // Test deserialization with code
    let json_str = r#"{"code": "auth_code_123"}"#;
    let deserialized: LoginMsg = serde_json::from_str(json_str).unwrap();
    assert_eq!(deserialized.code, "auth_code_123".to_string());
  }
}

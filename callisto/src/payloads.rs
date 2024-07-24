/*** All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all
 * here for completeness.
 */
use super::computer::FlightPathResult;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;

use super::entity::{Vec3, FlightPlan};

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct AddShipMsg {
    pub name: String,
    #[serde_as(as = "Vec3asVec")]
    pub position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    pub velocity: Vec3,
    #[serde_as(as = "Vec3asVec")]
    pub acceleration: Vec3,
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
    pub plan: FlightPlan
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
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        with = "::serde_with::rust::unwrap_or_skip"
    )]
    pub standoff_distance: Option<f64>,
}

pub type FlightPathMsg = FlightPathResult;

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct EffectMsg {
    #[serde_as(as = "Vec3asVec")]
    pub position: Vec3,
    pub kind: String,
}

// Constants for EffectMsg.kind
pub const SHIP_IMPACT: &str = "ShipImpact";
pub const EXHAUSTED_MISSILE: &str = "ExhaustedMissile";

// We don't currently need this explicit type to document the response to a ListEntities (GET) request
// So including here as a comment for completeness.
// pub type ListEntitiesMsg = Entities;


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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use cgmath::Zero;

    #[test]
    fn test_add_ship_msg() {
        let msg = AddShipMsg {
            name: "ship1".to_string(),
            position: Vec3::zero(),
            velocity: Vec3::zero(),
            acceleration: Vec3::zero(),
        };
        let json = json!({
            "name": "ship1",
            "position": [0.0, 0.0, 0.0],
            "velocity": [0.0, 0.0, 0.0],
            "acceleration": [0.0, 0.0, 0.0],
        });

        let json_str = serde_json::to_string(&msg).unwrap();
        assert_eq!(json_str, json.to_string());
    }

    #[test]
    fn test_compute_path_msg() {
        let msg = ComputePathMsg {
            entity_name: "ship1".to_string(),
            end_pos: Vec3::zero(),
            end_vel: Vec3::zero(),
            target_velocity: None,
            standoff_distance: None,
        };

        let json = json!({
            "entity_name": "ship1",
            "end_pos": [0.0, 0.0, 0.0],
            "end_vel": [0.0, 0.0, 0.0],
        });

        let json_str = serde_json::to_string(&msg).unwrap();
        assert_eq!(json_str, json.to_string());

        let rmsg = serde_json::from_str::<ComputePathMsg>(json_str.as_str()).unwrap();
        println!("rmsg {:?}", rmsg);

        let msg2 = ComputePathMsg {
            entity_name: "ship1".to_string(),
            end_pos: Vec3::zero(),
            end_vel: Vec3::zero(),
            target_velocity: Some(Vec3 { x: 10.0, y: 20.0, z: 30.0 }),
            standoff_distance: Some(100.0),
        };

        let json2 = json!({
            "entity_name": "ship1",
            "end_pos": [0.0, 0.0, 0.0],
            "end_vel": [0.0, 0.0, 0.0],
            "target_velocity": [10.0, 20.0, 30.0],
            "standoff_distance": 100.0,
        });

        let json_str2 = serde_json::to_string(&msg2).unwrap();
        println!("json_str2: {}", json_str2);
        assert_eq!(json_str2, json2.to_string());

        let rmsg2 = serde_json::from_str::<ComputePathMsg>(json_str2.as_str()).unwrap();
        println!("rmsg2 {:?}", rmsg2);
    }
}

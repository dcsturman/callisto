/*** All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all
 * here for completeness.
 */

use super::computer::FlightPlan;
use super::entity::UpdateAction;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_with::serde_as;

use super::entity::Vec3;

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
pub struct SetAccelerationMsg {
    pub name: String,
    pub acceleration: Vec3,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct ComputePathMsg {
    pub entity_name: String,
    #[serde_as(as = "Vec3asVec")]
    pub end_pos: Vec3,
    #[serde_as(as = "Vec3asVec")]
    pub end_vel: Vec3,
}

pub type FlightPathMsg = FlightPlan;

pub type UpdateActionsMsg = Vec<UpdateAction>;

// We don't currently need this explicit type to document the response to a ListEntities (GET) request
// So including here as a comment for completeness.
// pub type ListEntitiesMsg = Entities;

/**
 * Vec3asVec exists to allow us to serialize and deserialize Vec3 consistently with Javascript.  That is, as a \[f64;3\] rather than as a struct
 * with named elements x, y, and z.  i.e. [0.0, 0.0, 0.0] instead of [x: 0.0, y:0.0, z:0.0]
 */
pub struct Vec3asVec;
impl<'de> serde_with::DeserializeAs<'de, Vec3> for Vec3asVec {
    fn deserialize_as<D>(deserializer: D) -> Result<Vec3, D::Error>
    where
        D: Deserializer<'de>,
    {
        let v: Vec<f64> = Deserialize::deserialize(deserializer)?;
        Ok(Vec3::new(v[0], v[1], v[2]))
    }
}

impl serde_with::SerializeAs<Vec3> for Vec3asVec {
    fn serialize_as<S>(source: &Vec3, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let v = vec![source.x, source.y, source.z];
        Serialize::serialize(&v, serializer)
    }
}
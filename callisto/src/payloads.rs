use super::computer::FlightPlan;
/** All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all
 * here for completeness.  
 */
use super::entity::Vec3;
use serde::ser::{SerializeStruct, Serializer};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct AddShipMsg {
    pub name: String,
    pub position: Vec3,
    pub velocity: Vec3,
    pub acceleration: Vec3,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddMissileMsg {
    pub name: String,
    pub target: String,
    pub position: Vec3,
    pub acceleration: Vec3,
    pub burns: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct AddPlanetMsg {
    pub name: String,
    pub position: Vec3,
    pub color: String,
    pub primary: Vec3,
    pub mass: f64,
}

pub type RemoveEntityMsg = String;

#[derive(Serialize, Deserialize, Debug)]
pub struct SetAccelerationMsg {
    pub name: String,
    pub acceleration: Vec3,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ComputePathMsg {
    pub entity_name: String,
    pub end_pos: Vec3,
    pub end_vel: Vec3,
}

pub type FlightPathMsg = FlightPlan;

impl Serialize for FlightPathMsg {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut flight_path = serializer.serialize_struct("FlightPathMsg", 2)?;
        flight_path.serialize_field(
            "path",
            &self
                .path
                .iter()
                .map(|v| vec![v.x, v.y, v.z])
                .collect::<Vec<_>>(),
        )?;
        flight_path.serialize_field(
            "end_velocity",
            &vec![
                self.end_velocity.x,
                self.end_velocity.y,
                self.end_velocity.z,
            ],
        )?;
        flight_path.serialize_field(
            "accelerations",
            &self
                .accelerations
                .iter()
                .map(|(v, t)| (vec![v.x, v.y, v.z], *t))
                .collect::<Vec<_>>(),
        )?;
        flight_path.end()
    }
}

// We don't currently need this explicit type to document the response to a ListEntities (GET) request
// So including here as a comment for completeness.
// pub type ListEntitiesMsg = Entities;

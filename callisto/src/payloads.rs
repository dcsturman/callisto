/** All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all 
 * here for completeness.  
 */
use super::entity::{Entities, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct AddEntityMsg {
    pub name: String,
    pub position: Vec3, 
    pub velocity: Vec3,
    pub acceleration: Vec3,
}

pub type RemoveEntityMsg = String;

#[derive(Serialize, Deserialize, Debug)]
pub struct SetAccelerationMsg {
    pub name: String,
    pub acceleration: Vec3,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ComputeMsg {
    pub start_pos: Vec3,
    pub end_pos: Vec3,
    pub start_vel: Vec3,
    pub end_vel: Vec3,
    pub ship_name: String
}

#[derive(Serialize, Deserialize, Debug)]
pub struct FlightPathMsg {
    pub path: Vec<Vec3>,
    pub end_vel: Vec3
}

pub type ListEntitiesMsg = Entities;

/*** All the payloads used from the client to the server.  Some are not terribly meaningful or complex, but putting them all
 * here for completeness.
 */

use super::computer::FlightPlan;
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
pub struct AddMissileMsg {
    pub name: String,
    pub target: String,
    #[serde_as(as = "Vec3asVec")]
    pub position: Vec3,
    #[serde_as(as = "Vec3asVec")]
    pub acceleration: Vec3,
    pub burns: i32,
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug)]
pub struct AddPlanetMsg {
    pub name: String,
    #[serde_as(as = "Vec3asVec")]
    pub position: Vec3,
    pub color: String,
    #[serde_as(as = "Vec3asVec")]
    pub primary: Vec3,
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

// We don't currently need this explicit type to document the response to a ListEntities (GET) request
// So including here as a comment for completeness.
// pub type ListEntitiesMsg = Entities;

/**
 * Vec3asVec exists to allow us to serialize and deserialize Vec3 consistently with Javascript.  That is, as a [f64;3] rather than as a struct
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

/*
pub fn serialize_vec<S>(v: &Vec3, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut state = serializer.serialize_seq(Some(4))?;
    state.serialize_element(&v.x)?;
    state.serialize_element(&v.y)?;
    state.serialize_element(&v.z)?;
    state.end()
}

pub fn deserialize_vec<'de, D>(deserializer: D) -> Result<Vec3, D::Error>
where
    D: Deserializer<'de>,
{
    // define a visitor that deserializes
    struct VecVisitor;

    impl<'de> Visitor<'de> for VecVisitor {
        type Value = Vec3;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("3 floats in a sequence")
        }

        fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
        where
            A: SeqAccess<'de>,
        {
            let mut v = Vec3::zero();
            if let Some(x) = seq.next_element()? {
                v.x = x;
            } else {
                return Err(serde::de::Error::custom("expected 3 floats in a sequence"));
            }
            if let Some(y) = seq.next_element()? {
                v.y = y;
            } else {
                return Err(serde::de::Error::custom("expected 3 floats in a sequence"));
            }
            if let Some(z) = seq.next_element()? {
                v.z = z;
            } else {
                return Err(serde::de::Error::custom("expected 3 floats in a sequence"));
            }
            Ok(v)
        }
    }

    deserializer.deserialize_seq(VecVisitor)
}
*/

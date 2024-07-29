mod computer;
pub mod entity;
pub mod payloads;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};

use cgmath::InnerSpace;
use serde_json::from_slice;

use computer::{compute_flight_path, FlightParams};
use entity::{Entities, G};
use payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FlightPathMsg, LaunchMissileMsg, RemoveEntityMsg,
    SetPlanMsg,
};

enum SizeCheckError {
    SizeErr(Response<Full<Bytes>>),
    HyperErr(hyper::Error),
}

impl From<hyper::Error> for SizeCheckError {
    fn from(err: hyper::Error) -> Self {
        SizeCheckError::HyperErr(err)
    }
}

fn build_ok_response(body: &str) -> Response<Full<Bytes>> {
    let msg = Bytes::copy_from_slice(format!("{{ \"msg\" : \"{body}\" }}").as_bytes());
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("Access-Control-Allow-Origin", "*")
        .body(msg.into())
        .unwrap();

    resp.clone()
}

// Read a body while also protecting our server from massive bodies.
async fn get_body_size_check(req: Request<Incoming>) -> Result<Bytes, SizeCheckError> {
    let upper = req.body().size_hint().upper().unwrap_or(u64::MAX);
    if upper > 1024 * 64 {
        let mut resp: Response<Full<Bytes>> = Response::new("Body too big".as_bytes().into());
        *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
        return Err(SizeCheckError::SizeErr(resp));
    }
    let body_bytes = req.collect().await?.to_bytes();
    Ok(body_bytes)
}

macro_rules! deserialize_body_or_respond {
    ($req: ident, $msg_type:tt) => {{
        info!("Received and processing {} request.", stringify!($msg_type));
        let body_bytes = match get_body_size_check($req).await {
            Ok(bytes) => bytes,
            Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
            Err(SizeCheckError::HyperErr(err)) => return Err(err),
        };

        debug!("Body bytes: {:?}", body_bytes);

        let msg: $msg_type = match from_slice(&body_bytes) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("Invalid JSON ({}): {:?}", e, body_bytes);
                let mut resp: Response<Full<Bytes>> =
                    Response::new("Invalid JSON".as_bytes().into());
                *resp.status_mut() = StatusCode::BAD_REQUEST;
                return Ok(resp);
            }
        };
        msg
    }};
}

pub async fn handle_request(
    req: Request<Incoming>,
    entities: Arc<Mutex<Entities>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    info!(
        "Request: {:?}\n\tmethod: {}\n\turi: {}",
        req,
        req.method(),
        req.uri().path()
    );
    match (req.method(), req.uri().path()) {
        (&Method::OPTIONS, _) => {
            let mut resp = Response::new("".as_bytes().into());
            resp.headers_mut()
                .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
            resp.headers_mut().insert(
                "Access-Control-Allow-Methods",
                "POST, GET, OPTIONS".parse().unwrap(),
            );
            resp.headers_mut().insert(
                "Access-Control-Allow-Headers",
                "Content-Type".parse().unwrap(),
            );
            Ok(resp)
        }
        (&Method::POST, "/add_ship") => {
            let ship = deserialize_body_or_respond!(req, AddShipMsg);

            // Add the ship to the server
            entities.lock().unwrap().add_ship(
                ship.name,
                ship.position,
                ship.velocity,
                ship.acceleration,
            );

            Ok(build_ok_response("Add ship action executed"))
        }
        (&Method::POST, "/add_planet") => {
            let planet = deserialize_body_or_respond!(req, AddPlanetMsg);

            // Add the planet to the server
            entities.lock().unwrap().add_planet(
                planet.name,
                planet.position,
                planet.color,
                planet.primary,
                planet.radius,
                planet.mass,
            );

            Ok(build_ok_response("Add planet action executed"))
        }
        (&Method::POST, "/launch_missile") => {
            let missile = deserialize_body_or_respond!(req, LaunchMissileMsg);

            // Add the missile to the server
            entities
                .lock()
                .unwrap()
                .launch_missile(missile.source, missile.target);

            Ok(build_ok_response("Launch missile action executed"))
        }
        (&Method::POST, "/remove") => {
            let name = deserialize_body_or_respond!(req, RemoveEntityMsg);
            debug!("Removing entity: {}", name);

            // Remove the entity from the server
            entities.lock().unwrap().remove(&name);

            Ok(build_ok_response("Remove action executed"))
        }
        (&Method::POST, "/set_plan") => {
            let plan_msg = deserialize_body_or_respond!(req, SetPlanMsg);

            // Change the acceleration of the entity
            entities
                .lock()
                .unwrap()
                .set_flight_plan(&plan_msg.name, plan_msg.plan);

            Ok(build_ok_response("Set acceleration action executed"))
        }
        (&Method::POST, "/update") => {
            info!("Received and processing update request.");

            let effects = entities
                .lock()
                .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {}", e))
                .update_all();
            debug!("Effects: {:?}", effects);

            let json = match serde_json::to_string(&effects) {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting update actions to JSON".as_bytes().into(),
                    ));
                }
            };

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            Ok(resp)
        }

        (&Method::POST, "/compute_path") => {
            let msg = deserialize_body_or_respond!(req, ComputePathMsg);

            info!(
                "(/compute_path) Received and processing compute path request. {:?}",
                msg
            );

            // Temporary until ships have actual acceleration built in
            const MAX_ACCELERATION: f64 = 6.0 * G;

            debug!(
                "(/compute_path) Computing path for entity: {} End pos: {:?} End vel: {:?}",
                msg.entity_name, msg.end_pos, msg.end_vel
            );
            // Do this in a block to clean up the lock as soon as possible.
            let (start_pos, start_vel) = {
                let entities = entities.lock().unwrap();
                let entity = entities.get(&msg.entity_name).unwrap().read().unwrap();
                (entity.get_position(), entity.get_velocity())
            };

            let adjusted_end_pos = if msg.standoff_distance > 0.0 {
                msg.end_pos - (msg.end_pos - start_pos).normalize() * msg.standoff_distance
            } else {
                msg.end_pos
            };

            if msg.standoff_distance > 0.0 {
                debug!("(/compute_path) Standoff distance: {:0.0?} Adjusted end pos: {:0.0?} Original end pos {:0.0?}Difference {:0.0?}", msg.standoff_distance, adjusted_end_pos, msg.end_pos, 
                    (adjusted_end_pos - msg.end_pos).magnitude());
            }

            let params = FlightParams::new(
                start_pos,
                adjusted_end_pos,
                start_vel,
                msg.end_vel,
                msg.target_velocity,
                MAX_ACCELERATION,
            );

            debug!("(/compute_path)Call computer with params: {:?}", params);

            let plan: FlightPathMsg = compute_flight_path(&params);

            let json = match serde_json::to_string(&plan) {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting flight path to JSON".as_bytes().into(),
                    ));
                }
            };

            debug!("(/compute_path) Flight path response: {}", json);

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            Ok(resp)
        }

        (&Method::GET, "/") => {
            info!("Received and processing get request.");
            let json = match serde_json::to_string::<Entities>(&entities.lock().unwrap()) {
                Ok(json) => {
                    info!("Entities: {:?}", json);
                    json
                }
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting entities to JSON".as_bytes().into(),
                    ));
                }
            };

            debug!("Entities response: {}", json);
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();

            Ok(resp)
        }
        _ => {
            // Return a 404 Not Found response for any other requests
            Ok(Response::builder()
                .status(404)
                .body("Not Found".as_bytes().into())
                .unwrap())
        }
    }
}

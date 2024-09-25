pub mod combat;
mod computer;
mod damage_tables;
pub mod entity;
pub mod missile;
pub mod payloads;
pub mod planet;
pub mod ship;
pub mod server;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};

use rand::rngs::SmallRng;
use rand::SeedableRng;

use serde_json::from_slice;

use server::Server;

use entity::Entities;
use payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FireActionsMsg, RemoveEntityMsg,
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
    test_mode: bool
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let rng = &mut if test_mode {
        info!("(lib.handleRequest) Server in TEST mode.");
        // Use 0 to seed all test case random number generators.
        Box::new(SmallRng::seed_from_u64(0))
    } else {
        info!("(lib.handleRequest) Server in standard mode.");
        Box::new(SmallRng::from_entropy())
    };

    info!(
        "Request: {:?}\n\tmethod: {}\n\turi: {}",
        req,
        req.method(),
        req.uri().path()
    );

    let server = Server::new(entities);

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

            match server.add_ship(ship) {
                Ok(msg) => Ok(build_ok_response(&msg)),
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }
        }
        (&Method::POST, "/add_planet") => {
            let planet = deserialize_body_or_respond!(req, AddPlanetMsg);

            match server.add_planet(planet) {
                Ok(msg) => Ok(build_ok_response(&msg)),
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }            
        }
        (&Method::POST, "/remove") => {
            let name = deserialize_body_or_respond!(req, RemoveEntityMsg);

            match server.remove(name) {
                Ok(msg) => Ok(build_ok_response(&msg)),
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }
        }
        (&Method::POST, "/set_plan") => {
            info!("Received and processing plan set request.");
            let plan_msg = deserialize_body_or_respond!(req, SetPlanMsg);

            match server.set_plan(plan_msg) {
                Ok(msg) => Ok(build_ok_response(&msg)),
                Err(err) => {
                    let resp: Response<Full<Bytes>> = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Bytes::copy_from_slice(err.as_bytes()).into())
                    .unwrap();
                    return Ok(resp);
                }
            }
        }
        (&Method::POST, "/update") => {
            info!("(/update) Received and processing update request.");

            // Get the set of fire actions provided with the REST call to update
            // Fire actions are organized by each ship attacker in a two element tuple.
            // The first element is the name of the ship. The second element is a vector of FireActions.
            let fire_actions = deserialize_body_or_respond!(req, FireActionsMsg);

            match server.update(fire_actions, rng) {
                Ok(msg) => {
                    let resp = Response::builder()
                    .status(StatusCode::OK)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Bytes::copy_from_slice(msg.as_bytes()).into())
                    .unwrap();
                    Ok(resp)
                }
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }
        }

        (&Method::POST, "/compute_path") => {
            let msg = deserialize_body_or_respond!(req, ComputePathMsg);

            match server.compute_path(msg) {
                Ok(json) => {
                    let resp = Response::builder()
                    .status(StatusCode::OK)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Bytes::copy_from_slice(json.as_bytes()).into())
                    .unwrap();
                    Ok(resp)
                }
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }
        }

        (&Method::GET, "/") => {
            info!("Received and processing get request.");
            match server.get() {
                Ok(json) => {
                    let resp = Response::builder()
                    .status(StatusCode::OK)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Bytes::copy_from_slice(json.as_bytes()).into())
                    .unwrap();
                    Ok(resp)
                    }
                Err(err) => Ok(Response::new(Bytes::copy_from_slice(err.as_bytes()).into())),
            }
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

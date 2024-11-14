pub mod authentication;
pub mod combat;
mod combat_tables;
mod computer;
pub mod entity;
pub mod missile;
pub mod payloads;
pub mod planet;
pub mod server;
pub mod ship;

#[macro_use]
mod cov_util;

#[cfg(test)]
pub mod tests;

extern crate pretty_env_logger;

use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};

use once_cell::sync::OnceCell;

use serde_json::from_slice;

use server::Server;

use entity::Entities;
use payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FireActionsMsg, LoginMsg, RemoveEntityMsg, SetPlanMsg,
};

use authentication::TokenTimeoutError;

pub const STATUS_INVALID_TOKEN: u16 = 498;

pub static AUTHORIZED_USERS: OnceCell<Vec<String>> = OnceCell::new();

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

pub fn load_authorized_users_from_file(
    file_name: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(file_name)?;
    let reader = std::io::BufReader::new(file);
    let mut templates: Vec<String> = serde_json::from_reader(reader)?;
    templates.sort();

    Ok(templates)
}

async fn check_authorization(
    req: &Request<Incoming>,
    authenticator: Arc<crate::authentication::Authenticator>,
) -> Result<String, (hyper::StatusCode, String)> {
    let auth_header = req.headers().get("Authorization");
    debug!("(lib.check_authorization) Authorization header found.",);

    // Need to check if email address is valid and if it on our list of accepted users
    match auth_header {
        Some(header) => {
            let token = header.to_str().unwrap();
            let valid = authenticator.validate_session_key(token);

            match valid {
                Ok(email) => {
                    if AUTHORIZED_USERS.get().unwrap().contains(&email) {
                        Ok(email)
                    } else {
                        Err((
                            hyper::StatusCode::FORBIDDEN,
                            "Unauthorized user".to_string(),
                        ))
                    }
                }
                Err(e) => {
                    error!("(lib.check_authorization) Invalid session key: {:?}", e);
                    Err((
                        hyper::StatusCode::UNAUTHORIZED,
                        "Invalid session key".to_string(),
                    ))
                }
            }
        }
        None => Err((
            hyper::StatusCode::UNAUTHORIZED,
            "No Authorization header".to_string(),
        )),
    }
}

pub async fn handle_request(
    req: Request<Incoming>,
    entities: Arc<Mutex<Entities>>,
    test_mode: bool,
    authenticator: Arc<crate::authentication::Authenticator>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    info!(
        "Request: {:?}\n\tmethod: {}\n\turi: {}",
        req,
        req.method(),
        req.uri().path()
    );

    // Check authorization (session key) except in a few very specific cases.  We call that out
    // here as its easier to see what we aren't authenticating.
    if !test_mode && !(req.method() == Method::OPTIONS || req.uri().path() == "/login") {
        match check_authorization(&req, authenticator.clone()).await {
            Ok(email) => {
                debug!("(lib.handleRequest) User {} authorized.", email);
            }
            Err((status, msg)) => {
                warn!(
                    "(lib.handleRequest) User not authorized with status {} and message {}.",
                    status, msg
                );

                return Ok(Response::builder()
                    .status(status)
                    .body(Bytes::copy_from_slice(msg.as_bytes()).into())
                    .unwrap());
            }
        }
    } else if test_mode {
        warn!("(lib.handleRequest) Server in test mode.  All users authorized.");
    } else {
        debug!("(lib.handleRequest) Ignore authentication for this request.");
    }

    let mut server = Server::new(entities, test_mode);

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
                "Content-Type, Authorization".parse().unwrap(),
            );
            Ok(resp)
        }
        (&Method::POST, "/login") => {
            let login_msg = deserialize_body_or_respond!(req, LoginMsg);

            match server.login(login_msg, authenticator.clone()).await {
                Ok(msg) => {
                    debug!(
                        "(lib.handleRequest/login) Received and processing login request. {:?}",
                        msg
                    );
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
                Ok(_) => Ok(build_ok_response("Set acceleration action executed")),
                Err(err) => {
                    warn!("(/set_plan)) Error setting plan: {}", err);
                    let resp: Response<Full<Bytes>> = Response::builder()
                        .status(StatusCode::BAD_REQUEST)
                        .header("Access-Control-Allow-Origin", "*")
                        .body(Bytes::copy_from_slice(err.as_bytes()).into())
                        .unwrap();
                    Ok(resp)
                }
            }
        }
        (&Method::POST, "/update") => {
            info!("(/update) Received and processing update request.");

            // Get the set of fire actions provided with the REST call to update
            // Fire actions are organized by each ship attacker in a two element tuple.
            // The first element is the name of the ship. The second element is a vector of FireActions.
            let fire_actions = deserialize_body_or_respond!(req, FireActionsMsg);

            match server.update(fire_actions) {
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

        (&Method::GET, "/designs") => {
            info!("Received and processing get designs request.");
            match server.get_designs() {
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
            info!("Unknown method or URI on this request.  Returning 404.");
            // Return a 404 Not Found response for any other requests
            Ok(Response::builder()
                .status(404)
                .body("Not Found".as_bytes().into())
                .unwrap())
        }
    }
}

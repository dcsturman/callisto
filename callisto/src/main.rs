mod computer;
mod entity;
mod payloads;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::from_slice;
use tokio::net::TcpListener;

use computer::{compute_flight_path, FlightParams};
use entity::Entities;
use payloads::{
    AddMissileMsg, AddPlanetMsg, AddShipMsg, ComputePathMsg, FlightPathMsg, RemoveEntityMsg,
    SetAccelerationMsg,
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
        info!("Received and processing {} request.",stringify!($msg_type));
        let body_bytes = match get_body_size_check($req).await {
            Ok(bytes) => bytes,
            Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
            Err(SizeCheckError::HyperErr(err)) => return Err(err),
        };

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

async fn handle_request(
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
                planet.mass,
            );

            Ok(build_ok_response("Add planet action executed"))
        }
        (&Method::POST, "/add_missile") => {
            let missile = deserialize_body_or_respond!(req, AddMissileMsg);

            // Add the missile to the server
            entities.lock().unwrap().add_missile(
                missile.name,
                missile.position,
                missile.target,
                missile.burns,
                Arc::downgrade(&entities),
            );

            Ok(build_ok_response("Add missile action executed"))
        }
        (&Method::POST, "/remove") => {
            let name = deserialize_body_or_respond!(req, RemoveEntityMsg);

            // Remove the entity from the server
            entities.lock().unwrap().remove(&name);

            Ok(build_ok_response("Remove action executed"))
        }
        (&Method::POST, "/setaccel") => {
            let accel_msg = deserialize_body_or_respond!(req, SetAccelerationMsg);

            // Change the acceleration of the entity
            entities
                .lock()
                .unwrap()
                .set_acceleration(&accel_msg.name, accel_msg.acceleration);

            Ok(build_ok_response("Set acceleration action executed"))
        }
        (&Method::POST, "/update") => {
            info!("Received and processing update request.");
            entities.lock().unwrap().update_all();
            Ok(build_ok_response("Update action executed"))
        }

        (&Method::POST, "/computepath") => {
            let msg = deserialize_body_or_respond!(req, ComputePathMsg);

            // Temporary until ships have actual acceleration built in
            const MAX_ACCELERATION: f64 = 6.0;

            debug!(
                "Computing path for entity: {} End pos: {:?} End vel: {:?}",
                msg.entity_name, msg.end_pos, msg.end_vel
            );
            // Do this in a block to clean up the lock as soon as possible.
            let (start_pos, start_vel) = {
                let entities = entities.lock().unwrap();
                let entity = entities.get(&msg.entity_name).unwrap();
                (entity.get_position(), entity.get_velocity())
            };

            let params = FlightParams::new(
                start_pos,
                msg.end_pos,
                start_vel,
                msg.end_vel,
                MAX_ACCELERATION,
            );

            debug!("Call computer with params: {:?}", params);

            let plan: FlightPathMsg = compute_flight_path(&params);

            let json = match serde_json::to_string(&plan) {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting flight path to JSON".as_bytes().into(),
                    ));
                }
            };

            debug!("Flight path response: {}", json);

            Ok(build_ok_response(json.as_str()))
        }

        (&Method::GET, "/") => {
            let json = match entities.lock().unwrap().to_json() {
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

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build the main entities table that will be the state of our server.
    let entities = Arc::new(Mutex::new(Entities::new()));

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));

    println!("Starting Callisto server listening on address: {}", addr);

    pretty_env_logger::init();

    // We create a TcpListener and bind it to 127.0.0.1:3000
    let listener = TcpListener::bind(addr).await?;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        let e = entities.clone();
        tokio::task::spawn(async move {
            let ent = e.clone();
            let handler = move |req| handle_request(req, ent.clone());

            // We bind the incoming connection to our service
            let builder = http1::Builder::new();
            if let Err(err) = builder.serve_connection(io, service_fn(handler)).await {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

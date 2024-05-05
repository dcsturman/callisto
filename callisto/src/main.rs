mod entity;
mod payloads;
mod computer;

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

use entity::Entities;
use payloads::{ AddEntityMsg, RemoveEntityMsg, SetAccelerationMsg, ComputeMsg, FlightPathMsg };
use computer::{ compute_flight_path, FlightParams };

enum SizeCheckError {
    SizeErr(Response<Full<Bytes>>),
    HyperErr(hyper::Error),
}

impl From<hyper::Error> for SizeCheckError {
    fn from(err: hyper::Error) -> Self {
        SizeCheckError::HyperErr(err)
    }
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

async fn handle_request(
    req: Request<Incoming>,
    entities: Arc<Mutex<Entities>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    info!(
        "Request: {:?} method: {} uri: {}",
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
        (&Method::POST, "/add") => {
            info!("Processing add request");
            // Read the body of the request
            let body_bytes = match get_body_size_check(req).await {
                Ok(bytes) => bytes,
                Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
                Err(SizeCheckError::HyperErr(err)) => return Err(err),
            };

            // Deserialize the entity from JSON.  AddEntityMsg is just an Entity.
            let entity: AddEntityMsg = match from_slice(&body_bytes) {
                Ok(entity) => entity,
                Err(e) => {
                    debug!("Invalid JSON ({}): {:?}", e, body_bytes);
                    let mut resp: Response<Full<Bytes>> =
                        Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body("{ \"msg\" : \"Add action executed\" }".as_bytes().into())
                .unwrap();

            // Add the entity to the server
            entities.lock().unwrap().add(
                entity.name,
                entity.position,
                entity.velocity,
                entity.acceleration,
            );

            Ok(response)
        }
        (&Method::POST, "/remove") => {
            let body_bytes = match get_body_size_check(req).await {
                Ok(bytes) => bytes,
                Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
                Err(SizeCheckError::HyperErr(err)) => return Err(err),
            };

            // Deserialize the name of the entity to remove from JSON.
            let name: RemoveEntityMsg = match from_slice(&body_bytes) {
                Ok(name) => name,
                Err(_) => {
                    let mut resp: Response<Full<Bytes>> =
                        Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body("{ \"msg\" : \"Remove action executed\" }".as_bytes().into())
                .unwrap();

            // Remove the entity from the server
            entities.lock().unwrap().remove(&name);

            Ok(response)
        }
        (&Method::POST, "/setaccel") => {
            let body_bytes = match get_body_size_check(req).await {
                Ok(bytes) => bytes,
                Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
                Err(SizeCheckError::HyperErr(err)) => return Err(err),
            };

            // Deserialize the SetAccelerationMsg from JSON.
            let accel_msg: SetAccelerationMsg = match from_slice(&body_bytes) {
                Ok(msg) => msg,
                Err(_) => {
                    let mut resp: Response<Full<Bytes>> =
                        Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(
                    "{ \"msg\" : \"Change acceleration action executed\" }"
                        .as_bytes()
                        .into(),
                )
                .unwrap();

            // Change the acceleration of the entity
            entities
                .lock()
                .unwrap()
                .set_acceleration(&accel_msg.name, accel_msg.acceleration);

            Ok(response)
        }
        (&Method::POST, "/update") => {
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(
                    "{ \"msg\" : \"Time and position updated.\" }"
                        .as_bytes()
                        .into(),
                )
                .unwrap();

            entities.lock().unwrap().update_all();

            Ok(response)
        }

        (&Method::POST, "/compute") => {
            let body_bytes = match get_body_size_check(req).await {
                Ok(bytes) => bytes,
                Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
                Err(SizeCheckError::HyperErr(err)) => return Err(err),
            };

            // Deserialize the SetAccelerationMsg from JSON.
            let msg: ComputeMsg = match from_slice(&body_bytes) {
                Ok(msg) => msg,
                Err(_) => {
                    let mut resp: Response<Full<Bytes>> =
                        Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };
            // Temporary until ships have actual acceleration built in
            const MAX_ACCELERATION: f64 = 6.0;
            let params = FlightParams::new(msg.start_pos, msg.end_pos, msg.start_vel, msg.end_vel, MAX_ACCELERATION);

            let (flight_path, final_vel) = compute_flight_path(&params);

            let path_msg = FlightPathMsg{path: flight_path, end_vel: final_vel};
            let json = match serde_json::to_string(&path_msg) {
                Ok(json) => {
                    info!("Flight path computed and serialized.");
                    json
                }
                Err(_) => {
                    error!("Unable to serialize flight path for {:?}.", path_msg);
                    debug!("Flight path msg = {:?}", path_msg);
                    return Ok(Response::new(
                        "Error serializing calculated flight path to JSON".as_bytes().into()
                    ));
                }
            };

            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(json.into())
                .unwrap();
            Ok(response)
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

            let jb = json.clone();
            let response = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(jb.into())
                .unwrap();
            Ok(response)
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

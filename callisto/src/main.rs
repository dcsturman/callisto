mod entity;
mod payloads;

extern crate pretty_env_logger;
#[macro_use] extern crate log;

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
use payloads::{AddEntityMsg, RemoveEntityMsg, SetAccelerationMsg};

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
    info!("Request: {:?}", req);
    match (req.method(), req.uri().path()) {
        (&Method::POST, "/add") => {
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
                    let mut resp: Response<Full<Bytes>> = Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            // Add the entity to the server
            entities.lock().unwrap().add(entity.name, entity.position);

            Ok(Response::new("Add action executed".as_bytes().into()))
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
                    let mut resp: Response<Full<Bytes>> = Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            // Remove the entity from the server
            entities.lock().unwrap().remove(&name);

            Ok(Response::new("Remove action executed".as_bytes().into()))
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
                    let mut resp: Response<Full<Bytes>> = Response::new("Invalid JSON".as_bytes().into());
                    *resp.status_mut() = StatusCode::BAD_REQUEST;
                    return Ok(resp);
                }
            };

            // Change the acceleration of the entity
            entities
                .lock()
                .unwrap()
                .set_acceleration(&accel_msg.name, accel_msg.acceleration);

            Ok(Response::<Full<Bytes>>::new(
                "Change acceleration action executed".as_bytes().into(),
            ))
        }
        (&Method::POST, "/update") => {
            entities.lock().unwrap().update_all();
            Ok(Response::<Full<Bytes>>::new("Time and postions updated".as_bytes().into()))
        }

        (&Method::GET, "/") => {
            let json = match entities.lock().unwrap().to_json() {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting entities to JSON".as_bytes().into(),
                    ));
                }
            };

            let jb = json.clone();
            Ok(Response::<Full<Bytes>>::new(jb.into()))
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
            let handler = move |req| 
                handle_request(req, ent.clone());

            // We bind the incoming connection to our service
            let builder = http1::Builder::new();
            if let Err(err) = builder.serve_connection(io, service_fn(handler)).await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

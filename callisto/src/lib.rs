pub mod authentication;
pub mod combat;
mod combat_tables;
mod computer;
pub mod crew;
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

use headers::{
    AccessControlAllowCredentials, AccessControlAllowHeaders, AccessControlAllowMethods,
    AccessControlAllowOrigin, Cookie, HeaderMapExt,
};
use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};
use serde_json::from_slice;
use std::convert::TryFrom;

use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;

use std::fs::File;
use std::io::{BufReader, Read};

use authentication::Authenticator;
use entity::Entities;
use payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FireActionsMsg, LoadScenarioMsg, LoginMsg,
    RemoveEntityMsg, SetCrewActions, SetPlanMsg,
};
use server::Server;

pub const STATUS_INVALID_TOKEN: u16 = 498;
pub const SESSION_COOKIE_NAME: &str = "callisto-session-key";
enum SizeCheckError {
    SizeErr(Response<Full<Bytes>>),
    HyperErr(hyper::Error),
}

impl From<hyper::Error> for SizeCheckError {
    fn from(err: hyper::Error) -> Self {
        SizeCheckError::HyperErr(err)
    }
}

// Add the standard authentication errors to a response.
// We use these all over the place so adding to this util function so there is one
// place to check and modify these.
fn add_auth_headers(resp: &mut Response<Full<Bytes>>, web_backend: &str) {
    let allow_origin = AccessControlAllowOrigin::try_from(web_backend).unwrap();
    resp.headers_mut().typed_insert(allow_origin);
    let allow_credentials = AccessControlAllowCredentials;

    resp.headers_mut().typed_insert(allow_credentials);
}

fn build_ok_response(body: &str, web_backend: &str) -> Response<Full<Bytes>> {
    let msg = Bytes::copy_from_slice(body.as_bytes());

    let mut resp = Response::builder()
        .status(StatusCode::OK)
        .body(msg.into())
        .unwrap();

    add_auth_headers(&mut resp, web_backend);

    resp.clone()
}

fn build_err_response(status: StatusCode, body: &str, web_backend: &str) -> Response<Full<Bytes>> {
    let msg = Bytes::copy_from_slice(format!("{{ \"msg\" : \"{body}\" }}").as_bytes());
    let mut resp = Response::builder().status(status).body(msg.into()).unwrap();

    add_auth_headers(&mut resp, web_backend);
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

/// This is the main server loop, handling each request the server receives.
///
/// First this method needs to check if the user is authenticated on each request.  If not
/// then we go into our authentication flow.  It also much handle CORS messages.  Beyond that
/// messages are either POST or GET. Most of the logic for these should be handled by [Server]
/// so that unit testing of the logic is possible.
///
/// # Arguments
///
/// * `req` - The request to handle
/// * `entities` - The entities table. Each invocation is a new ref count/[clone](Arc::clone)
/// * `test_mode` - Whether we are in test mode.  Test mode disables authentication and ensures a deterministic seed for each random number generator.
/// * `authenticator` - The authenticator to use.
///
/// # Returns
///
/// A [Response] with the appropriate headers and body.
///
/// # Errors
///
/// Will return an `Err` when login fails.
///
/// # Panics
///
/// Will panic as a quick exit on a QUIT message.  Only possible when in test mode.
///
#[allow(clippy::too_many_lines)]
pub async fn handle_request(
    req: Request<Incoming>,
    entities: Arc<Mutex<Entities>>,
    test_mode: bool,
    authenticator: Arc<Box<dyn Authenticator>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    info!(
        "Request: {:?}\n\tmethod: {}\n\turi: {}",
        req,
        req.method(),
        req.uri().path()
    );

    // See if we have a proper session authorization cookie and from that generate a valid email.
    // This is a chain of events all of which have to be okay.
    // We need to have a cookie in the headers; then that cookie needs to be a callisto session key, and then it has to be one we know about it in our local table.
    // In the end though we just need to know if we have it or not.
    let cookie = req
        .headers()
        .typed_get::<Cookie>()
        .and_then(|cookies| cookies.get(SESSION_COOKIE_NAME).map(ToString::to_string));

    let mut server = Server::new(entities, authenticator.clone(), test_mode);
    let valid_email = cookie.and_then(|cookie| server.validate_session_key(&cookie).ok());

    // If we don't have a valid email, we reply with an Authorization error to the client.
    // The exceptions to doing that are 
    // 1) if we're doing an OPTIONS request (to get CORS headers) or
    // 2) if we're doing a login request.  Login will
    // have its own custom logic to test here.
    if let Some(email) = valid_email.clone() {
        debug!("(lib.handleRequest) User {} is authorized.", email);
    } else if valid_email.is_none()
        && !(req.method() == Method::OPTIONS || req.uri().path() == "/login")
    {
        debug!("(lib.handleRequest) No valid email.  Returning 401.");
        return Ok(Response::builder()
            .status(StatusCode::UNAUTHORIZED)
            .body(Bytes::copy_from_slice("Unauthorized".as_bytes()).into())
            .unwrap());
    } else {
        debug!("(lib.handleRequest) Ignore authentication for this {:?} request.", req.method());
    }

    let web_server = authenticator.as_ref().get_web_server();

    match (req.method(), req.uri().path()) {
        (&Method::OPTIONS, curious) => {
            debug!(
                "(lib.handleRequest) Received and processing OPTIONS request with uri: {}",
                curious
            );
            let mut resp = Response::new("".as_bytes().into());
            add_auth_headers(&mut resp, &web_server);
            let allow_methods = vec![
                hyper::http::Method::POST,
                hyper::http::Method::GET,
                hyper::http::Method::OPTIONS,
            ]
            .into_iter()
            .collect::<AccessControlAllowMethods>();
            let allow_headers = vec![
                hyper::http::header::CONTENT_TYPE,
                hyper::http::header::AUTHORIZATION,
                hyper::http::header::COOKIE,
                hyper::http::header::ACCESS_CONTROL_ALLOW_CREDENTIALS,
            ]
            .into_iter()
            .collect::<AccessControlAllowHeaders>();

            resp.headers_mut().typed_insert(allow_methods);
            resp.headers_mut().typed_insert(allow_headers);

            Ok(resp)
        }
        (&Method::POST, "/login") => {
            let login_msg = deserialize_body_or_respond!(req, LoginMsg);

            // When we call this it might be trivial - if valid_email is Some(_).
            // But we put all this business logic into [Server.login](Server::login) rather than 
            // split it up between the two locations.
            // Our role here is just to repackage the response and put it on the wire.
            match server.login(login_msg, &valid_email).await {
                Ok((auth_response, session_key)) => {
                    info!(
                        "(lib.handleRequest/login) LOGIN request successful for user {:?} with session key {:?}.",
                        auth_response.email,
                        if session_key.is_none() { "No key".to_string() } else if test_mode { session_key.clone().unwrap() } else { "**********".to_string() }
                    );

                    let mut resp = build_ok_response(
                        &serde_json::to_string(&auth_response).unwrap(),
                        &web_server,
                    );
                    // Add the set-cookie header only when we didn't have a valid cookie before.
                    if valid_email.is_none() && session_key.is_some() {
                        debug!("(lib.handleRequest/login) Adding session key as secure cookie to response.");

                        // Unfortunate that I cannot do this typed but the libraries for typed SetCookie look very broken.
                        let cookie_str = format!(
                            "{}={}; HttpOnly; Secure; SameSite=Strict",
                            SESSION_COOKIE_NAME,
                            session_key.unwrap()
                        );

                        resp.headers_mut()
                            .append("Set-Cookie", cookie_str.parse().unwrap());
                    }
                    Ok(resp)
                }
                Err(err) => {
                    // A few cases where we can end up here.
                    // 1) When authentication fails
                    // 2) When a client first loads and it doesn't know if it has a valid session key.
                    // 3) If the cookie times out and needs to be refreshed.
                    warn!(
                        "(lib.handleRequest/login) LOGIN: Invalid login attempt, returning UNAUTHORIZED.",
                    );
                    Ok(build_err_response(
                        StatusCode::UNAUTHORIZED,
                        &err,
                        &web_server,
                    ))
                }
            }
        }
        (&Method::POST, "/add_ship") => {
            let ship = deserialize_body_or_respond!(req, AddShipMsg);

            match server.add_ship(ship) {
                Ok(msg) => Ok(build_ok_response(&msg, &web_server)),
                Err(err) => Ok(build_err_response(
                    StatusCode::BAD_REQUEST,
                    &err,
                    &web_server,
                )),
            }
        }
        (&Method::POST, "/set_crew_actions") => {
            let request = deserialize_body_or_respond!(req, SetCrewActions);

            match server.set_crew_actions(&request) {
                Ok(msg) => Ok(build_ok_response(&msg, &web_server)),
                Err(err) => Ok(build_err_response(
                    StatusCode::BAD_REQUEST,
                    &err,
                    &web_server,
                )),
            }
        }
        (&Method::POST, "/add_planet") => {
            let planet = deserialize_body_or_respond!(req, AddPlanetMsg);

            match server.add_planet(planet) {
                Ok(msg) => Ok(build_ok_response(&msg, &web_server)),
                Err(err) => Ok(build_err_response(
                    StatusCode::BAD_REQUEST,
                    &err,
                    &web_server,
                )),
            }
        }

        (&Method::POST, "/remove") => {
            let name = deserialize_body_or_respond!(req, RemoveEntityMsg);

            match server.remove(&name) {
                Ok(msg) => Ok(build_ok_response(&msg, &web_server)),
                Err(err) => Ok(build_err_response(
                    StatusCode::BAD_REQUEST,
                    &err,
                    &web_server,
                )),
            }
        }
        (&Method::POST, "/set_plan") => {
            info!("Received and processing plan set request.");
            let plan_msg = deserialize_body_or_respond!(req, SetPlanMsg);

            match server.set_plan(&plan_msg) {
                Ok(msg) => Ok(build_ok_response(&msg, &web_server)),
                Err(err) => {
                    warn!("(/set_plan)) Error setting plan: {}", err);
                    Ok(build_err_response(
                        StatusCode::BAD_REQUEST,
                        &err,
                        &web_server,
                    ))
                }
            }
        }
        (&Method::POST, "/update") => {
            info!("(/update) Received and processing update request.");

            // Get the set of fire actions provided with the REST call to update
            // Fire actions are organized by each ship attacker in a two element tuple.
            // The first element is the name of the ship. The second element is a vector of FireActions.
            let fire_actions = deserialize_body_or_respond!(req, FireActionsMsg);

            let msg = server.update(fire_actions);
            let mut resp = Response::builder()
                .status(StatusCode::OK)
                .body(Bytes::copy_from_slice(msg.as_bytes()).into())
                .unwrap();
            add_auth_headers(&mut resp, &web_server);
            Ok(resp)
        }

        (&Method::POST, "/compute_path") => {
            let msg = deserialize_body_or_respond!(req, ComputePathMsg);
            match server.compute_path(&msg) {
                Ok(json) => {
                    let mut resp = Response::builder()
                        .status(StatusCode::OK)
                        .body(Bytes::copy_from_slice(json.as_bytes()).into())
                        .unwrap();
                    add_auth_headers(&mut resp, &web_server);
                    Ok(resp)
                }
                Err(err) => Ok(build_err_response(
                    StatusCode::BAD_REQUEST,
                    &err,
                    &web_server,
                )),
            }
        }

        (&Method::POST, "/load_scenario") => {
            let msg = deserialize_body_or_respond!(req, LoadScenarioMsg);

            match server.load_scenario(&msg).await {
                Ok(json) => {
                    debug!(
                        "(/lib.handleRequest/load_scenario) Successfully (re)loaded scenario {}.",
                        &msg.scenario_name
                    );
                    let mut resp = Response::builder()
                        .status(StatusCode::OK)
                        .body(Bytes::copy_from_slice(json.as_bytes()).into())
                        .unwrap();
                    add_auth_headers(&mut resp, &web_server);
                    Ok(resp)
                }
                Err(err) => {
                    warn!(
                        "(/lib.handleRequest/load_scenario) Error loading scenario {}: {}",
                        msg.scenario_name, err
                    );
                    Ok(build_err_response(
                        StatusCode::BAD_REQUEST,
                        &err,
                        &web_server,
                    ))
                }
            }
        }

        (&Method::GET, "/quit") => {
            if !test_mode {
                warn!("Receiving a quit request in non-test mode.  Ignoring.");
            }
            info!("Received and processing quit request.");
            panic!("Time to exit");
        }

        (&Method::GET, "/entities") => {
            info!("Received and processing get request.");
            let json = server.get_entities_json();
            let mut resp = Response::builder()
                .status(StatusCode::OK)
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            add_auth_headers(&mut resp, &web_server);
            Ok(resp)
        }

        (&Method::GET, "/designs") => {
            info!("Received and processing get designs request.");
            let json = server.get_designs();
            let mut resp = Response::builder()
                .status(StatusCode::OK)
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            add_auth_headers(&mut resp, &web_server);
            Ok(resp)
        }

        (method, uri) => {
            info!("Unknown method {method} or URI {uri} on this request.  Returning 404.");
            // Return a 404 Not Found response for any other requests
            Ok(build_err_response(
                StatusCode::NOT_FOUND,
                "Not Found",
                &web_server,
            ))
        }
    }
}

/**
 * Read a file from the local filesystem or GCS.
 * Given this function returns all the content in the file, its not great for large files, but 100% okay
 * for config files and scenarios (as is our case).
 * General utility routine to be used in a few places.
 *
 * # Errors
 *
 * Will return `Err` if the file cannot be read or if GCS cannot be reached (depending on url of file)
 *
 * # Panics
 *
 * Will panic with a helpful message if GCS authentication fails.  GCS authentication needs to be handled outside (and prior to)
 * this function.
 */
pub async fn read_local_or_cloud_file(
    filename: &str,
) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Check if the filename is a GCS path
    if filename.starts_with("gs://") {
        // Extract bucket name from the GCS URI
        let parts: Vec<&str> = filename.split('/').collect();
        let bucket_name = parts[2];
        let object_name = parts[3..].join("/");

        // Create a GCS client
        let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
            panic!("Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?")
        });

        let client = Client::new(config);

        // Read the file from GCS
        let data = client
            .download_object(
                &GetObjectRequest {
                    bucket: bucket_name.to_string(),
                    object: object_name.to_string(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await?;
        Ok(data)
    } else {
        // Read the file locally
        let file = File::open(filename)?;
        let mut buf_reader = BufReader::new(file);
        let mut content: Vec<u8> = Vec::with_capacity(1024);
        buf_reader.read_to_end(&mut content)?;
        Ok(content)
    }
}

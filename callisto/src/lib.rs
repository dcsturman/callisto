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

use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;

use std::fs::File;
use std::io::{BufReader, Read};

use authentication::Authenticator;
use entity::Entities;
use payloads::{RequestMsg, ResponseMsg};
use server::Server;

pub const STATUS_INVALID_TOKEN: u16 = 498;

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
/// A [Vec<Response>] with a list of responses to send back to the client.  This way we avoid having the web socket or other network mechanism in this code.
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
    message: RequestMsg,
    entities: Arc<Mutex<Entities>>,
    test_mode: bool,
    authenticator: &mut Box<dyn Authenticator>,
) -> Result<Vec<ResponseMsg>, String> {
    info!("(handle_request) Request: {:?}", message);

    let error_msg = |err_msg: String| Ok(vec![ResponseMsg::Error(err_msg)]);
    let response_with_update = |result: Result<String, String>| -> Result<Vec<ResponseMsg>, String> {
        result.map_or_else(error_msg, |msg| Ok(vec![ResponseMsg::SimpleMsg(msg), ResponseMsg::EntityResponse(entities.clone().lock().unwrap().clone())]))
    };
    let simple_response = |result: Result<String, String>| -> Result<Vec<ResponseMsg>, String> {
        result.map_or_else(error_msg, |msg| Ok(vec![ResponseMsg::SimpleMsg(msg)]))
    };

    let mut server = Server::new(entities.clone(), authenticator, test_mode);

    // If the connection has not logged in yet, that is the priority.
    // Nothing else is processed until login is complete.
    if !server.validated_user()
        && !matches!(message, RequestMsg::Login(_))
        && !matches!(message, RequestMsg::Quit)
    {
        return Ok(vec![ResponseMsg::PleaseLogin]);
    }

    match message {
        RequestMsg::Login(login_msg) => {
            // But we put all this business logic into [Server.login](Server::login) rather than
            // split it up between the two locations.
            // Our role here is just to repackage the response and put it on the wire.
            server
                .login(login_msg)
                .await
                .map_or_else(error_msg, |auth_response| {
                    Ok(vec![ResponseMsg::AuthResponse(auth_response)])
                })
        }
        RequestMsg::AddShip(ship) => response_with_update(server.add_ship(ship)),
        RequestMsg::SetCrewActions(request) => response_with_update(server.set_crew_actions(&request)),
        RequestMsg::AddPlanet(planet) => response_with_update(server.add_planet(planet)),
        RequestMsg::Remove(name) => response_with_update(server.remove(&name)),
        RequestMsg::SetPlan(plan) => response_with_update(server.set_plan(&plan)),
        RequestMsg::Update(fire_actions) => {
            let effects = server.update(&fire_actions);
            Ok(vec![ResponseMsg::Effects(effects), ResponseMsg::EntityResponse(entities.clone().lock().unwrap().clone())])
        }
        RequestMsg::ComputePath(path_goal) => server
            .compute_path(&path_goal)
            .map_or_else(error_msg, |path| Ok(vec![ResponseMsg::FlightPath(path)])),
        RequestMsg::LoadScenario(scenario_name) => {
            simple_response(server.load_scenario(&scenario_name).await)
        }
        RequestMsg::Quit => {
            if !test_mode {
                warn!("Receiving a quit request in non-test mode.  Ignoring.");
            }
            info!("Received and processing quit request.");
            panic!("Time to exit");
        }
        RequestMsg::EntitiesRequest => {
            info!("Received and processing get request.");
            let json = server.get_entities();
            Ok(vec![ResponseMsg::EntityResponse(json)])
        }
        RequestMsg::DesignTemplateRequest => {
            info!("Received and processing get designs request.");
            let template_msg = server.get_designs();
            Ok(vec![ResponseMsg::DesignTemplateResponse(template_msg)])
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

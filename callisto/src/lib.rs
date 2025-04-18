pub mod action;
pub mod authentication;
pub mod combat;
mod computer;
pub mod crew;
pub mod entity;
pub mod missile;
pub mod payloads;
pub mod planet;
pub mod processor;
mod rules_tables;
pub mod server;
pub mod ship;

#[macro_use]
mod cov_util;

#[cfg(test)]
pub mod tests;

extern crate pretty_env_logger;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;

use std::fs::File;
use std::io::{BufReader, Read};

use payloads::{AuthResponse, RequestMsg, ResponseMsg, Role, UserData};
use server::Server;

pub const STATUS_INVALID_TOKEN: u16 = 498;

/// This the primary server dispatch when a message arrives.  It knows nothing about threading or even how messages are received or sent.
///
/// The function first checks if the user is logged in.  If not
/// then we just send back a request for a proper login message (using Google oauth2).  Most of the logic for these should be handled by [Server]
/// so that unit testing of the logic is possible.
///
/// # Arguments
///
/// * `message`: - Incoming message of type [`RequestMsg`]
/// * `server` - The [`Server`] struct presenting the server for this connection.
/// * `session_keys` - The session keys for all connections.  It is needed so the login flow can update it with proper login information.  This isn't local
///         to this connection per se because this could be a reconnect of a previous connection.
/// * `test_mode` - Whether we are in test mode.  Test mode disables authentication and ensures a deterministic seed for each random number generator.
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
// Note the lifetimes do seem to be needed and the implicit_hasher rule has impact across
// a lot of the codebase.  So excluding those two clippy warnings.
#[allow(clippy::too_many_lines, clippy::needless_lifetimes, clippy::implicit_hasher)]
pub async fn handle_request(
  message: RequestMsg, server: &mut Server, session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
  mut context: Vec<UserData>,
) -> Vec<ResponseMsg> {
  info!("(handle_request) Request: {:?}", message);

  // If the connection has not logged in yet, that is the priority.
  // Nothing else is processed until login is complete.
  if !server.validated_user() && !matches!(message, RequestMsg::Login(_)) && !matches!(message, RequestMsg::Quit) {
    return vec![ResponseMsg::PleaseLogin];
  }

  match message {
    RequestMsg::Login(login_msg) => {
      // But we put all this business logic into [Server.login](Server::login) rather than
      // split it up between the two locations.
      // Our role here is just to repackage the response and put it on the wire.
      server
        .login(login_msg, &session_keys)
        .await
        .map_or_else(error_msg, |auth_response| {
          // Add ourselves to the context as we weren't included when we weren't logged in.
          context.push(UserData {
            email: auth_response.email.clone(),
            role: Role::General,
            ship: None,
          });
          // Now that we are successfully logged in, we can send back the design templates, entities, and users - no need to wait to be asked.
          build_successful_auth_msgs(auth_response, server, context)
        })
    }

    RequestMsg::Reset => response_with_update(server, server.reset()),
    RequestMsg::AddShip(ship) => response_with_update(server, server.add_ship(ship)),
    RequestMsg::SetPilotActions(request) => response_with_update(server, server.set_pilot_actions(&request)),
    RequestMsg::AddPlanet(planet) => response_with_update(server, server.add_planet(planet)),
    RequestMsg::Remove(name) => response_with_update(server, server.remove(&name)),
    RequestMsg::SetPlan(plan) => response_with_update(server, server.set_plan(&plan)),
    RequestMsg::SetRole(role) => {
      if server.get_email().is_none() {
        error!("(handle_request) Attempt to set role without being logged in.  Ignoring.");
        vec![ResponseMsg::Error(
          "Attempt to set role without being logged in.  Ignoring.".to_string(),
        )]
      } else {
        let mut msgs = simple_response(Ok(server.set_role(&role)));
        let mut revised_context: Vec<UserData> = context
          .into_iter()
          .filter(|c| server.get_email().is_some_and(|email| c.email != email))
          .collect();
        revised_context.push(UserData {
          email: server.get_email().unwrap(),
          role: role.role,
          ship: role.ship,
        });
        msgs.push(ResponseMsg::Users(revised_context));
        msgs
      }
    }
    RequestMsg::ModifyActions(ship_actions) => {
      let effects = server.merge_actions(ship_actions);
      response_with_update(server, Ok(effects))
    }
    RequestMsg::Update => {
      let effects = server.update();
      vec![
        ResponseMsg::Effects(effects),
        ResponseMsg::EntityResponse(server.clone_entities()),
      ]
    }
    RequestMsg::ComputePath(path_goal) => server
      .compute_path(&path_goal)
      .map_or_else(error_msg, |path| vec![ResponseMsg::FlightPath(path)]),
    RequestMsg::LoadScenario(scenario_name) => simple_response(server.load_scenario(&scenario_name).await),
    RequestMsg::Logout => {
      info!("Received and processing logout request.");
      server.logout(&session_keys);
      vec![ResponseMsg::LogoutResponse, ResponseMsg::Users(context.clone())]
    }
    RequestMsg::Quit => {
      if !server.in_test_mode() {
        warn!("Receiving a quit request in non-test mode.  Ignoring.");
      }
      info!("Received and processing quit request.");
      panic!("Time to exit");
    }
    RequestMsg::EntitiesRequest => {
      info!("Received and processing get request.");
      let json = server.get_entities();
      vec![ResponseMsg::EntityResponse(json)]
    }
    RequestMsg::DesignTemplateRequest => {
      info!("Received and processing get designs request.");
      let template_msg = server.get_designs();
      vec![ResponseMsg::DesignTemplateResponse(template_msg)]
    }
  }
}

#[allow(clippy::unnecessary_wraps)]
fn error_msg(err_msg: String) -> Vec<ResponseMsg> {
  vec![ResponseMsg::Error(err_msg)]
}

fn response_with_update(server: &Server, result: Result<String, String>) -> Vec<ResponseMsg> {
  result.map_or_else(error_msg, |msg| {
    vec![
      ResponseMsg::SimpleMsg(msg),
      ResponseMsg::EntityResponse(server.clone_entities()),
    ]
  })
}

fn simple_response(result: Result<String, String>) -> Vec<ResponseMsg> {
  result.map_or_else(error_msg, |msg| vec![ResponseMsg::SimpleMsg(msg)])
}

/// Build the list of messages to send back to the client after a successful login.
/// This is used both when a user logs in and when a user reconnects.
///
/// # Arguments
/// * `auth_response` - The authentication response from the server.
/// * `server` - The server object.
/// * `session_keys` - The session keys for all connections.  This is a map of session keys to email addresses.  Used here when a user logs in (to update this info)
///
/// # Returns
/// A vector of messages to send back to the client.
///
/// # Panics
/// If the session keys cannot be locked.
#[allow(clippy::implicit_hasher)]
#[must_use]
pub fn build_successful_auth_msgs(
  auth_response: AuthResponse, server: &Server, context: Vec<UserData>,
) -> Vec<ResponseMsg> {
  vec![
    ResponseMsg::AuthResponse(auth_response),
    ResponseMsg::DesignTemplateResponse(server.get_designs()),
    ResponseMsg::EntityResponse(server.clone_entities()),
    ResponseMsg::Users(context),
  ]
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
pub async fn read_local_or_cloud_file(filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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

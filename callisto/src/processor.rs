use std::boxed::Box;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Receiver;
use futures::select;
use futures::{stream::FuturesUnordered, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Utf8Bytes;
use tokio_tungstenite::tungstenite::{
  error::{Error, ProtocolError},
  Message,
};
use tokio_tungstenite::WebSocketStream;

#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_rustls::server::TlsStream;

use dyn_clone::clone_box;

use crate::authentication::Authenticator;

use crate::entity::Entities;
use crate::payloads::{AuthResponse, RequestMsg, ResponseMsg};
use crate::player::PlayerManager;
use crate::server::{Server, ServerMembersTable};

#[cfg(feature = "no_tls_upgrade")]
type SubStream = TcpStream;
#[cfg(not(feature = "no_tls_upgrade"))]
type SubStream = TlsStream<TcpStream>;

#[allow(unused_imports)]
use crate::{debug, error, info, warn};

pub struct Processor {
  connection_receiver: Receiver<(WebSocketStream<SubStream>, String, Option<String>)>,
  auth_template: Box<dyn Authenticator>,
  session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
  servers: HashMap<String, Arc<Server>>,
  next_player_id: u64,
  members: ServerMembersTable,

  //TODO: Move into server.
  test_mode: bool,
}

struct Connection {
  /// player is an [`Arc`] because its [`Server`] will have a [`std::sync::Weak`] reference back to it.
  /// It is not otherwise shared.
  player: PlayerManager,
  stream: WebSocketStream<SubStream>,
}

impl Processor {
  #[must_use]
  pub fn new(
    connection_receiver: Receiver<(WebSocketStream<SubStream>, String, Option<String>)>,
    auth_template: Box<dyn Authenticator>, session_keys: Arc<Mutex<HashMap<String, Option<String>>>>, test_mode: bool,
  ) -> Self {
    Processor {
      connection_receiver,
      auth_template,
      session_keys,
      servers: HashMap::new(),
      next_player_id:0,
      members: ServerMembersTable::new(),
      test_mode,
    }
  }
  /// Polls all incoming connections and transmits any messages to
  /// the processing loop.
  ///
  /// The structure here is:
  /// * one thread in [main] that that accepts incoming connections.  It gives up ownership of the connection once established.
  /// * one thread for this [`connection_manager`] that then receives messages from all connections, processes them, and send replies.
  ///
  /// # Arguments
  /// * `entities` - The entities for the server.
  ///
  /// # Panics
  /// If we cannot properly serialize or deserialize a message on the stream.
  #[allow(clippy::implicit_hasher)]
  #[allow(clippy::too_many_lines)]
  pub async fn processor(&mut self, entities: Arc<Mutex<Entities>>) {
    // All the data shared between authenticators.
    let mut connections = Vec::<Connection>::new();

    let mut initial_scenario = Entities::new();
    entities.lock().unwrap().deep_copy_into(&mut initial_scenario);
    loop {
      // If there are no connections, then we wait for one to come in.
      // Special case as waiting on an empty FuturesUnordered will not wait - just returns None.
      // TODO: Violating DRY here in a big way.  How do I fix it?
      if connections.is_empty() {
        let next_connection = self.connection_receiver.next().await;

        if let Some((stream, session_key, email)) = next_connection {
          let Some(connection) = self
            .build_connection(email.as_ref(), &session_key, stream)
            .await
          else {
            continue;
          };
          connections.push(connection);
        } else {
          warn!("(processor) Connection receiver disconnected.  Exiting.");
          break;
        }
        continue;
      }

      // Create a stream that returns messages from all the connections, along with the index of the connection.
      let mut message_streams = connections
        .iter_mut()
        .enumerate()
        .map(|(i, c)| Box::pin(async move { (i, c.stream.next().await) }))
        .collect::<FuturesUnordered<_>>();
      // Wait on either a new connection or a message from an existing connection, whichever comes first.
      // Return the next message to process if there is one.
      let to_do = select! {
          next_connection = self.connection_receiver.next() => {
              if let Some((stream, session_key, email)) = next_connection {
              // Build the authenticator
              let Some(connection) = self.build_connection(
                  email.as_ref(),
                  &session_key,
                  stream
              ).await else {
                  continue;
              };
              drop(message_streams);
              connections.push(connection);
              debug!("(processor) Added new connection.  Total connections: {}", connections.len());
              continue;
          }
              // This is expected when the main thread exits.
              info!("(processor) Connection receiver disconnected.  Exiting.");
              break;
          },
          next_item =  message_streams.next() => {
              match next_item {
              Some((index, Some(next_msg))) => {
                  Some((index, next_msg))
              },
              Some((index, None)) => {
                  info!("(processor) Connection {index} disconnected.  Removing.");
                  drop(message_streams);
                  connections.remove(index);
                  continue;
              },
              None => {
                  warn!("(processor) Strange response from message stream.  Exiting.");
                  break;
              },
              }
          }
      };

      drop(message_streams);

      match to_do {
        Some((index, Ok(Message::Text(text)))) => {
          debug!("(handle_connection) Received message: {text}");
          let response = match serde_json::from_str(&text) {
            Ok(parsed_message) => {
              let response = self.handle_request(parsed_message, &mut connections[index].player).await;
              // This is a bit of a hack. We use `LogoutResponse` to signal that we should close the connection.
              // but do not actually ever send it to the client (who has logged out!)
              if response.iter().filter(|msg| matches!(msg, ResponseMsg::LogoutResponse)).count() > 0 {
                // User has logged out.  Close the connection.
                debug!(
                  "(processor) User logged out.  Closing connection. Now {} connections.",
                  connections.len() - 1
                );
                connections[index].stream.close(None).await.unwrap_or_else(|e| {
                  error!("(handle_connection) Failed to close connection as directed by logout: {e:?}");
                });
              }
              response
                .into_iter()
                .filter(|msg| !matches!(msg, ResponseMsg::LogoutResponse))
                .collect()
            }
            Err(e) => {
              error!("(handle_connection) Failed to parse message: {e:?}");
              vec![ResponseMsg::Error(format!("Failed to parse message: {e:?}"))]
            }
          };
          debug!("(handle_connection) Response(s): {response:?}");

          // Send the response
          for message in response {
            let encoded_message: Utf8Bytes = serde_json::to_string(&message).unwrap().into();
            if is_broadcast_message(&message) {
              debug!(
                "(processor) Broadcast message {message:?} to {} connections.",
                connections.len()
              );
              for connection in &mut connections {
                connection
                  .stream
                  .send(Message::Text(encoded_message.clone()))
                  .await
                  .unwrap_or_else(|e| {
                    error!("(handle_connection) Failed to send broadcast response: {e:?}");
                  });
              }
            } else {
              debug!("(processor) Sending message {message:?} to connection {index}.");
              connections[index]
                .stream
                .send(Message::Text(
                  serde_json::to_string(&message).expect("Failed to serialize response").into(),
                ))
                .await
                .unwrap_or_else(|e| {
                  error!("(handle_connection) Failed to send response: {e:?}");
                });
            }
          }
        }
        Some((index, Ok(Message::Close(_)))) => {
          // Close the connection
          connections[index].stream.close(None).await.unwrap_or_else(|e| {
            if let Error::Protocol(ProtocolError::SendAfterClosing) = e {
              // This is expected when we try to close a connection that is already closed.
              debug!("(processor) Attempted to close a connection that was already closed.  Ignoring.");
            } else {
              error!("(handle_connection) Failed to close connection: {e:?}");
            }
          });
          // Mark this stream for deletion
          connections.remove(index);
          debug!("(processor) Removed connection.  Now {} connections.", connections.len());
        }
        Some((index, res)) => {
          error!("(processor) Unexpected message on connection {index}: {res:?}");
        }
        None => {
          warn!("(processor) Strange `None` response from message stream.  Ignoring");
        }
      }
    }
  }

  #[allow(clippy::borrowed_box)]
  #[allow(clippy::too_many_arguments)]
  #[must_use]
  async fn build_connection(
    &mut self, email: Option<&String>, session_key: &str, stream: WebSocketStream<SubStream>,
  ) -> Option<Connection> {
    let mut authenticator = clone_box(self.auth_template.as_ref());
    authenticator.set_session_key(session_key);
    authenticator.set_email(email);
    let mut connection = Connection {
      player: PlayerManager::new(self.next_player_id, None, authenticator, self.test_mode),
      stream,
    };
    self.next_player_id += 1;

    if let Some(email) = email {
      // If we got a successful Some(email) then we need to fake like this was a log in by
      // letting the client know auth was successful, but also sending any initialization messages.
      // We use [build_successful_auth_msgs] to keep this list of messages the same as if it was in response
      // to a login message.
      let msgs = build_successful_auth_msgs(AuthResponse { email: email.clone() });
      let mut okay = true;
      for msg in msgs {
        let encoded_message: Utf8Bytes = serde_json::to_string(&msg).unwrap().into();
        if connection.stream.send(Message::Text(encoded_message)).await.is_err() {
          okay = false;
          break;
        }
      }
      if okay {
        Some(connection)
      } else {
        warn!("(processor) Failed to send AuthResponse to new connection. Assuming its bad and dropping it.");
        None
      }
    } else {
      Some(connection)
    }
  }

  /// This the primary server dispatch when a message arrives.  It knows nothing about threading or even how messages are received or sent.
  ///
  /// The function first checks if the user is logged in.  If not
  /// then we just send back a request for a proper login message (using Google oauth2).  Most of the logic for these should be handled by [`PlayerManager`]
  /// so that unit testing of the logic is possible.
  ///
  /// # Arguments
  ///
  /// * `message`: - Incoming message of type [`RequestMsg`]
  /// * `player` - The [`PlayerManager`] struct presenting the server for this connection.
  /// * `context` - The context of all other users.  This is needed so we can send back the list of users to the client.
  /// * `session_keys` - The session keys for all connections.  It is needed so the login flow can update it with proper login information.  This isn't local
  ///   to this connection per se because this could be a reconnect of a previous connection.
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
  /// Will panic as a quick exit on a QUIT message.  Only possible when in test mode.  Also will panic whenever
  /// it tries to get its lock on its player record and cannot.
  ///
  // Note the lifetimes do seem to be needed and the implicit_hasher rule has impact across
  // a lot of the codebase.  So excluding those two clippy warnings.
  #[allow(clippy::too_many_lines, clippy::needless_lifetimes, clippy::implicit_hasher)]
  pub async fn handle_request(&mut self, message: RequestMsg, player: &mut PlayerManager) -> Vec<ResponseMsg> {
    info!("(handle_request) Request: {:?}", message);

    // If the connection has not logged in yet, that is the priority.
    // Nothing else is processed until login is complete.
    if !player.validated_user() && !matches!(message, RequestMsg::Login(_)) && !matches!(message, RequestMsg::Quit) {
      return vec![ResponseMsg::PleaseLogin];
    }

    match message {
      RequestMsg::Login(login_msg) => {
        // But we put all this business logic into [PlayerManager.login](PlayerManager::login) rather than
        // split it up between the two locations.
        // Our role here is just to repackage the response and put it on the wire.
        player
          .login(login_msg, &self.session_keys)
          .await
          .map_or_else(error_msg, |auth_response| {
            // Now that we are successfully logged in, we can send back the design templates, entities, and users - no need to wait to be asked.
            build_successful_auth_msgs(auth_response)
          })
      }

      RequestMsg::Reset => response_with_update(player, player.reset()),
      RequestMsg::AddShip(ship) => response_with_update(player, player.add_ship(ship)),
      RequestMsg::SetPilotActions(request) => {
        response_with_update(player, player.set_pilot_actions(&request))
      }
      RequestMsg::AddPlanet(planet) => response_with_update(player, player.add_planet(planet)),
      RequestMsg::Remove(name) => response_with_update(player, player.remove(&name)),
      RequestMsg::SetPlan(plan) => response_with_update(player, player.set_plan(&plan)),
      RequestMsg::SetRole(role) => {
        if player.get_email().is_none() {
          error!("(handle_request) Attempt to set role without being logged in.  Ignoring.");
          vec![ResponseMsg::Error(
            "Attempt to set role without being logged in.  Ignoring.".to_string(),
          )]
        } else {
          let mut msgs = simple_response(Ok(player.set_role(&role)));
          msgs.append(&mut player.server.as_ref().map_or_else(
            || error_msg("Cannot set role when no server has yet been joined.".to_string()),
            |server| {
              self.members.update(server.get_id(), player.get_id(), &player.get_email().unwrap(), role.role, role.ship);
              vec![ResponseMsg::Users(self.members.get_user_context(server.get_id()))]
            },
          ));
          msgs
        }
      }
      RequestMsg::ModifyActions(ship_actions) => {
        let effects = player.merge_actions(ship_actions);
        response_with_update(player, Ok(effects))
      }
      RequestMsg::Update => {
        let effects = player.update();
        vec![
          ResponseMsg::Effects(effects),
          ResponseMsg::EntityResponse(player.clone_entities()),
        ]
      }
      RequestMsg::ComputePath(path_goal) => player
        .compute_path(&path_goal)
        .map_or_else(error_msg, |path| vec![ResponseMsg::FlightPath(path)]),
      RequestMsg::LoadScenario(scenario_name) => simple_response(player.load_scenario(&scenario_name).await),
      RequestMsg::Logout => {
        info!("Received and processing logout request.");
        player.logout(&self.session_keys);
        let Some(ref server) = player.server else {
          error!("(handle_request) Attempt to logout without being in a scenario.  Ignoring.");
          return vec![ResponseMsg::Error("Attempt to logout without being in a scenario.  Ignoring.".to_string())];
        };

        self.members.remove(server.get_id(), player.get_id());
        vec![
          ResponseMsg::LogoutResponse,
          ResponseMsg::Users(self.members.get_user_context(server.get_id())),
        ]
      }
      RequestMsg::JoinScenario(join_scenario) => {
        if let Some(server) = self.servers.get(&join_scenario.scenario_name) {
          player.set_server(server.clone());
          self.members.update(server.get_id(), player.get_id(), &player.get_email().unwrap(), player.get_role().0, player.get_role().1);
          vec![ResponseMsg::JoinedScenario(join_scenario.scenario_name),
          ResponseMsg::EntityResponse(server.get_unlocked_entities().unwrap().clone()),
          ResponseMsg::Users(self.members.get_user_context(server.get_id()))]
        } else {
          vec![ResponseMsg::Error("Scenario does not exist.".to_string())]
        }
      }
      RequestMsg::CreateScenario(create_scenario) => {
        if self.servers.contains_key(&create_scenario.name) {
          return vec![ResponseMsg::Error("Scenario name already exists.".to_string())];
        }
        // Create the new server, register it in the servers tables, in the membership table, and with the player structure. 
        let server = Arc::new(Server::new(&create_scenario.name, &create_scenario.scenario).await);
        self.servers.insert(create_scenario.name.clone(), server.clone());
        self.members.update(server.get_id(), player.get_id(), &player.get_email().unwrap(), player.get_role().0, player.get_role().1);
        player.set_server(server.clone());

        // Get a clone of entities as we need update the user.
        let entities = server.get_unlocked_entities().unwrap().clone();
        
        vec![
          ResponseMsg::JoinedScenario(create_scenario.name),
          ResponseMsg::EntityResponse(entities),
          ResponseMsg::Users(self.members.get_user_context(server.get_id())),
        ]
      }
      RequestMsg::Quit => {
        if !player.in_test_mode() {
          warn!("Receiving a quit request in non-test mode.  Ignoring.");
        }
        info!("Received and processing quit request.");
        panic!("Time to exit");
      }
      RequestMsg::EntitiesRequest => {
        info!("Received and processing get request.");
        let json = player.get_entities();
        vec![ResponseMsg::EntityResponse(json)]
      }
      RequestMsg::DesignTemplateRequest => {
        info!("Received and processing get designs request.");
        vec![ResponseMsg::DesignTemplateResponse(PlayerManager::get_designs())]
      }
    }
  }
}

// Utility functions to help build messages etc.

fn is_broadcast_message(message: &ResponseMsg) -> bool {
  matches!(message, ResponseMsg::EntityResponse(_)) || matches!(message, ResponseMsg::Users(_))
}

#[allow(clippy::unnecessary_wraps)]
fn error_msg(err_msg: String) -> Vec<ResponseMsg> {
  vec![ResponseMsg::Error(err_msg)]
}

fn response_with_update(server: &PlayerManager, result: Result<String, String>) -> Vec<ResponseMsg> {
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
pub fn build_successful_auth_msgs(auth_response: AuthResponse) -> Vec<ResponseMsg> {
  vec![
    ResponseMsg::AuthResponse(auth_response),
    ResponseMsg::Scenarios(crate::SCENARIOS.get().unwrap().clone()),
    ResponseMsg::DesignTemplateResponse(PlayerManager::get_designs()),
  ]
}

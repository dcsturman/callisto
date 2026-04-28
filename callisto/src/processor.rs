use std::boxed::Box;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::{Receiver, UnboundedReceiver};
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

use crate::entity::MetaData;
use crate::payloads::{AuthResponse, RequestMsg, ResponseMsg, SaveScenarioMsg, ScenariosMsg};
use crate::player::PlayerManager;
use crate::server::{Server, ServerMembersTable};
use crate::{get_scenarios_snapshot, read_local_or_cloud_file, replace_scenarios, write_local_or_cloud_file};
use crate::{LOGOUT, LOG_FILE_USE};

#[cfg(feature = "no_tls_upgrade")]
type SubStream = TcpStream;
#[cfg(not(feature = "no_tls_upgrade"))]
type SubStream = TlsStream<TcpStream>;

#[allow(unused_imports)]
use crate::{debug, error, info, warn, LOG_SCENARIO_ACTIVITY};
use tracing::{event, Level};

pub struct Processor {
  connection_receiver: Receiver<(WebSocketStream<SubStream>, String, Option<String>)>,
  reload_receiver: UnboundedReceiver<ReloadNotification>,
  auth_template: Box<dyn Authenticator>,
  session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
  servers: HashMap<String, Arc<Server>>,
  members: ServerMembersTable,

  // Unchanging value with directory for all scenarios.
  scenario_dir: String,

  // Test mode is here as its an aspect of the entire server (mostly how we authenticate)
  // not a particular scenario.
  test_mode: bool,
  reload_notifications_enabled: bool,
}

struct Connection {
  /// player is an [`Arc`] because its [`Server`] will have a [`std::sync::Weak`] reference back to it.
  /// It is not otherwise shared.
  player: PlayerManager,
  stream: WebSocketStream<SubStream>,
}

#[derive(Debug, Clone, Copy)]
pub enum ReloadNotification {
  Scenarios,
  ShipTemplates,
}

type IncomingConnection = (WebSocketStream<SubStream>, String, Option<String>);

enum IdleProcessorEvent {
  Connection(Option<Box<IncomingConnection>>),
  Reload(Option<ReloadNotification>),
}

enum ActiveProcessorEvent {
  Connection(Option<Box<IncomingConnection>>),
  Reload(Option<ReloadNotification>),
  Message(Option<(usize, Option<Result<Message, Error>>)>),
}

impl Processor {
  #[must_use]
  pub fn new(
    connection_receiver: Receiver<(WebSocketStream<SubStream>, String, Option<String>)>,
    reload_receiver: UnboundedReceiver<ReloadNotification>, auth_template: Box<dyn Authenticator>,
    session_keys: Arc<Mutex<HashMap<String, Option<String>>>>, scenario_dir: &str, test_mode: bool,
  ) -> Self {
    // Clean up scenario_dir so that it does not have a trailing slash.
    let scenario_dir = scenario_dir.trim_end_matches('/').to_string();

    Processor {
      connection_receiver,
      reload_receiver,
      auth_template,
      session_keys,
      servers: HashMap::new(),
      members: ServerMembersTable::new(),
      scenario_dir,
      test_mode,
      reload_notifications_enabled: true,
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
  pub async fn processor(&mut self) {
    // All the data shared between authenticators.
    let mut connections = Vec::<Connection>::new();

    loop {
      // In here, clean up old scenarios that haven't had anyone in them for 5 minutes.
      let removed_scenario = self.members.clean_expired_scenarios();

      // If there are no connections, then we wait for one to come in.
      // Special case as waiting on an empty FuturesUnordered will not wait - just returns None.
      // TODO: Violating DRY here in a big way.  How do I fix it?
      if connections.is_empty() {
        let next_event = if self.reload_notifications_enabled {
          let connection_receiver = &mut self.connection_receiver;
          let reload_receiver = &mut self.reload_receiver;
          select! {
            next_connection = connection_receiver.next() => IdleProcessorEvent::Connection(next_connection.map(Box::new)),
            next_reload = reload_receiver.next() => IdleProcessorEvent::Reload(next_reload),
          }
        } else {
          IdleProcessorEvent::Connection(self.connection_receiver.next().await.map(Box::new))
        };

        match next_event {
          IdleProcessorEvent::Connection(Some(connection)) => {
            let (stream, session_key, email) = *connection;
            let Some(connection) = self.build_connection(email.as_ref(), &session_key, stream).await else {
              continue;
            };
            connections.push(connection);
          }
          IdleProcessorEvent::Connection(None) => {
            warn!("(processor) Connection receiver disconnected.  Exiting.");
            break;
          }
          IdleProcessorEvent::Reload(Some(notification)) => {
            self.handle_reload_notification(&mut connections, notification).await;
          }
          IdleProcessorEvent::Reload(None) => {
            warn!("(processor) Reload notification channel disconnected. Continuing without live reload pushes.");
            self.reload_notifications_enabled = false;
          }
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
      let to_do = if self.reload_notifications_enabled {
        let connection_receiver = &mut self.connection_receiver;
        let reload_receiver = &mut self.reload_receiver;
        select! {
          next_connection = connection_receiver.next() => ActiveProcessorEvent::Connection(next_connection.map(Box::new)),
          next_reload = reload_receiver.next() => ActiveProcessorEvent::Reload(next_reload),
          next_item =  message_streams.next() => ActiveProcessorEvent::Message(next_item),
        }
      } else {
        select! {
          next_connection = self.connection_receiver.next() => ActiveProcessorEvent::Connection(next_connection.map(Box::new)),
          next_item =  message_streams.next() => ActiveProcessorEvent::Message(next_item),
        }
      };

      drop(message_streams);

      match to_do {
        ActiveProcessorEvent::Connection(Some(connection)) => {
          let (stream, session_key, email) = *connection;
          let Some(connection) = self.build_connection(email.as_ref(), &session_key, stream).await else {
            continue;
          };
          connections.push(connection);
          debug!("(processor) Added new connection.  Total connections: {}", connections.len());
        }
        ActiveProcessorEvent::Connection(None) => {
          // This is expected when the main thread exits.
          warn!("(processor) Connection receiver disconnected.  This is okay if the server is shutting down.Exiting.");
          break;
        }
        ActiveProcessorEvent::Reload(Some(notification)) => {
          self.handle_reload_notification(&mut connections, notification).await;
        }
        ActiveProcessorEvent::Reload(None) => {
          warn!("(processor) Reload notification channel disconnected. Continuing without live reload pushes.");
          self.reload_notifications_enabled = false;
        }
        ActiveProcessorEvent::Message(Some((index, Some(Ok(Message::Text(text)))))) => {
          debug!("(handle_connection) Received message: {text}");

          // Process the message and return a list of response messages. Also return the server for this player when complete.
          // The returned server is used to limit broadcast messages to only those in the server.
          let (mut response, incoming_server) = match serde_json::from_str::<RequestMsg>(&text) {
            Ok(parsed_message) => {
              // Grab this here as the ordering of the connections vector may change while we yield the thread!
              let num_connections = connections.len();
              let current_connection = &mut connections[index];

              let response = self.handle_request(parsed_message, &mut current_connection.player).await;
              // This is a bit of a hack. We use `LogoutResponse` to signal that we should close the connection.
              // but do not actually ever send it to the client (who has logged out!)
              if response.iter().filter(|msg| matches!(msg, ResponseMsg::LogoutResponse)).count() > 0 {
                // User has logged out.  Close the connection.
                event!(
                  target: LOGOUT,
                  Level::INFO,
                  email = current_connection.player.get_email().unwrap_or("UNKNOWN USER".to_string()),
                  action = format!("User intentionally logged out. Now {} connections.", num_connections - 1)
                );
                current_connection.stream.close(None).await.unwrap_or_else(|e| {
                  error!("(handle_connection) Failed to close connection as directed by logout: {e:?}");
                });
              }
              (
                response
                  .into_iter()
                  .filter(|msg| !matches!(msg, ResponseMsg::LogoutResponse))
                  .collect(),
                // Its important to get the server here at the end as it might have changed during processing of the message
                // (e.g. with CreateScenario or JoinScenario)
                current_connection.player.server.clone(),
              )
            }
            Err(e) => {
              error!("(handle_connection) Failed to parse message: {e:?}");
              (vec![ResponseMsg::Error(format!("Failed to parse message: {e:?}"))], None)
            }
          };
          debug!("(handle_connection) Response(s): {response:?}");

          // If we removed a scenario at the start of this loop, then add a Scenarios message to the response.
          // However, we do not need or want to send two so if we already have one, we don't add another.
          if removed_scenario && !response.iter().any(|msg| matches!(msg, ResponseMsg::Scenarios(_))) {
            response.push(ResponseMsg::Scenarios(self.build_scenarios_msg()));
          }

          // Send the response
          for message in response {
            let encoded_message: Utf8Bytes = serde_json::to_string(&message).unwrap().into();
            if is_broadcast_message(&message) {
              debug!(
                "(processor) Broadcast message {message:?} to {} connections.",
                connections.len()
              );
              for connection in &mut connections {
                // For most messages, broadcast only to those in the same server.
                // The exception is sending the Scenarios list so that everyone has that.
                if connection.player.server == incoming_server || matches!(message, ResponseMsg::Scenarios(_)) {
                  connection
                    .stream
                    .send(Message::Text(encoded_message.clone()))
                    .await
                    .unwrap_or_else(|e| {
                      error!("(handle_connection) Failed to send broadcast response: {e:?}");
                    });
                }
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
        ActiveProcessorEvent::Message(Some((index, Some(Ok(Message::Close(_)))))) => {
          // Close the connection
          event!(
            target: LOGOUT,
            Level::INFO,
            email = connections[index].player.get_email().unwrap_or("(Unauthenticated users)".to_string()),
            action = format!("Connection closed ungracefully. Now {} connections.", connections.len() - 1)
          );
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
        ActiveProcessorEvent::Message(Some((index, Some(res)))) => {
          error!("(processor) Unexpected message on connection {index}: {res:?}");
        }
        ActiveProcessorEvent::Message(Some((index, None))) => {
          info!("(processor) Connection {index} disconnected.  Removing.");
          connections.remove(index);
        }
        ActiveProcessorEvent::Message(None) => {
          warn!("(processor) Strange response from message stream.  Exiting.");
          break;
        }
      }
    }
  }

  async fn handle_reload_notification(&self, connections: &mut [Connection], notification: ReloadNotification) {
    match notification {
      ReloadNotification::Scenarios => {
        let scenarios_message = ResponseMsg::Scenarios(self.build_scenarios_msg());
        debug!(
          "(processor) Broadcasting live scenario refresh to {} authenticated connections.",
          connections
            .iter()
            .filter(|connection| connection.player.validated_user())
            .count()
        );
        for connection in connections.iter_mut().filter(|connection| connection.player.validated_user()) {
          send_response(&mut connection.stream, &scenarios_message, "live scenario refresh").await;
        }
      }
      ReloadNotification::ShipTemplates => {
        debug!(
          "(processor) Broadcasting live design refresh to {} authenticated connections.",
          connections
            .iter()
            .filter(|connection| connection.player.validated_user())
            .count()
        );
        for connection in connections.iter_mut().filter(|connection| connection.player.validated_user()) {
          let design_message = ResponseMsg::DesignTemplateResponse(connection.player.get_designs());
          send_response(&mut connection.stream, &design_message, "live design refresh").await;
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
      player: PlayerManager::new(None, authenticator, self.test_mode),
      stream,
    };

    // If we got a successful Some(email) then we need to fake like this was a log in by
    // letting the client know auth was successful, but also setting this player up with the old player state
    // and by sending any initialization messages.
    // We use [build_successful_auth_msgs] to keep this list of messages the same as if it was in response
    // to a login message.  In the case though of having been previously logged in and part of a scenario we need to
    // include the scenario state in the Auth message AND send an Entities message.
    if let Some(email) = email {
      if let Some((old_server, old_email, old_role, old_ship)) =
        self.members.find_scenario_info_by_session_key(session_key)
      {
        if old_email != *email {
          error!("(build_connection) Found a session key for {old_email} but got a login for {old_email} instead of {email}.  This should not happen.  Ignoring.");
          return None;
        }
        if let Some(server) = self.servers.get(&old_server) {
          connection.player.set_server(server.clone());
          connection.player.set_role_ship(old_role, old_ship);
        }
      }

      let (role, ship) = connection.player.get_role();

      let mut msgs = self.build_successful_auth_msgs(
        &connection.player,
        AuthResponse {
          email: email.clone(),
          scenario: connection.player.server.as_ref().map(|s| s.id.clone()),
          role: Some(role),
          ship,
        },
      );

      // Now add messages just as if we had joined this scenario.
      msgs.append(&mut self.build_post_join_msgs(&connection.player));

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
      // Unauthenticated connection - just accept it
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
    event!(Level::INFO, request = Into::<&str>::into(&message), contents = ?message);

    // If the connection has not logged in yet, that is the priority.
    // Nothing else is processed until login is complete.
    if !player.validated_user()
      && !matches!(message, RequestMsg::Login(_))
      && !matches!(message, RequestMsg::Quit)
      && !matches!(message, RequestMsg::ValidateSession)
    {
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
            self.build_successful_auth_msgs(player, auth_response)
          })
      }

      RequestMsg::Reset => response_with_update(player, player.reset()),
      RequestMsg::AddShip(ship) => response_with_update(player, player.add_ship(ship)),
      RequestMsg::SetPilotActions(request) => response_with_update(player, player.set_pilot_actions(&request)),
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
              let Some(session_key) = player.get_session_key() else {
                error!("(handle_request) Attempt to set role without a session key.  Ignoring.");
                return error_msg("Cannot set role without a session key.".to_string());
              };
              self.members.update(
                server.get_id(),
                &session_key,
                &player.get_email().unwrap(),
                role.role,
                role.ship,
              );
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
      RequestMsg::CaptainAction(msg) => {
        let result = player.captain_action(&msg);
        vec![
          ResponseMsg::CaptainActionResult(result),
          ResponseMsg::EntityResponse(player.clone_entities()),
        ]
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
      RequestMsg::Exit => {
        info!("Received and processing Exit request.");
        let mut old_server = None;
        // Do the swap so that we can use the old server information to properly clean up
        // while at the same time having the player.server value be None going forward.
        std::mem::swap(&mut old_server, &mut player.server);
        let Some(ref server) = &old_server else {
          error!("(handle_request) Attempt to exit scenario  without being in a scenario.  Ignoring.");
          return vec![ResponseMsg::Error(
            "Attempt to logout without being in a scenario.  Ignoring.".to_string(),
          )];
        };
        let server_id = server.get_id();
        let Some(session_key) = player.get_session_key() else {
          error!("(handle_request) Attempt to exit scenario without a session key.  Ignoring.");
          return vec![ResponseMsg::Error(
            "Attempt to exit scenario without a session key.  Ignoring.".to_string(),
          )];
        };
        self.members.remove(server_id, &session_key);
        event!(
          target: LOG_SCENARIO_ACTIVITY,
          Level::INFO,
          email = player.get_email().unwrap(),
          scenario = server_id,
          action = "exit"
        );
        player.set_role_ship(crate::payloads::Role::General, None);

        vec![ResponseMsg::Users(self.members.get_user_context(server_id))]
      }
      RequestMsg::Logout => {
        info!("Received and processing logout request.");
        player.logout(&self.session_keys);

        vec![ResponseMsg::LogoutResponse]
      }
      RequestMsg::ValidateSession => {
        // Check if the current session is valid by looking up the session key
        if let Some(session_key) = player.get_session_key() {
          let session_keys_lock = self.session_keys.lock().unwrap();
          if let Some(Some(email)) = session_keys_lock.get(&session_key) {
            // Session is valid and user is logged in
            debug!("(ValidateSession) Session key is valid, user email: {}", email);
            let email_clone = email.clone();
            drop(session_keys_lock);
            let (role, ship) = player.get_role();
            vec![ResponseMsg::AuthResponse(AuthResponse {
              email: email_clone,
              scenario: player.server.as_ref().map(|s| s.id.clone()),
              role: Some(role),
              ship,
            })]
          } else {
            // Session key exists but no email (not logged in yet)
            debug!("(ValidateSession) Session key exists but no email found in map");
            drop(session_keys_lock);
            vec![ResponseMsg::PleaseLogin]
          }
        } else {
          // No session key found
          debug!("(ValidateSession) No session key found on player");
          vec![ResponseMsg::PleaseLogin]
        }
      }
      RequestMsg::JoinScenario(join_scenario) => {
        if let Some(server) = self.servers.get(&join_scenario.scenario_name) {
          player.set_server(server.clone());
          event!(
            target: LOG_SCENARIO_ACTIVITY,
            Level::INFO,
            email = player.get_email().unwrap(),
            scenario = join_scenario.scenario_name,
            action = "join"
          );
          let Some(session_key) = player.get_session_key() else {
            error!("(handle_request) Attempt to join scenario without a session key.  Ignoring.");
            return vec![ResponseMsg::Error(
              "Attempt to join scenario without a session key.  Ignoring.".to_string(),
            )];
          };
          self.members.update(
            server.get_id(),
            &session_key,
            &player.get_email().unwrap(),
            player.get_role().0,
            player.get_role().1,
          );
          let mut msgs = vec![ResponseMsg::JoinedScenario(join_scenario.scenario_name)];
          msgs.append(&mut self.build_post_join_msgs(player));
          msgs
        } else {
          vec![ResponseMsg::Error("Scenario does not exist.".to_string())]
        }
      }
      RequestMsg::CreateScenario(create_scenario) => {
        debug!("(Processor.handle_request) Creating scenario {}", create_scenario.name);
        if self.servers.contains_key(&create_scenario.name) {
          return vec![ResponseMsg::Error("Scenario name already exists.".to_string())];
        }

        let scenario_full_name = if create_scenario.scenario.is_empty() {
          String::new()
        } else {
          format!("{}/{}", self.scenario_dir, create_scenario.scenario)
        };
        debug!(
          "(Processor.handle_request) Creating scenario {} from {scenario_full_name}",
          create_scenario.name
        );
        // Create the new server, register it in the servers tables, in the membership table, and with the player structure.
        let server = Arc::new(Server::new(&create_scenario.name, &scenario_full_name).await);
        self.servers.insert(create_scenario.name.clone(), server.clone());
        event!(
          target: LOG_SCENARIO_ACTIVITY,
          Level::INFO,
          email = player.get_email().unwrap(),
          scenario = create_scenario.name,
          action = "create"
        );
        // TODO: Violating DRY here.
        self.members.register(&create_scenario.name, &create_scenario.scenario);
        let Some(session_key) = player.get_session_key() else {
          error!("(handle_request) Attempt to create scenario without a session key.  Ignoring.");
          return vec![ResponseMsg::Error(
            "Attempt to create scenario without a session key.  Ignoring.".to_string(),
          )];
        };
        self.members.update(
          server.get_id(),
          &session_key,
          &player.get_email().unwrap(),
          player.get_role().0,
          player.get_role().1,
        );
        event!(
          target: LOG_SCENARIO_ACTIVITY,
          Level::INFO,
          email = player.get_email().unwrap(),
          scenario = create_scenario.name,
          action = "join"
        );
        player.set_server(server.clone());

        // Get a clone of entities as we need update the user.
        let entities = server.get_unlocked_entities().unwrap().clone();

        vec![
          ResponseMsg::JoinedScenario(create_scenario.name),
          ResponseMsg::Scenarios(self.build_scenarios_msg()),
          ResponseMsg::EntityResponse(entities),
          ResponseMsg::Users(self.members.get_user_context(server.get_id())),
        ]
      }
      RequestMsg::SaveScenario(save_msg) => self.handle_save_scenario(player, save_msg).await,
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
        vec![ResponseMsg::DesignTemplateResponse(player.get_designs())]
      }
      RequestMsg::Ping => vec![ResponseMsg::Pong],
    }
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
  pub fn build_successful_auth_msgs(&self, player: &PlayerManager, auth_response: AuthResponse) -> Vec<ResponseMsg> {
    vec![
      ResponseMsg::AuthResponse(auth_response),
      ResponseMsg::Scenarios(self.build_scenarios_msg()),
      ResponseMsg::DesignTemplateResponse(player.get_designs()),
    ]
  }

  /// Build the list of scenarios and scenario templates to send back to the client.
  ///
  /// # Panics
  /// Panics if the scenarios list hasn't been initialized.
  #[must_use]
  pub fn build_scenarios_msg(&self) -> ScenariosMsg {
    ScenariosMsg {
      current_scenarios: self.members.current_scenario_list(),
      templates: get_scenarios_snapshot().as_ref().clone(),
    }
  }

  /// Build the list of messages to send back to the client after a successful join of a scenario.
  ///
  /// # Panics
  /// Panics if the server entities cannot be unlocked.
  #[must_use]
  pub fn build_post_join_msgs(&self, player: &PlayerManager) -> Vec<ResponseMsg> {
    if let Some(server) = &player.server {
      vec![
        ResponseMsg::EntityResponse(server.get_unlocked_entities().unwrap().clone()),
        ResponseMsg::Users(self.members.get_user_context(server.get_id())),
      ]
    } else {
      vec![]
    }
  }

  /// Persist the player's currently-loaded scenario to the configured scenario
  /// directory (local FS or `gs://...`). Performs name sanitization, ownership
  /// enforcement, and force-overwrite handshake. On success, refreshes the
  /// global scenarios snapshot and broadcasts an updated `Scenarios` message.
  async fn handle_save_scenario(&self, player: &PlayerManager, save_msg: SaveScenarioMsg) -> Vec<ResponseMsg> {
    // Local helper — only used to read the owner out of an existing scenario file
    // without needing the ship-template context that full Entities deserialization
    // requires. Declared up front so it doesn't sit between statements.
    #[derive(serde::Deserialize)]
    struct ExistingMeta {
      #[serde(default)]
      metadata: MetaData,
    }

    let Some(user_email) = player.get_email() else {
      return vec![ResponseMsg::Error(
        "Must be authenticated to save a scenario.".to_string(),
      )];
    };
    let Some(server) = player.server.clone() else {
      return vec![ResponseMsg::Error("Not in a scenario; nothing to save.".to_string())];
    };

    // Sanitize the requested name. Reject anything that could escape the scenario
    // directory or hit a hidden/reserved file. Auto-append `.json` when missing
    // so the picker (which loads `*.json`) sees the new entry.
    let raw = save_msg.name.trim();
    if raw.is_empty() {
      return vec![ResponseMsg::Error("Scenario name cannot be empty.".to_string())];
    }
    if raw.contains('/') || raw.contains('\\') || raw.contains("..") || raw.starts_with('.') {
      return vec![ResponseMsg::Error("Invalid scenario name.".to_string())];
    }
    let has_json_ext = std::path::Path::new(raw)
      .extension()
      .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));
    let file_name = if has_json_ext {
      raw.to_string()
    } else {
      format!("{raw}.json")
    };
    let file_stem = file_name.trim_end_matches(".json").to_string();
    let full_path = format!("{}/{}", self.scenario_dir, file_name);

    if let Ok(existing_bytes) = read_local_or_cloud_file(&full_path).await {
      if let Ok(existing) = serde_json::from_slice::<ExistingMeta>(&existing_bytes) {
        let existing_owner = existing.metadata.owner;
        // Empty owner = pre-ownership scenario; treat as orphaned and allow takeover.
        if !existing_owner.is_empty() && existing_owner != user_email {
          return vec![ResponseMsg::Error(format!("NOT_OWNER:{existing_owner}"))];
        }
        if !save_msg.force_overwrite {
          return vec![ResponseMsg::Error("SCENARIO_EXISTS".to_string())];
        }
      }
    }

    // Stamp metadata + filename onto the live Entities, then snapshot to JSON.
    // The dialog's "Display Name" is what lands in metadata.name; the on-disk
    // filename is tracked separately on Entities.filename. If the user didn't
    // provide a display name, fall back to the file stem so metadata.name
    // is never blank (the picker renders it).
    let display_label = if save_msg.display_name.trim().is_empty() {
      file_stem.clone()
    } else {
      save_msg.display_name.clone()
    };
    let json = {
      let mut entities = match server.get_unlocked_entities() {
        Ok(e) => e,
        Err(e) => return vec![ResponseMsg::Error(format!("Failed to lock scenario: {e}"))],
      };
      entities.metadata = MetaData {
        name: display_label.clone(),
        description: save_msg.description.clone(),
        owner: user_email.clone(),
      };
      entities.filename.clone_from(&file_name);
      match entities.to_scenario_file_json() {
        Ok(bytes) => bytes,
        Err(e) => return vec![ResponseMsg::Error(format!("Failed to serialize scenario: {e}"))],
      }
    };

    if let Err(e) = write_local_or_cloud_file(&full_path, json).await {
      return vec![ResponseMsg::Error(format!("Failed to write scenario: {e}"))];
    }

    event!(
      target: LOG_FILE_USE,
      Level::INFO,
      file_name = full_path.as_str(),
      use = "Save scenario."
    );
    event!(
      target: LOG_SCENARIO_ACTIVITY,
      Level::INFO,
      email = user_email.as_str(),
      scenario = file_name.as_str(),
      action = "save"
    );

    // Refresh the global scenarios snapshot so the picker reflects this save
    // immediately, instead of waiting for the 5s file-watcher poll.
    let new_metadata = server.get_unlocked_entities().map(|e| e.metadata.clone()).ok();
    if let Some(metadata) = new_metadata {
      let mut scenarios: Vec<(String, MetaData)> = (*get_scenarios_snapshot()).clone();
      if let Some(idx) = scenarios.iter().position(|(n, _)| n == &file_name) {
        scenarios[idx] = (file_name.clone(), metadata);
      } else {
        scenarios.push((file_name.clone(), metadata));
      }
      replace_scenarios(scenarios);
    }

    vec![
      ResponseMsg::ScenarioSaved(file_name),
      ResponseMsg::Scenarios(self.build_scenarios_msg()),
    ]
  }
}

// Utility functions to help build messages etc.

fn is_broadcast_message(message: &ResponseMsg) -> bool {
  matches!(message, ResponseMsg::EntityResponse(_))
    || matches!(message, ResponseMsg::Users(_))
    || matches!(message, ResponseMsg::Scenarios(_))
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

async fn send_response(stream: &mut WebSocketStream<SubStream>, message: &ResponseMsg, context: &str) {
  let encoded_message: Utf8Bytes = serde_json::to_string(message).expect("Failed to serialize response").into();
  stream.send(Message::Text(encoded_message)).await.unwrap_or_else(|e| {
    error!("(processor) Failed to send {context}: {e:?}");
  });
}

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

use crate::build_successful_auth_msgs;
use crate::entity::Entities;
use crate::handle_request;
use crate::payloads::{AuthResponse, ResponseMsg};
use crate::server::Server;

#[cfg(feature = "no_tls_upgrade")]
type SubStream = TcpStream;
#[cfg(not(feature = "no_tls_upgrade"))]
type SubStream = TlsStream<TcpStream>;

#[allow(unused_imports)]
use crate::{debug, error, info, warn};

/// Polls all incoming connections and transmits any messages to
/// the processing loop.
///
/// The structure here is:
/// * one thread in [main] that that accepts incoming connections.  It gives up ownership of the connection once established.
/// * one thread for this [`connection_manager`] that then receives messages from all connections, processes them, and send replies.
///
/// # Arguments
/// * `connection_receiver` - A channel to receive new connections from the acceptor thread.  It takes a fully upgraded secure websocket stream,
///     as well as any already authenticated email and session key (due to http cookies on the connection).
/// * `auth_template` - A template [Authenticator], i.e. one without session key or email set.  We use this to clone on each new connection, and then set the session key and email.
/// * `session_keys` - The session keys for all connections.  This is a map of session keys to email addresses.  Used here when a user logs in (to update this info)
/// * `test_mode` - Whether we are in test mode.  Test mode disables authentication and ensures a deterministic seed for each random number generator.
///
/// # Panics
/// If we cannot properly serialize or deserialize a message on the stream.
#[allow(clippy::implicit_hasher)]
#[allow(clippy::too_many_lines)]
pub async fn processor(
    mut connection_receiver: Receiver<(WebSocketStream<SubStream>, String, Option<String>)>,
    auth_template: Box<dyn Authenticator>,
    entities: Arc<Mutex<Entities>>,
    session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
    test_mode: bool,
) {
    // All the data shared between authenticators.
    let mut connections = Vec::<Connection>::new();

    loop {
        // If there are no connections, then we wait for one to come in.
        // Special case as waiting on an empty FuturesUnordered will not wait - just returns None.
        // TODO: Violating DRY here in a big way.  How do I fix it?
        if connections.is_empty() {
            let next_connection = connection_receiver.next().await;
            if let Some((stream, session_key, email)) = next_connection {
                let Some(connection) = build_connection(
                    &auth_template,
                    &session_key,
                    &session_keys,
                    email.as_ref(),
                    stream,
                    &entities,
                    test_mode,
                )
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
            next_connection = connection_receiver.next() => {
                if let Some((stream, session_key, email)) = next_connection {
                // Build the authenticator
                let Some(connection) = build_connection(
                    &auth_template,
                    &session_key,
                    &session_keys,
                    email.as_ref(),
                    stream,
                    &entities,
                    test_mode,
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
                let response = if let Ok(parsed_message) = serde_json::from_str(&text) {
                    // TODO: I think we move handle_request into the server, but lets come back to that.
                    let response = handle_request(
                        parsed_message,
                        &mut connections[index].server,
                        session_keys.clone(),
                    )
                    .await;
                    // This is a bit of a hack. We use `LogoutResponse` to signal that we should close the connection.
                    // but do not actually ever send it to the client (who has logged out!)
                    if response
                        .iter()
                        .filter(|msg| matches!(msg, ResponseMsg::LogoutResponse))
                        .count()
                        > 0
                    {
                        // User has logged out.  Close the connection.
                        debug!(
                            "(processor) User logged out.  Closing connection. Now {} connections.",
                            connections.len() - 1
                        );
                        connections[index]
                                .stream
                                .close(None)
                                .await
                                .unwrap_or_else(|e| {
                                    error!("(handle_connection) Failed to close connection as directed by logout: {e:?}");
                                });
                    }
                    response
                        .into_iter()
                        .filter(|msg| !matches!(msg, ResponseMsg::LogoutResponse))
                        .collect()
                } else {
                    vec![ResponseMsg::Error(format!("Malformed message: {text}"))]
                };

                debug!("(handle_connection) Response(s): {response:?}");

                // Send the response
                for message in response {
                    let encoded_message: Utf8Bytes =
                        serde_json::to_string(&message).unwrap().into();
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
                        connections[index]
                            .stream
                            .send(Message::Text(
                                serde_json::to_string(&message)
                                    .expect("Failed to serialize response")
                                    .into(),
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
                connections[index]
                    .stream
                    .close(None)
                    .await
                    .unwrap_or_else(|e| {
                        if let Error::Protocol(ProtocolError::SendAfterClosing) = e {
                            // This is expected when we try to close a connection that is already closed.
                            debug!("(processor) Attempted to close a connection that was already closed.  Ignoring.");
                        } else {
                            error!("(handle_connection) Failed to close connection: {e:?}");
                    }});
                // Mark this stream for deletion
                connections.remove(index);
                debug!(
                    "(processor) Removed connection.  Now {} connections.",
                    connections.len()
                );
            }
            Some((index, res)) => {
                error!("(processor) Unexpected message on connection {index}: {res:?}");
            }
            None => {
                warn!("(processor) Strange `None` response from message stream.  Ignoring");
                continue;
            }
        }
    }
}

struct Connection {
    server: Server,
    stream: WebSocketStream<SubStream>,
}

fn is_broadcast_message(message: &ResponseMsg) -> bool {
    matches!(message, ResponseMsg::EntityResponse(_)) || matches!(message, ResponseMsg::Users(_))
}

#[allow(clippy::borrowed_box)]
#[must_use]
async fn build_connection(
    auth_template: &Box<dyn Authenticator>,
    session_key: &str,
    session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    email: Option<&String>,
    stream: WebSocketStream<SubStream>,
    entities: &Arc<Mutex<Entities>>,
    test_mode: bool,
) -> Option<Connection> {
    let mut authenticator = clone_box(auth_template.as_ref());
    authenticator.set_session_key(session_key);
    authenticator.set_email(email);
    let mut connection = Connection {
        server: Server::new(entities.clone(), authenticator, test_mode),
        stream,
    };

    if let Some(email) = email {
        // If we got a successful Some(email) then we need to fake like this was a log in by
        // letting the client know auth was successful, but also sending any initialization messages.
        // We use [build_successful_auth_msgs] to keep this list of messages the same as if it was in response
        // to a login message.
        let msgs = build_successful_auth_msgs(
            AuthResponse {
                email: email.clone(),
            },
            &connection.server,
            session_keys,
        );
        let mut okay = true;
        for msg in msgs {
            let encoded_message: Utf8Bytes = serde_json::to_string(&msg).unwrap().into();
            if connection
                .stream
                .send(Message::Text(encoded_message))
                .await
                .is_err()
            {
                okay = false;
                break;
            };
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

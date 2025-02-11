use std::boxed::Box;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::Receiver;
use futures::select;
use futures::{stream::FuturesUnordered, SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio_rustls::server::TlsStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;

use dyn_clone::clone_box;

use crate::authentication::Authenticator;

use crate::entity::Entities;
use crate::handle_request;
use crate::payloads::ResponseMsg;
use crate::server::Server;
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
    mut connection_receiver: Receiver<(
        WebSocketStream<TlsStream<TcpStream>>,
        String,
        Option<String>,
    )>,
    auth_template: Box<dyn Authenticator>,
    session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
    test_mode: bool,
) {
    // All the data shared between authenticators.
    let entities = Arc::new(Mutex::new(Entities::new()));
    let mut connections = Vec::<Connection>::new();

    loop {
        // If there are no connections, then we wait for one to come in.
        // Special case as waiting on an empty FuturesUnordered will not wait - just returns None.
        if connections.is_empty() {
            let next_connection = connection_receiver.next().await;
            if let Some((stream, session_key, email)) = next_connection {
                let connection = build_connection(
                    &auth_template,
                    &session_key,
                    email.as_ref(),
                    stream,
                    &entities,
                    test_mode,
                );
                connections.push(connection);
            } else {
                // This is expected when the main thread exits.
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

        debug!("(processor) Waiting for next connection or message.");

        // Wait on either a new connection or a message from an existing connection, whichever comes first.
        // Return the next message to process if there is one.
        let to_do = select! {
            next_connection = connection_receiver.next() => {
                debug!("(processor) New connection");
                if let Some((stream, session_key, email)) = next_connection {
                // Build the authenticator
                let connection = build_connection(
                    &auth_template,
                    &session_key,
                    email.as_ref(),
                    stream,
                    &entities,
                    test_mode,
                );
                drop(message_streams);
                connections.push(connection);

                debug!("(processor) Added new connection.  Total connections: {}", connections.len());
                continue;
            }
                debug!("(processor) Issue with new connection.");
                // This is expected when the main thread exits.
                info!("(processor) Connection receiver disconnected.  Exiting.");
                break;
            },
            next_item =  message_streams.next() => {
                debug!("(processor) New message");
                if let Some((index, Some(next_msg))) = next_item {
                    Some((index, next_msg))
                } else {
                    // This is expected when the main thread exits.
                    info!("(processor) Connection receiver disconnected.  Exiting. {next_item:?}");
                    continue;
                }
            }
        };

        drop(message_streams);

        debug!("(processor) Process message (if any): {to_do:?}");
        if let Some((index, Ok(Message::Text(text)))) = to_do {
            let current = &mut connections[index];
            debug!("(handle_connection) Received message: {text}");
            // Handle the request
            let response = if let Ok(parsed_message) = serde_json::from_str(&text) {
                // TODO: I think we move handle_request into the server, but lets come back to that.
                handle_request(parsed_message, &mut current.server, session_keys.clone()).await
            } else {
                Ok(vec![ResponseMsg::Error("Malformed message.".to_string())])
            };

            debug!("(handle_connection) Response(s): {response:?}");

            // Send the response
            if let Ok(response) = response {
                for message in response {
                    current
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
            } else {
                error!(
                    "(handle_connection) Error processing handle_request on message {response:?}."
                );
                // Let the client know
                current
                    .stream
                    .send(Message::Text(
                        serde_json::to_string(&ResponseMsg::Error(
                            "Unable to process message.".to_string(),
                        ))
                        .expect("Failed to serialize response")
                        .into(),
                    ))
                    .await
                    .unwrap_or_else(|e| {
                        error!("(handle_connection) Failed to send response: {e:?}");
                    });
            }
        } else {
            error!("(processor) Unexpected message: {:?}", to_do);
            // Close the connection
            //current.stream.close(None).await.unwrap_or_else(|e| {
            //    error!("(handle_connection) Failed to close connection: {e:?}");
            //});
            // Mark this stream for deletion
        }
    }
}
/*/
match select(new_connection, next_data).await {
    Either::Left((Some((stream, session_key, email)), _)) => {
        // Build the authenticator
        let authenticator = build_authenticator(
            server_name.clone(),
            web_server_name.clone(),
            my_credentials.clone(),
            authorized_users.clone(),
            google_keys.clone(),
            session_key,
            email,
            test_mode,
        );

        let connection = Connection {
            server: Server::new(entities.clone(), authenticator, test_mode),
            stream,
        };
        connections.push(connection);
    }
    Either::Left((None, _)) => {
        // This is expected when the main thread exits.
        println!("(processor) Connection receiver disconnected.  Exiting.");
        break;
    }
    Either::Right((((index, next_msg), _, _), _)) => {
        // TODO: This is horribly complex and needs to be simplified.
        let current = &mut connections[index];
        match next_msg {
            Some(Ok(Message::Text(text))) => {
                debug!("(handle_connection) Received message: {text}");
                // Handle the request
                let response = if let Ok(parsed_message) = serde_json::from_str(&text) {
                    // TODO: I think we move handle_request into the server, but lets come back to that.
                    handle_request(
                        parsed_message,
                        &mut current.server,
                        session_keys.clone(),
                    )
                    .await
                } else {
                    Ok(vec![ResponseMsg::Error("Malformed message.".to_string())])
                };

                debug!("(handle_connection) Response(s): {response:?}");

                // Send the response
                if let Ok(response) = response {
                    for message in response {
                        current
                            .stream
                            .send(Message::Text(
                                serde_json::to_string(&message)
                                    .expect("Failed to serialize response")
                                    .into(),
                            ))
                            .await
                            .unwrap_or_else(|e| {
                                error!(
                                    "(handle_connection) Failed to send response: {e:?}"
                                );
                            });
                    }
                } else {
                    error!("(handle_connection) Error processing handle_request on message {response:?}.");
                    // Let the client know
                    current
                        .stream
                        .send(Message::Text(
                            serde_json::to_string(&ResponseMsg::Error(
                                "Unable to process message.".to_string(),
                            ))
                            .expect("Failed to serialize response")
                            .into(),
                        ))
                        .await
                        .unwrap_or_else(|e| {
                            error!("(handle_connection) Failed to send response: {e:?}");
                        });
                }
            }
            Some(Ok(Message::Close(_))) => {
                // Close the connection
                current.stream.close(None).await.unwrap_or_else(|e| {
                    error!("(handle_connection) Failed to close connection: {e:?}");
                });
                // TODO: Need to mark this stream for deletion
                break;
            }
            Some(Ok(m)) => {
                error!("(handle_connection) Unexpected message: {:?}", m);
            }
            Some(Err(e)) => {
                error!("(handle_connection) Could not read next message: {e:?}");
            }
            None => {
                // BAD: No idea why we get a None here so not sure if this is correct.
                error!("(handle_connection) Connection closed.");
                // Close the connection
                current.stream.close(None).await.unwrap_or_else(|e| {
                    error!("(handle_connection) Failed to close connection: {e:?}");
                });
                // TODO: Mark this stream for deletion
                break;
            }
        }
    } */

struct Connection {
    server: Server,
    stream: WebSocketStream<TlsStream<TcpStream>>,
}

#[allow(clippy::borrowed_box)]
fn build_connection(
    auth_template: &Box<dyn Authenticator>,
    session_key: &str,
    email: Option<&String>,
    stream: WebSocketStream<TlsStream<TcpStream>>,
    entities: &Arc<Mutex<Entities>>,
    test_mode: bool,
) -> Connection {
    let mut authenticator = clone_box(auth_template.as_ref());
    authenticator.set_session_key(session_key);
    authenticator.set_email(email);
    Connection {
        server: Server::new(entities.clone(), authenticator, test_mode),
        stream,
    }
}

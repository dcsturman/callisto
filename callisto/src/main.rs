use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::panic;
use std::path::PathBuf;
use std::process;
use std::str::FromStr;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

use clap::Parser;
use log::{debug, error, info, warn};

extern crate callisto;

use callisto::authentication::{
    load_authorized_users, Authenticator, GoogleAuthenticator, HeaderCallback, MockAuthenticator,
};
use callisto::entity::Entities;
use callisto::handle_request;
use callisto::payloads::ResponseMsg;
use callisto::ship::{load_ship_templates_from_file, SHIP_TEMPLATES};

const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";
const DEFAULT_AUTHORIZED_USERS_FILE: &str = "./config/authorized_users.json";

/// Server to implement physically pseudo-realistic spaceflight and possibly combat.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port for server to listen on
    #[arg(short, long, default_value_t = 8443)]
    port: u16,

    /// Local IP address to bind to.  Typically 127.0.0.1 or 0.0.0.0 (for docker)
    #[arg(short, long, default_value = "0.0.0.0")]
    address: String,

    /// JSON file for planets in scenario
    #[arg(short = 'f', long)]
    scenario_file: Option<String>,

    /// JSON file for ship templates in scenario
    #[arg(short, long, default_value = DEFAULT_SHIP_TEMPLATES_FILE)]
    design_file: String,

    /// Run in test mode. Specifically, this will use a fixed random number generator.
    #[arg(short, long)]
    test: bool,

    // Name of the web server hosting the react app. This must be used correct to make CORS work.
    #[arg(short, long, default_value = "http://localhost:50001")]
    web_server: String,

    // Location of the oauth google credentials.  Important, for example, if using Docker secrets
    #[arg(long, default_value = "./secrets/google_credentials.json")]
    oauth_creds: String,

    // Prefix of the certificate and key files for tls.  The server will append .crt and .key to this.
    #[arg(short = 'k', long, default_value = "keys/localhost")]
    tls_keys: String,

    // Google Cloud Storage bucket to use in lieu of config directory
    #[arg(short, long, default_value = DEFAULT_AUTHORIZED_USERS_FILE)]
    users_file: String,
}

async fn handle_connection(
    stream: TcpStream,
    acceptor: Arc<TlsAcceptor>,
    entities: Arc<Mutex<Entities>>,
    mut authenticator: Box<dyn Authenticator>,
    session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
    test_mode: bool,
) {
    // First, upgrade the stream to be TLS
    let stream = match acceptor.accept(stream).await {
        Ok(stream) => stream,
        Err(e) => {
            warn!("(handle_connection) Failed to upgrade TcpStream to TLS: {e:?}");
            return;
        }
    };

    debug!("(handle_connection) Upgraded to TLS.");

    // Second, upgrade the stream to use websockets with tungstenite
    // TODO: Add a config here for extra safety
    // TODO: This is where we can check headers. How do we set them?

    let mut tmp_email = None;
    let callback_handler = HeaderCallback {
        session_keys: session_keys.clone(),
        email_setter: |email| tmp_email = email,
    };

    let mut ws_stream = tokio_tungstenite::accept_hdr_async(stream, callback_handler)
        .await
        .unwrap_or_else(|e| {
            error!(
                "(handle_connection) Error during the websocket handshake occurred: {}",
                e
            );
            process::exit(1);
        });

    debug!("(handle_connection) Upgraded to websockets and starting message handling loop.");

    while let Some(msg) = ws_stream.next().await {
        match msg {
            Ok(Message::Text(text)) => {
                debug!("(handle_connection) Received message: {text}");
                // Handle the request
                let response = if let Ok(parsed_message) = serde_json::from_str(&text) {
                    handle_request(
                        parsed_message,
                        entities.clone(),
                        test_mode,
                        &mut authenticator,
                    )
                    .await
                } else {
                    // TODO: Really this should return a 400? But can I do that without killing the connection?
                    Ok(vec![ResponseMsg::Error("Malformed message.".to_string())])
                };

                debug!("(handle_connection) Response(s): {response:?}");
                
                // Send the response
                if let Ok(response) = response {
                    for message in response {
                        ws_stream
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
                    error!("(handle_connection) Error processing handle_request on message {response:?}.");
                    // Let the client know
                    ws_stream
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
            Ok(Message::Close(_)) => {
                // Close the connection
                ws_stream.close(None).await.unwrap_or_else(|e| {
                    error!("(handle_connection) Failed to close connection: {e:?}");
                });
                break;
            }
            Ok(m) => {
                error!("(handle_connection) Unexpected message: {:?}", m);
            }
            Err(e) => {
                error!("(handle_connection) Could not read next message: {e:?}");
            }
        }
    }

    debug!("(handle_connection) Connection closed.");
}

#[tokio::main]
#[quit::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    pretty_env_logger::init();
    let args = Args::parse();

    let port = args.port;
    debug!("Using port: {port}");

    let ip_addr = IpAddr::from_str(&args.address)?;
    let addr = SocketAddr::from((ip_addr, port));

    // Load our certs and key.
    let cert_path = PathBuf::from(format!("{}.crt", args.tls_keys));
    let certs = CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;

    let key_path = PathBuf::from(format!("{}.key", args.tls_keys));
    let key = PrivateKeyDer::from_pem_file(key_path)?;

    info!("(main) Loaded certs and key.");

    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    let acceptor = Arc::new(TlsAcceptor::from(Arc::new(config)));
    let listener = TcpListener::bind(&addr).await?;

    info!("(main) Bound to address (tcp): {}", addr);
    let test_mode = args.test;
    if test_mode {
        info!("(main) Server in TEST mode.");
    } else {
        info!(
            "(main) Server in standard mode.  Referring frontend = {}",
            args.web_server
        );
    }

    let templates = load_ship_templates_from_file(&args.design_file)
        .await
        .unwrap_or_else(|e| {
            panic!(
                "Unable to load ship template file {}. Reason {:?}",
                args.design_file, e
            )
        });

    info!("(main) Loaded ship templates from {}.", &args.design_file);

    SHIP_TEMPLATES
        .set(templates)
        .expect("(Main) attempting to set SHIP_TEMPLATES twice!");

    info!("(main) Loaded ship templates.");

    // Build the main entities table that will be the state of our server.
    let entities = Arc::new(Mutex::new(if let Some(file_name) = args.scenario_file {
        Entities::load_from_file(&file_name)
            .await
            .unwrap_or_else(|e| panic!("Issue loading scenario file {file_name}: {e}"))
    } else {
        Entities::new()
    }));

    info!(
        "Starting with scenario entities: {:?}",
        entities.lock().unwrap()
    );

    info!("Starting Callisto server listening on address: {}", addr);

    if test_mode {
        debug!("(main) In test mode: custom catching of panics.");
        let orig_hook = panic::take_hook();
        panic::set_hook(Box::new(move |panic_info| {
            if panic_info
                .payload()
                .downcast_ref::<&str>()
                .unwrap_or(&"")
                .contains("Time to exit")
            {
                process::exit(0);
            } else {
                orig_hook(panic_info);
                process::exit(1);
            }
        }));
    }

    // All the data shared between authenticators.
    let session_keys = Arc::new(Mutex::new(HashMap::new()));
    let valid_users = load_authorized_users(&args.users_file).await;
    let my_credentials = GoogleAuthenticator::load_google_credentials(&args.oauth_creds);
    let google_keys = GoogleAuthenticator::fetch_google_public_keys().await;

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        debug!("(main) Accepted connection from: {}", peer_addr);

        // Spawn a tokio task to serve multiple connections concurrently
        let ent = entities.clone();
        let acceptor = acceptor.clone();

        // Build the authenticator
        let authenticator: Box<dyn Authenticator> = if test_mode {
            Box::new(MockAuthenticator::new(&args.web_server))
        } else {
            Box::new(GoogleAuthenticator::new(
                &args.web_server,
                args.web_server.clone(),
                my_credentials.clone(),
                google_keys.clone(),
                valid_users.clone(),
            ))
        };
        tokio::task::spawn(handle_connection(
            stream,
            acceptor,
            ent,
            authenticator,
            session_keys.clone(),
            test_mode,
        ));
    }
}

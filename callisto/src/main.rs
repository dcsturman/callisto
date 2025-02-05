use callisto::server::Server;
use tokio::net::TcpListener;

use std::net::SocketAddr;
use std::panic;
use std::path::PathBuf;
use std::process;
use std::sync::{Arc, Mutex};

use futures_util::{SinkExt, StreamExt};

use rustls::pki_types::pem::PemObject;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::net::TcpStream;
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite;

use clap::Parser;
use log::{debug, error, info};

extern crate callisto;

use callisto::authentication::Authenticator;
use callisto::authentication::GoogleAuthenticator;
use callisto::authentication::MockAuthenticator;
use callisto::entity::Entities;
use callisto::handle_request;
use callisto::payloads::OutgoingMsg;
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

    // Location of the secrets directory.  Important, for example, if using Docker secrets
    #[arg(long, default_value = "./secrets/google_credentials.json")]
    secret: String,

    // Google Cloud Storage bucket to use in lieu of config directory
    #[arg(short, long, default_value = DEFAULT_AUTHORIZED_USERS_FILE)]
    users_file: String,
}

async fn handle_connection(
    stream: TcpStream,
    acceptor: Arc<TlsAcceptor>,
    entities: Arc<Mutex<Entities>>,
    authenticator: Arc<Box<dyn Authenticator>>,
    test_mode: bool,
) {
    // First, upgrade the stream to be TLS
    let stream = acceptor
        .accept(stream)
        .await
        .expect("(handle_connection) Failed to upgrade TcpStream to TLS.");
    debug!("(handle_connection) Upgraded to TLS.");

    // Second, upgrade the stream to use websockets with tungstenite
    let mut ws_stream = tokio_tungstenite::accept_async(stream)
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
            Ok(tungstenite::Message::Text(text)) => {
                // Handle the request
                let response = if let Ok(parsed_message) = serde_json::from_str(&text) {
                    handle_request(
                        parsed_message,
                        entities.clone(),
                        test_mode,
                        authenticator.clone(),
                    )
                    .await
                } else {
                    // TODO: Really this should return a 400? But can I do that without killing the connection?
                    Ok(OutgoingMsg::Error("Malformed message.".to_string()))
                };

                // Send the response
                if let Ok(response) = response {
                    ws_stream
                        .send(tungstenite::Message::Text(
                            serde_json::to_string(&response)
                                .expect("Failed to serialize response")
                                .into(),
                        ))
                        .await
                        .unwrap_or_else(|e| {
                            error!("(handle_connection) Failed to send response: {e:?}");
                        });
                } else {
                    error!("(handle_connection) Error processing handle_request on message {response:?}.");
                    // Let the client know
                    ws_stream
                        .send(tungstenite::Message::Text(
                            serde_json::to_string(&OutgoingMsg::Error(
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
            Ok(tungstenite::Message::Close(_)) => {
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

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

    // Load our certs and key.
    let cert_path = PathBuf::from("keys/server-cert.crt");
    let certs = CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;

    let key_path = PathBuf::from("keys/server-cert.key");
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

    // Build the authenticator
    let authenticator: Box<dyn Authenticator> = if test_mode {
        Box::new(MockAuthenticator::new(
            &args.web_server,
            args.secret,
            &args.users_file,
            args.web_server.clone(),
        ))
    } else {
        Box::new(
            GoogleAuthenticator::new(
                &args.web_server,
                args.secret,
                &args.users_file,
                args.web_server.clone(),
            )
            .await,
        )
    };

    let authenticator = Arc::new(authenticator);

    info!("(main) Built authenticator.");

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

    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, peer_addr) = listener.accept().await?;
        debug!("(main) Accepted connection from: {}", peer_addr);

        // Spawn a tokio task to serve multiple connections concurrently
        let ent = entities.clone();
        let auth = authenticator.clone();
        let acceptor = acceptor.clone();

        /*let fut = async move  {
            handle_connection(stream, acceptor, ent, auth).await.unwrap();
            future::ok::<(),Box<dyn std::error::Error + Send + Sync + 'static>>(())
        };*/

        tokio::task::spawn(handle_connection(stream, acceptor, ent, auth, test_mode));
    }
}

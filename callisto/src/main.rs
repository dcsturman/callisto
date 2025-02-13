use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::panic;
use std::process;
use std::sync::{Arc, Mutex};

use futures::channel::mpsc::{ Sender, Receiver, channel };
use tokio_tungstenite::WebSocketStream;
use tokio::net::{TcpListener, TcpStream};

// Things we use only when we are not using the `no_tls_upgrade` feature.
#[cfg(not(feature = "no_tls_upgrade"))]
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_rustls::server::TlsStream;
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_rustls::TlsAcceptor;
#[cfg(not(feature = "no_tls_upgrade"))]
use std::path::PathBuf;
#[cfg(not(feature = "no_tls_upgrade"))]
use rustls::pki_types::pem::PemObject;


use clap::Parser;
use log::{debug, error, info, warn};

extern crate callisto;

use callisto::authentication::{
    load_authorized_users, Authenticator, GoogleAuthenticator, HeaderCallback, MockAuthenticator,
};

use callisto::entity::Entities;
use callisto::processor::processor;
use callisto::ship::{load_ship_templates_from_file, SHIP_TEMPLATES};

const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";
const DEFAULT_AUTHORIZED_USERS_FILE: &str = "./config/authorized_users.json";

const MAX_CHANNEL_DEPTH: usize = 10;

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


#[cfg(feature = "no_tls_upgrade")]
pub type SubStream = TcpStream;
#[cfg(not(feature = "no_tls_upgrade"))]
pub type SubStream = TlsStream<TcpStream>;

/// Build a possibly full secure websocket from a raw TCP stream.
/// This function relies heavily on the feature `no_tls_upgrade`.  If the feature is enabled
/// the type [`SubStream`] is a [`TcpStream`], otherwise it is a [`TlsStream<TcpStream>`].
///
/// # Arguments
/// * `stream` - The raw TCP stream to upgrade.
/// * `acceptor` - The TLS acceptor to use to upgrade the stream. (only when `no_tls_upgrade` is not enabled)
/// * `session_keys` - The session keys to use for authentication.  This is a map of session keys to email addresses.  This is used to authenticate the user.  Its included
///     here because on connection we can see any `HttpCookie` on the request.  We use that in case a connection drops and reconnects so we don't need to force a re-login.
///
/// # Returns
/// A tuple of the websocket stream, the session key, and an optional email address.  The email address we `Some(email)` if this user has previously logged in.
async fn handle_connection(
    stream: TcpStream,
    #[cfg(not(feature = "no_tls_upgrade"))] acceptor: Arc<TlsAcceptor>,
    session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
) -> Result<(WebSocketStream<SubStream>, String, Option<String>), String> {
    #[cfg(not(feature = "no_tls_upgrade"))]
    // First, upgrade the stream to be TLS
    let stream: SubStream = match acceptor.accept(stream).await {
        Ok(stream) => {
            debug!("(handle_connection) Upgraded to TLS.");
            stream
        }
        Err(e) => {
            warn!("(handle_connection) Failed to upgrade TcpStream to TLS: {e:?}");
            return Err(format!(
                "(handle_connection) Failed to upgrade TcpStream to TLS: {e:?}"
            ));
        }
    };

    // Second, upgrade the stream to use websockets with tungstenite
    // TODO: Add a config here for extra safety

    // Tmp locked structure to get info out of the accept handler.
    // This is necessary because the callback_handler is consumed, so other approaches didn't need for me.
    // First element is the session key, second is the email.
    // TODO: Is there a better way to do this?  We just need to get this returned.  We don't actually need to create
    // this here. We also don't need to access it until the callback_handler is done.
    let auth_info = Arc::new(Mutex::new((String::new(), None)));

    let callback_handler = HeaderCallback {
        session_keys: session_keys.clone(),
        auth_info: auth_info.clone(),
    };

    let ws_stream: WebSocketStream<SubStream> = tokio_tungstenite::accept_hdr_async(stream, callback_handler)
        .await
        .unwrap_or_else(|e| {
            error!(
                "(handle_connection) Error during the websocket handshake occurred: {}",
                e
            );
            process::exit(1);
        });

    let auth_info = auth_info.lock().unwrap();
    Ok((ws_stream, auth_info.0.clone(), auth_info.1.clone()))
}

#[tokio::main]
#[quit::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    pretty_env_logger::init();
    let args = Args::parse();

    let port = args.port;
    debug!("Using port: {port}");

    //let ip_addr = IpAddr::from_str(&args.address)?;
    //let addr = SocketAddr::from((ip_addr, port));
    let addr = (args.address.clone(), port)
        .to_socket_addrs()
        .expect("Unable to resolve the IP address for this server")
        .next()
        .expect("DNS resolution returned no IP addresses");

    // Load our certs and key.
    #[cfg(not(feature = "no_tls_upgrade"))]
    let cert_path = PathBuf::from(format!("{}.crt", args.tls_keys));
    #[cfg(not(feature = "no_tls_upgrade"))]
    let certs = CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;
    #[cfg(not(feature = "no_tls_upgrade"))]
    let key_path = PathBuf::from(format!("{}.key", args.tls_keys));
    #[cfg(not(feature = "no_tls_upgrade"))]
    let key = PrivateKeyDer::from_pem_file(key_path)?;
    #[cfg(not(feature = "no_tls_upgrade"))]
    info!("(main) Loaded certs and key.");

    #[cfg(not(feature = "no_tls_upgrade"))]
    let config = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    #[cfg(not(feature = "no_tls_upgrade"))]
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

    // Keep track of session keys (cookies) on connections.
    let session_keys = Arc::new(Mutex::new(HashMap::new()));

    // All the data shared between authenticators.
    let authorized_users = load_authorized_users(&args.users_file).await;
    let my_credentials = GoogleAuthenticator::load_google_credentials(&args.oauth_creds);
    let (mut connection_sender, connection_receiver): (Sender<(WebSocketStream<SubStream>, String, Option<String>)>, Receiver<(WebSocketStream<SubStream>, String, Option<String>)>) = channel(MAX_CHANNEL_DEPTH);

    // Create an Authenticator to be cloned on each new connection.

    let auth_template: Box<dyn Authenticator> = if test_mode {
        Box::new(MockAuthenticator::new(&args.address))
    } else {
        let authorized_users = Arc::new(authorized_users);
        let my_credentials = Arc::new(my_credentials);
        let google_keys = GoogleAuthenticator::fetch_google_public_keys().await;

        Box::new(GoogleAuthenticator::new(
            &args.web_server,
            my_credentials,
            google_keys,
            authorized_users,
        ))
    };

    // Start a processor thread to handle all connections once established.
    tokio::task::spawn(processor(
        connection_receiver,
        auth_template,
        entities,
        session_keys.clone(),
        test_mode,
    ));

    info!("Starting Callisto server listening on address: {}", addr);

    // We start a loop to continuously accept incoming connections.  Once we have a connection
    // it gets upgraded (or fails) and then is sent to the master thread.
    // Eventually we'll have one such thread per server.
    loop {
        let (stream, peer_addr) = listener.accept().await?;

        // Upgrade will be built differently depending on the feature `no_tls_upgrade`.
        #[cfg(feature = "no_tls_upgrade")]
        let upgrade: Result<(WebSocketStream<SubStream>, _, _), _> = handle_connection(stream, session_keys.clone()).await;

        #[cfg(not(feature = "no_tls_upgrade"))]
        let upgrade: Result<(WebSocketStream<SubStream>, _, _), _> = handle_connection(stream, acceptor.clone(), session_keys.clone()).await;

        match upgrade {
            Ok((ws_stream, session_key, email)) => {
                debug!("(main) Successfully established websocket connection to {peer_addr}.");
                connection_sender
                    .try_send((ws_stream, session_key, email))
                    .unwrap();
            }
            Err(e) => {
                warn!("(main) Server at {addr} failed to establish websocket connection from {peer_addr}: {e}");
                continue;
            }
        }
    }
}

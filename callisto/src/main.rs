use tokio::net::TcpListener;

use std::net::SocketAddr;
use std::panic;
use std::process;
use std::sync::{Arc, Mutex};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;

use clap::Parser;
use log::{debug, error, info};

extern crate callisto;

use callisto::authentication::Authenticator;
use callisto::authentication::GoogleAuthenticator;
use callisto::authentication::MockAuthenticator;
use callisto::entity::Entities;
use callisto::handle_request;
use callisto::ship::{load_ship_templates_from_file, SHIP_TEMPLATES};

const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";
const DEFAULT_AUTHORIZED_USERS_FILE: &str = "./config/authorized_users.json";

/// Server to implement physically pseudo-realistic spaceflight and possibly combat.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port for server to listen on
    #[arg(short, long, default_value_t = 3000)]
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

#[tokio::main]
#[quit::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();
    let args = Args::parse();

    let port = args.port;
    debug!("Using port: {port}");

    let addr = SocketAddr::from(([0, 0, 0, 0], port));

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

    // Build the authenticator
    let authenticator: Box<dyn Authenticator> = if test_mode {
        Box::new(
            MockAuthenticator::new(
                &args.web_server,
                args.secret,
                &args.users_file,
                args.web_server.clone(),
            )
            .await,
        )
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
    // Build the main entities table that will be the state of our server.
    let entities = Arc::new(Mutex::new(if let Some(file_name) = args.scenario_file {
        Entities::load_from_file(&file_name)
            .await
            .unwrap_or_else(|e| panic!("Issue loading scenario file {}: {}", file_name, e))
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
            if !panic_info
                .payload()
                .downcast_ref::<&str>()
                .unwrap_or(&"")
                .contains("Time to exit")
            {
                orig_hook(panic_info);
                process::exit(1);
            } else {
                process::exit(0);
            }
        }));
    }

    // We create a TcpListener and bind it to 127.0.0.1:PORT
    let listener = TcpListener::bind(addr).await?;
    // We start a loop to continuously accept incoming connections
    loop {
        let (stream, _) = listener.accept().await?;

        // Use an adapter to access something implementing `tokio::io` traits as if they implement
        // `hyper::rt` IO traits.
        let io = TokioIo::new(stream);

        // Spawn a tokio task to serve multiple connections concurrently
        let e = entities.clone();
        let a = authenticator.clone();
        tokio::task::spawn(async move {
            let ent = e.clone();
            let handler = move |req| handle_request(req, ent.clone(), test_mode, a.clone());

            // We bind the incoming connection to our service
            let builder = http1::Builder::new();
            if let Err(err) = builder.serve_connection(io, service_fn(handler)).await {
                error!("Error serving connection: {:?}", err);
            }
        });
    }
}

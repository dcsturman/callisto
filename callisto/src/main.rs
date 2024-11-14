use tokio::net::TcpListener;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;

use clap::Parser;
use log::info;

extern crate callisto;

use callisto::entity::Entities;
use callisto::handle_request;
use callisto::ship::{load_ship_templates_from_file, SHIP_TEMPLATES};
use callisto::AUTHORIZED_USERS;

const DEFAULT_SHIP_TEMPLATES_FILE: &str = "./scenarios/default_ship_templates.json";
const DEFAULT_AUTHORIZED_USERS_FILE: &str = "./scenarios/authorized_users.json";


/// Server to implement physically pseudo-realistic spaceflight and possibly combat.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Port for server to listen on
    #[arg(short, long, default_value_t = 3000)]
    port: u16,

    /// JSON file for planets in scenario
    #[arg(short, long)]
    scenario_file: Option<String>,

    /// JSON file for ship templates in scenario
    #[arg(short, long, default_value = DEFAULT_SHIP_TEMPLATES_FILE)]
    design_file: String,

    /// Run in test mode. Specifically, this will use a fixed random number generator.
    #[arg(short, long)]
    test: bool,

    // Name of the web server hosting the react app.
    #[arg(short, long, default_value = "http://localhost:50001")]
    web_server: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();

    let args = Args::parse();

    let port = args.port;
    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    let test_mode = args.test;

    let authorized_users = callisto::load_authorized_users_from_file(DEFAULT_AUTHORIZED_USERS_FILE)
        .expect("Unable to load authorized users file.");
    AUTHORIZED_USERS
        .set(authorized_users)
        .expect("(Main) attempting to set AUTHORIZED_USERS twice!");

    let templates = load_ship_templates_from_file(&args.design_file).unwrap_or_else(|e| panic!(
        "Unable to load ship template file {}. Reason {:?}",
        args.design_file,
        e
    ));
    
    SHIP_TEMPLATES
        .set(templates)
        .expect("(Main) attempting to set SHIP_TEMPLATES twice!");
    info!(
        "(main) Loaded authorized users: {:?}",
        AUTHORIZED_USERS.get().unwrap()
    );

    // Build the authenticator
    let mut authenticator = callisto::authentication::Authenticator::new(
        &args.web_server
    );

    authenticator.fetch_google_public_keys().await;

    let authenticator = Arc::new(authenticator);

    // Build the main entities table that will be the state of our server.
    let entities = Arc::new(Mutex::new(if let Some(file_name) = args.scenario_file {
        println!("Loading scenario file: {}", file_name);
        Entities::load_from_file(&file_name)
            .unwrap_or_else(|e| panic!("Issue loading scenario file {}: {}", file_name, e))
    } else {
        Entities::new()
    }));

    info!(
        "Starting with scenario entities: {:?}",
        entities.lock().unwrap()
    );

    println!("Starting Callisto server listening on address: {}", addr);

    // We create a TcpListener and bind it to 127.0.0.1:3000
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
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

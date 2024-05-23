use tokio::net::TcpListener;

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::TokioIo;

use clap::Parser;
use log::info;
use pretty_env_logger;

extern crate callisto;

use callisto::entity::Entities;
use callisto::handle_request;

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
}

fn load_scenario(file_name: &str) -> Result<Entities, Box<dyn std::error::Error>> {
    let file = std::fs::File::open(file_name)?;
    let reader = std::io::BufReader::new(file);
    let entities: Entities = serde_json::from_reader(reader)?;
    Ok(entities)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    pretty_env_logger::init();

    let args = Args::parse();

    let port = args.port;

    let addr = SocketAddr::from(([127, 0, 0, 1], port));

    // Build the main entities table that will be the state of our server.
    let entities = Arc::new(Mutex::new(if let Some(file_name) = args.scenario_file {
        println!("Loading scenario file: {}", file_name);
        load_scenario(&file_name)
            .expect(format!("Issue loading scenario file: {}", file_name).as_str())
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
        tokio::task::spawn(async move {
            let ent = e.clone();
            let handler = move |req| handle_request(req, ent.clone());

            // We bind the incoming connection to our service
            let builder = http1::Builder::new();
            if let Err(err) = builder.serve_connection(io, service_fn(handler)).await {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}

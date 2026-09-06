use std::collections::HashMap;
use std::net::ToSocketAddrs;
use std::panic;
use std::process;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

use futures::channel::mpsc::{channel, unbounded, UnboundedSender};
use futures::stream::{self, StreamExt};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::WebSocketStream;

// Things we use only when we are not using the `no_tls_upgrade` feature.
#[cfg(not(feature = "no_tls_upgrade"))]
use rustls::pki_types::pem::PemObject;
#[cfg(not(feature = "no_tls_upgrade"))]
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
#[cfg(not(feature = "no_tls_upgrade"))]
use std::path::PathBuf;
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_rustls::server::TlsStream;
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_rustls::TlsAcceptor;

use callisto::{debug, error, info, warn, LOG_FILE_USE};
use clap::Parser;
use tracing::{event, Level};
//use tracing_gcp::GcpLayer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter};

extern crate callisto;

use callisto::authentication::{
  load_user_directory, Authenticator, GoogleAuthenticator, HeaderCallback, MockAuthenticator, UserDirectory,
};

use callisto::entity::Entities;
use callisto::processor::{Processor, ReloadNotification};
use callisto::ship::DEFAULT_SHIP_TEMPLATES_DIR;
use callisto::ship::{load_ship_templates_from_dir, merge_ship_templates};
use callisto::{
  extract_scenario_owner, get_local_or_cloud_dir_fingerprint, read_local_or_cloud_file, replace_scenario_failures,
  replace_scenarios, ScenarioFailure, MAX_CONCURRENT_DIR_FILE_READS,
};

const DEFAULT_AUTHORIZED_USERS_FILE: &str = "./config/authorized_users.json";

const MAX_CHANNEL_DEPTH: usize = 10;
const RELOAD_POLL_INTERVAL: Duration = Duration::from_secs(5);

/// How many times a startup load from the scenario/design directory is
/// attempted before falling back to serving an empty registry.
const STARTUP_LOAD_ATTEMPTS: u32 = 5;

/// Delay before the second startup-load attempt. Doubles each retry, so five
/// attempts span roughly 3.75s of backoff in the worst case.
const STARTUP_LOAD_BACKOFF: Duration = Duration::from_millis(250);

/// Cap on a single startup-load attempt.
///
/// The failure this retries is not always a fast refusal: a cold-start GCS read
/// can hang. Canary was observed taking about four minutes per attempt, which
/// would let five attempts stall startup for twenty minutes. That matters
/// because the listener is bound before these loads run but does not accept
/// until after them, so Cloud Run routes traffic to an instance that answers
/// nothing. Timing out each attempt bounds the whole sequence to roughly a
/// minute, after which the caller's soft-fail path starts serving.
const STARTUP_LOAD_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(10);

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

  /// Directory for all possible scenarios
  #[arg(short, long, default_value = "./scenarios/")]
  scenario_dir: String,

  /// Directory holding ship-design templates. Each design lives in its own
  /// JSON file (single object, not array). Adding or changing a file
  /// triggers a watcher reload; removing a file is intentionally harmless
  /// (the in-memory copy persists).
  #[arg(short, long, default_value = DEFAULT_SHIP_TEMPLATES_DIR)]
  design_dir: String,

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
  #[arg(long, default_value = "keys/localhost.key")]
  tls_keys_private: String,

  // Prefix of the certificate and key files for tls.  The server will append .crt and .key to this.
  #[arg(long, default_value = "keys/localhost.crt")]
  tls_keys_public: String,

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
///   here because on connection we can see any `HttpCookie` on the request.  We use that in case a connection drops and reconnects so we don't need to force a re-login.
///
/// # Returns
/// A tuple of the websocket stream, the session key, and an optional email address.  The email address we `Some(email)` if this user has previously logged in.
async fn handle_connection(
  stream: TcpStream, #[cfg(not(feature = "no_tls_upgrade"))] acceptor: Arc<TlsAcceptor>,
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
      return Err(format!("(handle_connection) Failed to upgrade TcpStream to TLS: {e:?}"));
    }
  };

  // Second, upgrade the stream to use websockets with tungstenite
  // Tmp locked structure to get info out of the accept handler.
  // This is necessary because the callback_handler is consumed, so other approaches didn't work.
  // First element is the session key, second is the email.
  // HACK: Is there a better way to do this?  We just need to get this returned.  We don't actually need to create
  // this here. We also don't need to access it until the callback_handler is done.
  let auth_info = Arc::new(Mutex::new((String::new(), None)));

  let callback_handler = HeaderCallback {
    session_keys: session_keys.clone(),
    auth_info: auth_info.clone(),
  };

  let ws_stream: WebSocketStream<SubStream> = tokio_tungstenite::accept_hdr_async(stream, callback_handler)
    .await
    .map_err(|e| {
      error!("(handle_connection) Error during the websocket handshake occurred: {e}");
      format!("(handle_connection) Error during the websocket handshake occurred: {e}")
    })?;

  let auth_info = auth_info.lock().unwrap();
  Ok((ws_stream, auth_info.0.clone(), auth_info.1.clone()))
}

#[tokio::main]
#[quit::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
  let subscriber = tracing_subscriber::Registry::default()
    .with(EnvFilter::from_default_env())
    .with(tracing_stackdriver::layer());
  tracing::subscriber::set_global_default(subscriber)?;
  let args = Args::parse();
  let design_dir = args.design_dir.clone();
  let scenario_dir = args.scenario_dir.clone();

  let port = args.port;

  // DSN comes from SENTRY_DSN; SENTRY_ENVIRONMENT and SENTRY_RELEASE are auto-picked
  // up by the SDK. Skip init in --test mode so integration tests don't send events.
  let _sentry_guard = (!args.test).then(|| {
    sentry::init(sentry::ClientOptions {
      release: sentry::release_name!(),
      // Capture user IPs and potentially sensitive headers when using HTTP server integrations
      // see https://docs.sentry.io/platforms/rust/data-management/data-collected for more info
      send_default_pii: true,
      ..Default::default()
    })
  });

  let addr = (args.address.clone(), port)
    .to_socket_addrs()
    .expect("Unable to resolve the IP address for this server")
    .next()
    .expect("DNS resolution returned no IP addresses");
  info!(
    "START CALLISTO server listening on address: {}:{} {}",
    addr,
    port,
    if args.test { " (in TEST mode)" } else { "" }
  );

  // Load our certs and key.
  #[cfg(not(feature = "no_tls_upgrade"))]
  let cert_path = PathBuf::from(args.tls_keys_public.clone());
  #[cfg(not(feature = "no_tls_upgrade"))]
  let certs = CertificateDer::pem_file_iter(cert_path)?.collect::<Result<Vec<_>, _>>()?;
  #[cfg(not(feature = "no_tls_upgrade"))]
  let key_path = PathBuf::from(args.tls_keys_private);
  #[cfg(not(feature = "no_tls_upgrade"))]
  let key = PrivateKeyDer::from_pem_file(key_path)?;
  #[cfg(not(feature = "no_tls_upgrade"))]
  event!(target: LOG_FILE_USE, Level::INFO, file_name = &args.tls_keys_public, use = "Load certs and key.");

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
    info!("(main) Server in standard mode.  Referring frontend = {}", args.web_server);
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

  // Build the shared GCS client up front, before anything reads a directory.
  //
  // Every GCS read used to construct its own client, and each construction
  // fetches an OAuth token from the instance metadata server. Loading a
  // directory fans out over its files, so N designs meant N simultaneous token
  // requests; the metadata server refuses that burst. At 19 designs it failed
  // intermittently, at 79 it failed for over half of them every time.
  //
  // The client is now shared and cached, so warming it here means exactly one
  // token request happens, while the instance still has its startup CPU boost,
  // rather than racing a fan-out. Skipped entirely when no `gs://` path is
  // configured - local dev and the test suite have no credentials and never
  // touch GCS.
  //
  // A failure here is not fatal. It is retried on the lazy path, and the
  // scenario/design loads below keep their own soft-fail, so a genuine GCS
  // outage still cannot stop the instance coming up.
  if args.design_dir.starts_with("gs://") || args.scenario_dir.starts_with("gs://") {
    match retry_startup_load("GCS client", || Box::pin(callisto::ensure_gcs_client())).await {
      Ok(()) => {
        info!("(main) GCS client ready.");
      }
      Err(e) => {
        warn!("(main) Could not pre-build the GCS client: {e}. Falling back to building it on demand.");
      }
    }
  }

  debug!("(main) Loading ship templates from {}...", &args.design_dir);
  // Soft-fail like scenarios: a transient GCS hiccup on cold start
  // shouldn't wedge the instance. The watcher polls the directory
  // fingerprint and will retry on the next tick if the initial listing
  // failed. Per-file parse errors inside `load_ship_templates_from_dir`
  // are already swallowed and logged; the only error path here is the
  // listing itself failing.
  let initial_design_load_ok = match retry_startup_load("ship templates", || {
    Box::pin(load_ship_templates_from_dir(&args.design_dir))
  })
  .await
  {
    Ok(templates) => {
      let count = templates.len();
      merge_ship_templates(templates);
      event!(
        target: LOG_FILE_USE,
        Level::INFO,
        file_name = &args.design_dir,
        count,
        use = "Load ship templates."
      );
      true
    }
    Err(e) => {
      warn!(
        "(main) Initial ship-template load from {} failed: {e}. Starting with empty registry; the reload watcher will retry every {}s.",
        args.design_dir, RELOAD_POLL_INTERVAL.as_secs()
      );
      // Initialize the global to an empty map so callers don't panic on
      // first lookup. A subsequent successful watcher reload will merge in.
      merge_ship_templates(HashMap::<String, Arc<callisto::ship::ShipDesignTemplate>>::new());
      false
    }
  };

  // Don't crash the server if scenario loading fails — a transient GCS
  // metadata-server flake at cold start would otherwise take down a fresh
  // Cloud Run instance permanently. Log and start with empty scenarios; the
  // reload watcher (5s polling, fingerprint-based) is told to seed its
  // fingerprint as empty when the initial load failed, so the next poll
  // triggers a retry. This is the self-healing path.
  //
  // Per-file parse errors are NOT a load failure — the directory was readable,
  // so we publish the scenarios that did load and record the failed ones in
  // the failure registry. The reload watcher advances its fingerprint and only
  // retries when the bucket changes. Owners of broken files see a notice on
  // their next login.
  let initial_scenario_load_ok = match retry_startup_load("scenarios", || {
    Box::pin(load_scenarios_and_metadata(&args.scenario_dir))
  })
  .await
  {
    Ok(results) => {
      replace_scenarios(results.scenarios);
      replace_scenario_failures(results.failures);
      true
    }
    Err(e) => {
      warn!(
        "(main) Initial scenario load from {} failed: {e}. Starting with empty list; the reload watcher will retry every {}s.",
        args.scenario_dir, RELOAD_POLL_INTERVAL.as_secs()
      );
      replace_scenarios(Vec::new());
      replace_scenario_failures(Vec::new());
      false
    }
  };
  let (reload_sender, reload_receiver) = unbounded();
  // Clone for the users-file watcher we spawn below; the scenario/design
  // watcher takes the original.
  let users_reload_sender = reload_sender.clone();
  tokio::spawn(watch_reloadable_data(
    design_dir.clone(),
    scenario_dir.clone(),
    initial_scenario_load_ok,
    initial_design_load_ok,
    reload_sender,
  ));

  // Keep track of session keys (cookies) on connections.
  let session_keys = Arc::new(Mutex::new(HashMap::new()));

  let (mut connection_sender, connection_receiver) = channel(MAX_CHANNEL_DEPTH);

  // Shared user directory + register lock. Both must be created BEFORE the
  // authenticator so the GoogleAuthenticator constructor can seed the cell
  // and the Processor can read from the same Arc. The register lock is only
  // consumed by GoogleAuthenticator; mock mode owns its own swap discipline.
  let user_directory: Arc<RwLock<Arc<UserDirectory>>> = Arc::new(RwLock::new(Arc::new(UserDirectory::default())));

  // Seed the directory cell from the users file. In test mode the
  // GoogleAuthenticator path doesn't run, but tests still want to inject
  // pre-seeded blacklists by writing the file. Failure is non-fatal — an
  // empty cell behaves as "everyone allowed" under MockAuthenticator and
  // "nobody allowed" under GoogleAuthenticator (which then errors below).
  match load_user_directory(&args.users_file).await {
    Ok(initial) => {
      *user_directory.write().expect("(main) user_directory lock poisoned at startup") = Arc::new(initial);
      event!(target: LOG_FILE_USE, Level::INFO, file_name = args.users_file.as_str(), use = "Seed authorized users");
    }
    Err(e) => {
      warn!(
        "(main) Initial users-file load from {} failed: {e}. Starting with empty directory; the watcher will retry.",
        args.users_file
      );
    }
  }

  // Create an Authenticator to be cloned on each new connection.
  let auth_template: Box<dyn Authenticator> = if test_mode {
    Box::new(MockAuthenticator::new(&args.address).with_directory(user_directory.clone()))
  } else {
    // All the data shared between authenticators.
    let my_credentials = GoogleAuthenticator::load_google_credentials(&args.oauth_creds);

    let my_credentials = Arc::new(my_credentials);
    let google_keys = GoogleAuthenticator::fetch_google_public_keys().await;

    let register_lock = Arc::new(tokio::sync::Mutex::new(()));
    Box::new(
      GoogleAuthenticator::new(
        &args.web_server,
        my_credentials,
        google_keys,
        &args.users_file,
        user_directory.clone(),
        register_lock,
      )
      .await
      .expect("Failed to create GoogleAuthenticator"),
    )
  };

  // Spawn the users-file watcher (separate from the design/scenario watcher
  // because the polling path differs — single file, not a directory).
  let users_file_clone = args.users_file.clone();
  let user_directory_for_watcher = user_directory.clone();
  tokio::spawn(watch_users_file(
    users_file_clone,
    user_directory_for_watcher,
    users_reload_sender,
  ));

  let session_keys_clone = session_keys.clone();
  let directory_handle = user_directory.clone();
  tokio::task::spawn(async move {
    let mut processor = Processor::new(
      connection_receiver,
      reload_receiver,
      auth_template,
      session_keys_clone,
      &scenario_dir,
      test_mode,
      directory_handle,
    );
    processor.processor().await;
  });

  // Start a processor thread to handle all connections once established.

  #[cfg(feature = "no_tls_upgrade")]
  debug!("(main) No TLS upgrade enabled.");

  #[cfg(not(feature = "no_tls_upgrade"))]
  debug!("(main) TLS upgrade enabled.");

  info!("Starting Callisto server listening on address: {}", addr);

  // We start a loop to continuously accept incoming connections.  Once we have a connection
  // it gets upgraded (or fails) and then is sent to the master thread.
  // Eventually we'll have one such thread per server.
  loop {
    let (stream, peer_addr) = listener.accept().await?;

    debug!("(main) Accepted connection from {peer_addr}.");
    // Upgrade will be built differently depending on the feature `no_tls_upgrade`.
    #[cfg(feature = "no_tls_upgrade")]
    let upgrade: Result<(WebSocketStream<SubStream>, _, _), _> = handle_connection(stream, session_keys.clone()).await;

    #[cfg(not(feature = "no_tls_upgrade"))]
    let upgrade: Result<(WebSocketStream<SubStream>, _, _), _> =
      handle_connection(stream, acceptor.clone(), session_keys.clone()).await;

    match upgrade {
      Ok((ws_stream, session_key, email)) => {
        info!("(main) Successfully established websocket connection to {peer_addr}.");
        // Hardened: never unwrap here. If the processor task has died (or
        // the channel is full and the receive side is wedged), an `unwrap`
        // would crash the main accept loop too — turning a single worker
        // panic into a full container crash. Log and continue instead so
        // Cloud Run's healthcheck still sees a live process.
        if let Err(e) = connection_sender.try_send((ws_stream, session_key, email)) {
          error!(
            "(main) Could not hand websocket from {peer_addr} to processor (channel full or closed): {e}. Dropping connection."
          );
        }
      }
      Err(e) => {
        warn!("(main) Server at {addr} failed to establish websocket connection from {peer_addr}: {e}");
      }
    }
  }
}

async fn watch_reloadable_data(
  design_dir: String, scenario_dir: String, initial_scenario_load_ok: bool, initial_design_load_ok: bool,
  reload_sender: UnboundedSender<ReloadNotification>,
) {
  // Same self-healing fingerprint seed as scenarios: if the initial load
  // failed, seed the fingerprint as empty so the very next poll retries.
  let mut last_design_fingerprint = if initial_design_load_ok {
    get_local_or_cloud_dir_fingerprint(&design_dir).await.unwrap_or_else(|e| {
      warn!("(main) Unable to fingerprint design directory {design_dir}: {e}");
      Vec::new()
    })
  } else {
    Vec::new()
  };

  // If the initial scenario load in main() succeeded, seed the fingerprint
  // with the current bucket state so the first poll only reloads on real
  // changes. If it failed (e.g. cold-start GCS metadata-server flake),
  // seed empty so the very next poll triggers a retry — that's the
  // self-healing path: a transient failure clears once the metadata server
  // recovers, without needing the bucket to be touched.
  let mut last_scenario_fingerprint = if initial_scenario_load_ok {
    get_local_or_cloud_dir_fingerprint(&scenario_dir).await.unwrap_or_else(|e| {
      warn!("(main) Unable to fingerprint scenario directory {scenario_dir}: {e}");
      Vec::new()
    })
  } else {
    Vec::new()
  };

  loop {
    tokio::time::sleep(RELOAD_POLL_INTERVAL).await;

    let design_reload = match get_local_or_cloud_dir_fingerprint(&design_dir).await {
      Ok(fingerprint) if fingerprint != last_design_fingerprint => Some(fingerprint),
      Ok(_) => None,
      Err(e) => {
        warn!("(main) Unable to check design directory {design_dir}: {e}");
        None
      }
    };

    if let Some(fingerprint) = design_reload {
      match load_ship_templates_from_dir(&design_dir).await {
        Ok(templates) => {
          let count = templates.len();
          merge_ship_templates(templates);
          last_design_fingerprint = fingerprint;
          // Scenario parsing resolves every ship's `design` field against the
          // template registry, so any scenario parsed while that registry was
          // empty or incomplete failed with "Could not find design" and is now
          // cached as a failure. Because scenario reloads are gated on the
          // *scenario* directory fingerprint, those stale failures would
          // otherwise persist until someone touched the scenario bucket - which
          // is exactly what happens after a cold-start GCS auth flake leaves an
          // instance with an empty registry. Clearing the scenario fingerprint
          // forces a re-parse further down this same loop iteration, so the
          // scenarios recover as soon as the designs do. This also covers the
          // ordinary case of uploading a design a scenario was waiting on.
          last_scenario_fingerprint = Vec::new();
          event!(
            target: LOG_FILE_USE,
            Level::INFO,
            file_name = &design_dir,
            count,
            use = "Reloaded ship templates"
          );
          if let Err(e) = reload_sender.unbounded_send(ReloadNotification::ShipTemplates) {
            warn!("(main) Unable to notify processor of ship template reload: {e:?}");
            break;
          }
        }
        Err(e) => {
          warn!("(main) Unable to reload ship templates from {design_dir}: {e:?}");
        }
      }
    }

    let scenario_reload = match get_local_or_cloud_dir_fingerprint(&scenario_dir).await {
      Ok(fingerprint) if fingerprint != last_scenario_fingerprint => Some(fingerprint),
      Ok(_) => None,
      Err(e) => {
        warn!("(main) Unable to check scenario directory {scenario_dir}: {e}");
        None
      }
    };

    if let Some(fingerprint) = scenario_reload {
      match load_scenarios_and_metadata(&scenario_dir).await {
        Ok(results) => {
          replace_scenarios(results.scenarios);
          replace_scenario_failures(results.failures);
          last_scenario_fingerprint = fingerprint;
          event!(target: LOG_FILE_USE, Level::INFO, file_name = &scenario_dir, use = "Reloaded scenarios");
          if let Err(e) = reload_sender.unbounded_send(ReloadNotification::Scenarios) {
            warn!("(main) Unable to notify processor of scenario reload: {e:?}");
            break;
          }
        }
        Err(e) => {
          warn!("(main) Unable to reload scenarios from {scenario_dir}: {e:?}");
        }
      }
    }
  }
}

/// Poll the authorized-users file every `RELOAD_POLL_INTERVAL` and push a
/// `ReloadNotification::AuthorizedUsers` whenever it changes. Same shape as
/// the design-template watcher (one file, mtime-based) but runs on its own
/// task so a slow GCS poll on one watcher doesn't starve the other.
async fn watch_users_file(
  users_file: String, directory_handle: Arc<RwLock<Arc<UserDirectory>>>,
  reload_sender: UnboundedSender<ReloadNotification>,
) {
  // Seed last-known mtime from whatever the GoogleAuthenticator already
  // loaded. If the directory cell is still default (test mode), this will
  // be 0 and the first poll will pick the file up.
  let mut last_users_modified = directory_handle
    .read()
    .expect("(watch_users_file) directory_handle lock poisoned")
    .last_modified;

  loop {
    tokio::time::sleep(RELOAD_POLL_INTERVAL).await;

    let users_reload = match callisto::get_file_last_modified_timestamp(&users_file).await {
      Ok(Some(last_modified)) if last_modified > last_users_modified => Some(last_modified),
      Ok(_) => None,
      Err(e) => {
        warn!("(watch_users_file) Unable to check users file timestamp for {users_file}: {e}");
        None
      }
    };

    if let Some(last_modified) = users_reload {
      match load_user_directory(&users_file).await {
        Ok(new_dir) => {
          last_users_modified = last_modified;
          *directory_handle
            .write()
            .expect("(watch_users_file) directory_handle lock poisoned for write") = Arc::new(new_dir);
          event!(target: LOG_FILE_USE, Level::INFO, file_name = users_file.as_str(), use = "Reloaded authorized users");
          if let Err(e) = reload_sender.unbounded_send(ReloadNotification::AuthorizedUsers) {
            warn!("(watch_users_file) Unable to notify processor of users reload: {e:?}");
            break;
          }
        }
        Err(e) => {
          warn!("(watch_users_file) Unable to reload users file {users_file}: {e:?}");
        }
      }
    }
  }
}

fn join_dir_entry_path(dir: &str, entry: &str) -> String {
  format!("{}/{entry}", dir.trim_end_matches('/'))
}

/// Result of loading all scenarios in a directory.
///
/// Listing the directory is the only operation that can fail the whole call
/// — a transient GCS metadata-server flake there means we can't see any
/// files and must retry. Per-file parse errors are NOT fatal: they're
/// collected into `failures` so the caller can publish the successful
/// scenarios immediately and surface the broken ones to their owners. A
/// single bad scenario must never poison the entire catalog.
struct ScenarioLoadResults {
  scenarios: callisto::ScenarioMetadataList,
  failures: Vec<ScenarioFailure>,
}

/// Boxed future returned by a startup loader. Boxing keeps
/// [`retry_startup_load`] usable with closures that borrow their directory
/// argument, which a bare `impl Fn() -> impl Future` bound cannot express.
type StartupLoadFuture<'a, T> =
  std::pin::Pin<Box<dyn std::future::Future<Output = Result<T, Box<dyn std::error::Error>>> + 'a>>;

/// Run a startup load, retrying a bounded number of times with exponential
/// backoff before letting the caller fall back to its soft-fail path.
///
/// A cold-starting Cloud Run instance is sometimes refused a token by the
/// metadata server at 169.254.169.254, which fails the initial GCS read. The
/// instance then goes live with an empty registry: every scenario fails to
/// parse with "Could not find design" until the reload watcher heals it up to
/// `RELOAD_POLL_INTERVAL` later. The metadata server is normally ready within
/// a second or two, so retrying here closes that window rather than merely
/// recovering from it.
///
/// Each attempt is capped at [`STARTUP_LOAD_ATTEMPT_TIMEOUT`], because the
/// failure mode is often a hang rather than a refusal.
///
/// This deliberately does NOT replace the caller's soft-fail: if every attempt
/// fails the error is returned and the caller still starts with an empty
/// registry and self-heals, so a genuine GCS outage cannot wedge the instance.
/// On the happy path the first attempt succeeds and nothing is added.
async fn retry_startup_load<'a, T>(
  what: &str, mut load: impl FnMut() -> StartupLoadFuture<'a, T>,
) -> Result<T, Box<dyn std::error::Error>> {
  let mut backoff = STARTUP_LOAD_BACKOFF;
  let mut last_err = None;
  for attempt in 1..=STARTUP_LOAD_ATTEMPTS {
    // Bound each attempt: a hung read is indistinguishable from a slow one, and
    // without this a stalled GCS call blocks startup for minutes at a time.
    let outcome = match tokio::time::timeout(STARTUP_LOAD_ATTEMPT_TIMEOUT, load()).await {
      Ok(result) => result,
      Err(_) => Err(format!("attempt exceeded {}s", STARTUP_LOAD_ATTEMPT_TIMEOUT.as_secs()).into()),
    };
    match outcome {
      Ok(value) => {
        if attempt > 1 {
          warn!("(main) Startup load of {what} succeeded on attempt {attempt}.");
        }
        return Ok(value);
      }
      Err(e) => {
        if attempt < STARTUP_LOAD_ATTEMPTS {
          warn!(
            "(main) Startup load of {what} failed on attempt {attempt}/{STARTUP_LOAD_ATTEMPTS}: {e}. Retrying in {}ms.",
            backoff.as_millis()
          );
          tokio::time::sleep(backoff).await;
          backoff *= 2;
        }
        last_err = Some(e);
      }
    }
  }
  Err(last_err.unwrap_or_else(|| "no attempts made".into()))
}

async fn load_scenarios_and_metadata(scenario_dir: &str) -> Result<ScenarioLoadResults, Box<dyn std::error::Error>> {
  let scenarios_list = callisto::list_local_or_cloud_dir(scenario_dir).await?;

  event!(target: LOG_FILE_USE, Level::INFO, file_name = &scenario_dir, use = "Loaded scenario");

  // Per-file: read the bytes first so we can attempt owner extraction even
  // when the full parse fails — the parse error usually fires deep in ship
  // deserialization (e.g. "Could not find design X"), but `metadata.owner`
  // sits at the top of the JSON and is almost always still readable.
  //
  // Bounded fan-out: see `MAX_CONCURRENT_DIR_FILE_READS`. Reading all of them at
  // once against GCS makes a handful fail at the transport layer every time.
  // Completion order is not preserved, which is fine — nothing downstream
  // depends on scenario ordering.
  let results = stream::iter(scenarios_list)
    .map(|scenario| {
      // Everything the future needs is resolved to owned data here, outside the
      // `async` block. A future that borrowed the closure's argument would make
      // its type depend on that borrow's lifetime, which `buffer_unordered`
      // cannot name.
      let scenario_path = join_dir_entry_path(scenario_dir, &scenario);
      async move {
        let bytes = match read_local_or_cloud_file(&scenario_path).await {
          Ok(b) => b,
          Err(e) => {
            error!("ERROR: Failed to read scenario file '{scenario_path}': {e}");
            return Err(ScenarioFailure {
              filename: scenario,
              owner: String::new(),
              error: format!("Failed to read file: {e}"),
            });
          }
        };
        match Entities::load_from_bytes(&bytes, &scenario_path) {
          Ok(entities) => Ok((scenario, entities.metadata.clone())),
          Err(e) => {
            let owner = extract_scenario_owner(&bytes);
            error!("ERROR: Failed to parse scenario file '{scenario_path}' (owner='{owner}'): {e}");
            Err(ScenarioFailure {
              filename: scenario,
              owner,
              error: e.to_string(),
            })
          }
        }
      }
    })
    .buffer_unordered(MAX_CONCURRENT_DIR_FILE_READS)
    .collect::<Vec<_>>()
    .await;

  let mut scenarios = callisto::ScenarioMetadataList::with_capacity(results.len());
  let mut failures = Vec::new();
  for r in results {
    match r {
      Ok(s) => scenarios.push(s),
      Err(f) => failures.push(f),
    }
  }

  if !failures.is_empty() {
    warn!(
      "(load_scenarios_and_metadata) Loaded {} of {} scenario file(s); {} failed.",
      scenarios.len(),
      scenarios.len() + failures.len(),
      failures.len()
    );
  }

  Ok(ScenarioLoadResults { scenarios, failures })
}

#[cfg(test)]
mod tests {
  use super::{retry_startup_load, STARTUP_LOAD_ATTEMPTS, STARTUP_LOAD_ATTEMPT_TIMEOUT};
  use std::cell::Cell;

  /// The happy path must not retry or sleep: one call, one success.
  #[tokio::test(start_paused = true)]
  async fn retry_startup_load_succeeds_first_try() {
    let calls = Cell::new(0);
    let result: Result<u32, _> = retry_startup_load("test", || {
      calls.set(calls.get() + 1);
      Box::pin(async { Ok(7) })
    })
    .await;
    assert_eq!(result.unwrap(), 7);
    assert_eq!(calls.get(), 1, "Happy path must not retry");
  }

  /// The case this exists for: the metadata server refuses a token for the
  /// first moment of a cold start, then recovers.
  #[tokio::test(start_paused = true)]
  async fn retry_startup_load_recovers_after_transient_failures() {
    let calls = Cell::new(0);
    let result: Result<u32, _> = retry_startup_load("test", || {
      calls.set(calls.get() + 1);
      let n = calls.get();
      Box::pin(async move {
        if n < 3 {
          Err("metadata server not ready".into())
        } else {
          Ok(42)
        }
      })
    })
    .await;
    assert_eq!(result.unwrap(), 42);
    assert_eq!(calls.get(), 3, "Should stop retrying once it succeeds");
  }

  /// A hung load must be abandoned rather than waited on. This is the case
  /// that motivated the per-attempt timeout: canary was seen spending about
  /// four minutes inside a single GCS read, which would stall startup for
  /// twenty minutes across five attempts.
  #[tokio::test(start_paused = true)]
  async fn retry_startup_load_times_out_a_hung_attempt() {
    let calls = Cell::new(0);
    let start = tokio::time::Instant::now();
    let result: Result<u32, _> = retry_startup_load("test", || {
      calls.set(calls.get() + 1);
      let n = calls.get();
      Box::pin(async move {
        if n == 1 {
          // Hangs far longer than the per-attempt cap.
          tokio::time::sleep(STARTUP_LOAD_ATTEMPT_TIMEOUT * 100).await;
          Ok(0)
        } else {
          Ok(99)
        }
      })
    })
    .await;
    assert_eq!(result.unwrap(), 99, "Should abandon the hung attempt and retry");
    assert_eq!(calls.get(), 2);
    assert!(
      start.elapsed() < STARTUP_LOAD_ATTEMPT_TIMEOUT * 2,
      "Gave up on the hung attempt too late: {:?}",
      start.elapsed()
    );
  }

  /// A real outage must still give up so the caller can fall back to its
  /// soft-fail path rather than blocking startup forever.
  #[tokio::test(start_paused = true)]
  async fn retry_startup_load_gives_up_and_returns_error() {
    let calls = Cell::new(0);
    let result: Result<u32, _> = retry_startup_load("test", || {
      calls.set(calls.get() + 1);
      Box::pin(async { Err("gcs down".into()) })
    })
    .await;
    assert!(result.is_err(), "Persistent failure must surface as an error");
    assert_eq!(
      calls.get(),
      i32::try_from(STARTUP_LOAD_ATTEMPTS).unwrap(),
      "Should make exactly STARTUP_LOAD_ATTEMPTS attempts"
    );
  }
}

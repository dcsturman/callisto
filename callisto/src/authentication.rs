use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use once_cell::sync::Lazy;
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::error::Error;
use std::fmt::Debug;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use dyn_clone::DynClone;
use tracing::{event, Level};

use tokio_tungstenite::tungstenite::handshake::server::{Callback, ErrorResponse, Request, Response};

use crate::{
  get_file_last_modified_timestamp, read_local_or_cloud_file_with_generation,
  write_local_or_cloud_file_if_generation_match, GenerationWriteError, LOG_AUTH_RESULT,
};

#[allow(unused_imports)]
use crate::{debug, error, info, warn, LOG_FILE_USE};

type GoogleProfile = String;

const GOOGLE_X509_CERT_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const SESSION_COOKIE_NAME: &str = "callisto-session-key";
const COOKIE_ID: &str = "cookie";

/// Maximum number of attempts for a generation-guarded write before giving up.
const REGISTER_WRITE_MAX_ATTEMPTS: u32 = 5;

/// Per-user account status. Stored in the authorized-users file alongside
/// timestamps. `Active` users may log in and play; `Blacklisted` users are
/// kicked at every gate (Login, Register, and on watcher-driven reload).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserStatus {
  Active,
  Blacklisted,
}

/// On-disk record for a single user. The file is a JSON object with
/// `version: 1` and a `users` array of these. Legacy `Vec<String>` files
/// are accepted on read and synthesized into records with
/// `status: Active, registered_at: 0`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
  pub email: String,
  pub status: UserStatus,
  pub registered_at: i64,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  pub blacklisted_at: Option<i64>,
}

/// In-memory snapshot of the authorized-users file. `active`/`blacklisted`
/// are derived sets for O(1) gating; `raw` preserves the original ordering
/// for re-serialization. `generation` is the GCS object generation (or
/// `None` for local files / first-write); `last_modified` is the file's
/// mtime (used by the watcher to detect external changes).
#[derive(Debug, Default, Clone)]
pub struct UserDirectory {
  pub active: HashSet<String>,
  pub blacklisted: HashSet<String>,
  pub raw: Vec<UserRecord>,
  pub generation: Option<i64>,
  pub last_modified: i64,
}

impl UserDirectory {
  #[must_use]
  pub fn is_blacklisted(&self, email: &str) -> bool {
    self.blacklisted.contains(&email.to_lowercase())
  }

  #[must_use]
  pub fn is_active(&self, email: &str) -> bool {
    self.active.contains(&email.to_lowercase())
  }
}

#[derive(Debug, Serialize, Deserialize)]
struct UsersFileV1 {
  version: u32,
  users: Vec<UserRecord>,
}

/// Parse the authorized-users file body. Only the V1 shape is accepted.
/// The legacy `Vec<String>` format must be migrated via the bundled CLI
/// (`scripts/migrate-users-file.sh <users-file>`) before the server will
/// start.
fn parse_user_directory_body(body: &[u8]) -> Result<Vec<UserRecord>, Box<dyn std::error::Error>> {
  if body.is_empty() {
    return Ok(Vec::new());
  }
  match serde_json::from_slice::<UsersFileV1>(body) {
    Ok(file) => Ok(file.users),
    Err(v1_err) => {
      // Detect the legacy shape only to give an actionable error message.
      if serde_json::from_slice::<Vec<String>>(body).is_ok() {
        Err(
          "Legacy users file format detected. \
           Run `scripts/migrate-users-file.sh <users-file>` to promote it \
           to the V1 format before starting the server."
            .into(),
        )
      } else {
        Err(Box::new(v1_err))
      }
    }
  }
}

fn build_directory_from_records(
  records: Vec<UserRecord>, generation: Option<i64>, last_modified: i64,
) -> UserDirectory {
  let mut active = HashSet::new();
  let mut blacklisted = HashSet::new();
  let mut raw = Vec::with_capacity(records.len());
  for record in records {
    let lower = UserRecord {
      email: record.email.to_lowercase(),
      ..record
    };
    match lower.status {
      UserStatus::Active => {
        active.insert(lower.email.clone());
      }
      UserStatus::Blacklisted => {
        blacklisted.insert(lower.email.clone());
      }
    }
    raw.push(lower);
  }
  UserDirectory {
    active,
    blacklisted,
    raw,
    generation,
    last_modified,
  }
}

/// Load the authorized-users file from disk or GCS into a `UserDirectory`.
///
/// # Errors
/// Returns an error if the file cannot be read or if both the v1 and legacy
/// shapes fail to parse.
pub async fn load_user_directory(filename: &str) -> Result<UserDirectory, Box<dyn std::error::Error>> {
  let (body, generation) = read_local_or_cloud_file_with_generation(filename).await?;
  let last_modified = get_file_last_modified_timestamp(filename).await?.unwrap_or(0);
  let records = parse_user_directory_body(&body)?;
  Ok(build_directory_from_records(records, generation, last_modified))
}

/// Trait defining the authentication behavior for the application
#[async_trait]
pub trait Authenticator: Send + Sync + DynClone + Debug {
  /// Returns the web server URL
  fn get_web_server(&self) -> String;

  /// Authenticates a Google user with the provided code
  /// Returns a tuple of `(session_key, user_profile)` on success.
  async fn authenticate_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>>;

  /// Register a new user. Mirrors `authenticate_user`'s `(code, session_keys)`
  /// shape so the wire-side `Login` / `Register` paths share the same client
  /// payload (`LoginMsg { code }`). On success, the email is added to the
  /// authorized-users directory and the session key is recorded.
  async fn register_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>>;

  /// Checks if this socket has been validated, i.e. has successfully logged in.
  /// # Returns
  /// `true` if the user is validated, `false` otherwise.
  fn validated_user(&self) -> bool;

  fn set_email(&mut self, email: Option<&String>);
  fn set_session_key(&mut self, session_key: &str);
  fn get_email(&self) -> Option<String>;
  fn get_session_key(&self) -> Option<String>;

  /// Snapshot of the live user directory, if this authenticator has one.
  /// Default implementation returns `None` (e.g. legacy mocks without state).
  fn directory_snapshot(&self) -> Option<Arc<UserDirectory>> {
    None
  }
}

/// Tagged error used by `register_user` so the processor can map to the
/// pinned wire strings the FE branches on. The `Display` impl IS the wire
/// contract — keep the strings exact.
#[derive(Debug)]
pub enum RegisterError {
  /// Blacklisted email address tried to register.
  NotAuthorized,
  /// Email is already registered as active.
  AlreadyRegistered,
  /// Google JWT validation failed (bad code, expired token, etc.).
  AuthFailed,
  /// Generation-guarded write retries were exhausted.
  WriteFailed,
}

impl std::fmt::Display for RegisterError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let s = match self {
      RegisterError::NotAuthorized => "NOT_AUTHORIZED",
      RegisterError::AlreadyRegistered => "ALREADY_REGISTERED",
      RegisterError::AuthFailed => "AUTH_FAILED",
      RegisterError::WriteFailed => "REGISTRATION_FAILED",
    };
    write!(f, "{s}")
  }
}

impl Error for RegisterError {}

/// Load the list of active authorized users from a file. Wrapper around the
/// shared parser that flattens the new directory shape down to "just the
/// active emails" for any remaining caller that doesn't care about status.
///
/// New code should prefer [`load_user_directory`] which exposes both the
/// active set and the blacklist.
///
/// # Arguments
/// * `users_file` - The name of the file to load.
///
/// # Returns
/// A list of all the active authorized users. If the file is invalid, then
/// note the warning and return the empty list.
///
/// # Panics
/// If the file cannot be read.
pub async fn load_authorized_users(users_file: &str) -> Vec<String> {
  let directory = load_user_directory(users_file).await.unwrap_or_else(|_| {
    panic!("(load_authorized_users) Unable to load authorized users file. No such file or directory '{users_file}'.");
  });
  info!(
    "(Authentication.load_authorized_users) Loaded {} active authorized users from file {}",
    directory.active.len(),
    users_file
  );
  directory.active.into_iter().collect()
}

pub struct HeaderCallback {
  pub session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
  // First is the session key, second is the email
  pub auth_info: Arc<Mutex<(String, Option<String>)>>,
}

impl Callback for HeaderCallback {
  fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
    let (new_response, new_session_key, new_email) = generic_on_request(&self.session_keys, request, response);
    *self.auth_info.lock().unwrap() = (new_session_key, new_email);
    Ok(new_response)
  }
}

#[derive(Debug, Clone)]
pub struct GoogleAuthenticator {
  /**
   * The Google credentials for this server's domain (for oauth2).
   */
  credentials: Arc<GoogleCredentials>,
  /**
   * The email address of the user.  If None, the user hasn't logged in yet.
   */
  email: Option<String>,
  /**
   * The session key for this user.  If None, the user hasn't logged in yet.
   */
  session_key: Option<String>,
  /**
   * The URL of the web server (front end).
   */
  web_server: String,
  /**
   * The Google public keys.  These are used to validate the Google tokens.
   */
  google_keys: Arc<GooglePublicKeys>,
  /**
   * Shared, hot-reloadable user directory. Every per-connection clone of
   * this authenticator points at the same backing `RwLock<Arc<...>>`, so a
   * successful register-write swaps the inner `Arc` and is immediately
   * visible to all sessions. (This is the per-clone-Arc bug fix from the
   * old `authorized_users: Arc<Vec<String>>` design.)
   */
  directory: Arc<RwLock<Arc<UserDirectory>>>,
  /**
   * Serializes concurrent `register_user` calls within this replica. Single
   * deployment topology + GCS generation preconditions handle inter-replica
   * concurrency; this lock just coalesces same-process registrations so we
   * don't burn retry budget on self-conflicts.
   */
  register_lock: Arc<tokio::sync::Mutex<()>>,
  /**
   * The path to the authorized users file.
   */
  authorized_users_file: String,
}

impl GoogleAuthenticator {
  /// Creates a new `GoogleAuthenticator` instance
  ///
  /// # Arguments
  /// * `web_server` - The URL of the web server (front end).
  /// * `credentials` - The Google credentials for this server's domain (for oauth2).
  /// * `google_keys` - A copy of the previously fetched Google public keys.
  /// * `authorized_users_file` - The path to the authorized users file.
  /// * `directory` - Shared user directory (reload-cell). Cloned authenticators
  ///   point at the same cell so a register-write is visible everywhere.
  /// * `register_lock` - Shared lock used to serialize registrations.
  ///
  /// # Errors
  /// Returns `Err` if the authorized users file cannot be read or parsed.
  ///
  /// # Panics
  /// Panics if the directory `RwLock` is poisoned (process startup; never
  /// expected in normal operation).
  pub async fn new(
    web_server: &str, credentials: Arc<GoogleCredentials>, google_keys: Arc<GooglePublicKeys>,
    authorized_users_file: &str, directory: Arc<RwLock<Arc<UserDirectory>>>,
    register_lock: Arc<tokio::sync::Mutex<()>>,
  ) -> Result<Self, Box<dyn std::error::Error>> {
    // Seed the directory cell from the file. Any later changes (CLI edits,
    // register_user) will update the same cell.
    let initial = load_user_directory(authorized_users_file).await?;
    *directory.write().expect("(GoogleAuthenticator.new) directory lock poisoned") = Arc::new(initial);

    Ok(GoogleAuthenticator {
      credentials,
      email: None,
      session_key: None,
      directory,
      register_lock,
      google_keys,
      web_server: web_server.to_string(),
      authorized_users_file: authorized_users_file.to_string(),
    })
  }

  /// Snapshot of the currently-loaded directory.
  fn directory(&self) -> Arc<UserDirectory> {
    self
      .directory
      .read()
      .expect("(GoogleAuthenticator.directory) directory lock poisoned")
      .clone()
  }

  /// Reload the directory if the source file has been touched since the last
  /// load. Returns the latest snapshot regardless of whether a reload happened.
  async fn maybe_reload_user_directory(&self) -> Arc<UserDirectory> {
    let last_modified = match get_file_last_modified_timestamp(&self.authorized_users_file).await {
      Ok(last_modified) => last_modified,
      Err(e) => {
        warn!(
          "(maybe_reload_user_directory) Unable to check authorized users file {}: {e}",
          self.authorized_users_file
        );
        return self.directory();
      }
    };

    let current = self.directory();
    if let Some(last_modified) = last_modified {
      if last_modified > current.last_modified {
        match load_user_directory(&self.authorized_users_file).await {
          Ok(new_dir) => {
            let new_arc = Arc::new(new_dir);
            *self
              .directory
              .write()
              .expect("(maybe_reload_user_directory) directory lock poisoned") = new_arc.clone();
            event!(target: LOG_FILE_USE, Level::INFO, file_name = &self.authorized_users_file, use = "Reloaded authorized users");
            return new_arc;
          }
          Err(e) => {
            warn!(
              "(maybe_reload_user_directory) Unable to reload authorized users from file {}: {e}",
              self.authorized_users_file
            );
          }
        }
      }
    }
    current
  }

  /// Validate a Google authorization code, returning the email claim. Shared
  /// between `authenticate_user` and `register_user` so JWT semantics stay
  /// identical at both gates.
  async fn validate_google_token(&self, code: &str) -> Result<String, Box<dyn Error>> {
    const GRANT_TYPE: &str = "authorization_code";
    let redirect_uri = self.get_web_server();

    let token_request = [
      ("code", &code.to_string()),
      ("client_id", &self.credentials.client_id.clone()),
      ("client_secret", &self.credentials.client_secret.clone()),
      ("redirect_uri", &redirect_uri.clone()),
      ("access_type", &"offline".to_string()),
      ("grant_type", &GRANT_TYPE.to_string()),
    ];

    debug!(
      "(validate_google_token) Make request of Google with client_id {:?}, redirect_uri {:?}.",
      self.credentials.client_id, redirect_uri
    );

    let client = reqwest::Client::new();

    let token_response = client
      .post(&self.credentials.token_uri)
      .form(&token_request)
      .send()
      .await
      .expect("(validate_google_token) Unable to fetch Google token");

    debug!("(validate_google_token) Fetched token response.");
    let body = token_response
      .text()
      .await
      .unwrap_or_else(|e| panic!("(validate_google_token) Unable to get text from token response: {e:?}"));

    let token_response_json: GoogleTokenResponse = serde_json::from_str(&body).map_err(|e| {
      error!("(validate_google_token) Unable to parse token response: {e:?} {body}");
      e
    })?;

    let token = token_response_json.id_token;

    let header =
      decode_header(&token).unwrap_or_else(|e| panic!("(validate_google_token) Unable to decode token header: {e:?}"));
    let kid = header
      .kid
      .unwrap_or_else(|| panic!("(validate_google_token) Unable to get key ID from token header"));

    let public_key = self
      .google_keys
      .keys
      .iter()
      .find(|k| k.kid == kid)
      .ok_or("No matching public key found")?;

    let decoding_key = DecodingKey::from_rsa_components(&public_key.n, &public_key.e).unwrap();

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(std::slice::from_ref(&self.credentials.client_id));
    validation.set_issuer(&["https://accounts.google.com"]);

    let token_data = decode::<GoogleClaims>(&token, &decoding_key, &validation)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if token_data.claims.exp < now {
      debug!(
        "(validate_google_token) Token expired at time {} vs now {}",
        token_data.claims.exp, now
      );
      return Err(Box::new(TokenTimeoutError {}));
    }
    Ok(token_data.claims.email.to_lowercase())
  }

  /// Register a new user. See trait docs.
  ///
  /// # Errors
  /// Returns `RegisterError` mapped to the wire-pinned strings.
  ///
  /// # Panics
  /// Panics if the directory `RwLock` or `session_keys` mutex is poisoned.
  #[allow(clippy::too_many_lines)]
  pub async fn register(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<String, RegisterError> {
    // 1. Validate token first — outside the lock — so a bad token doesn't hold
    //    up the next concurrent registration.
    let email = self.validate_google_token(code).await.map_err(|e| {
      warn!("(register) Token validation failed: {e:?}");
      RegisterError::AuthFailed
    })?;

    // 2. Acquire the register lock to serialize concurrent registrations within
    //    this replica.
    let _guard = self.register_lock.lock().await;

    // 3. Reload the directory under the lock so we see CLI edits.
    let current = self.maybe_reload_user_directory().await;
    if current.is_blacklisted(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Blacklisted");
      return Err(RegisterError::NotAuthorized);
    }
    if current.is_active(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "AlreadyRegistered");
      return Err(RegisterError::AlreadyRegistered);
    }

    // 4. Retry loop: on 412, re-read, re-check predicates, re-serialize.
    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));

    let mut latest = current;
    for attempt in 1..=REGISTER_WRITE_MAX_ATTEMPTS {
      // Build the next file body from the latest snapshot.
      let mut new_records: Vec<UserRecord> = latest.raw.clone();
      new_records.push(UserRecord {
        email: email.clone(),
        status: UserStatus::Active,
        registered_at: now,
        blacklisted_at: None,
      });
      let file = UsersFileV1 {
        version: 1,
        users: new_records.clone(),
      };
      let bytes = match serde_json::to_vec_pretty(&file) {
        Ok(b) => b,
        Err(e) => {
          error!("(register) Unable to serialize users file: {e:?}");
          return Err(RegisterError::WriteFailed);
        }
      };

      match write_local_or_cloud_file_if_generation_match(&self.authorized_users_file, bytes, latest.generation).await {
        Ok(new_generation) => {
          // Build a fresh directory in-memory (avoid an extra read round-trip)
          // and atomically swap the cell so all clones see it.
          let mtime = get_file_last_modified_timestamp(&self.authorized_users_file)
            .await
            .ok()
            .and_then(|t| t)
            .unwrap_or(now);
          let new_dir = build_directory_from_records(new_records, Some(new_generation), mtime);
          *self.directory.write().expect("(register) directory lock poisoned") = Arc::new(new_dir);

          // Record the session key for the newly registered user. Mirrors the
          // login flow.
          if let Some(key) = self.session_key.clone() {
            session_keys.lock().unwrap().insert(key, Some(email.clone()));
          } else {
            warn!("(register) No session key on this connection.");
          }

          event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Registered");
          self.email = Some(email.clone());
          return Ok(email);
        }
        Err(GenerationWriteError::PreconditionFailed) => {
          warn!(
            "(register) GCS generation precondition failed on attempt {attempt}/{REGISTER_WRITE_MAX_ATTEMPTS}; re-reading."
          );
          // Backoff then re-read.
          tokio::time::sleep(Duration::from_millis(50 * u64::from(attempt))).await;
          match load_user_directory(&self.authorized_users_file).await {
            Ok(new_dir) => {
              let new_arc = Arc::new(new_dir);
              *self.directory.write().expect("(register) directory lock poisoned") = new_arc.clone();
              latest = new_arc;
            }
            Err(e) => {
              error!("(register) Unable to re-read users file after 412: {e:?}");
              return Err(RegisterError::WriteFailed);
            }
          }
          // Re-check predicates against the freshly read directory.
          if latest.is_blacklisted(&email) {
            event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Blacklisted");
            return Err(RegisterError::NotAuthorized);
          }
          if latest.is_active(&email) {
            event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "AlreadyRegistered");
            return Err(RegisterError::AlreadyRegistered);
          }
        }
        Err(GenerationWriteError::Other(e)) => {
          error!("(register) Write failed for {}: {e}", self.authorized_users_file);
          return Err(RegisterError::WriteFailed);
        }
      }
    }

    error!(
      "(register) Exhausted {REGISTER_WRITE_MAX_ATTEMPTS} retries trying to register {email} in {}.",
      self.authorized_users_file
    );
    Err(RegisterError::WriteFailed)
  }

  // Static helper methods
  /**
   * Fetches the Google public keys from the Google API (public internet).  These are hosted at a well known
   * public URL.
   *
   * # Returns
   * The Google public keys.
   *
   * # Panics
   * If the keys cannot be fetched or parsed.
   */
  pub async fn fetch_google_public_keys() -> Arc<GooglePublicKeys> {
    // Fetch Google's public keys
    let client = reqwest::Client::new();
    let public_keys_response = client
      .get(GOOGLE_X509_CERT_URL)
      .send()
      .await
      .expect("(validate_google_token) Unable to fetch Google public keys");

    debug!("(validate_google_token) Fetched Google public keys.",);

    let text = public_keys_response.text().await.unwrap();

    debug!("(validate_google_token) Fetched Google public keys okay.");

    Arc::new(
      serde_json::from_str::<GooglePublicKeys>(&text)
        .unwrap_or_else(|e| panic!("(validate_google_token) Error: Unable to parse Google public keys: {e:?}")),
    )
  }

  /// Load the oauth credentials for this server's domain from a file.
  ///
  /// # Arguments
  /// * `file_name` - The name of the file to load.
  ///
  /// # Returns
  /// The Google issued oauth credentials.
  ///
  /// # Panics
  /// If the file cannot be read or the credentials are malformed (cannot be parsed).
  #[must_use]
  pub fn load_google_credentials(file_name: &str) -> GoogleCredentials {
    let file = std::fs::File::open(file_name)
      .unwrap_or_else(|e| panic!("Error {e:?} opening Google credentials file {file_name}"));
    let reader = std::io::BufReader::new(file);
    let credentials: GoogleCredsJson = serde_json::from_reader(reader)
      .unwrap_or_else(|e| panic!("Error {e:?} parsing Google credentials file {file_name}"));
    debug!("Load Google credentials file \"{}\".", file_name);
    credentials.web
  }
}

fn generic_on_request(
  session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>, request: &Request, response: Response,
) -> (Response, String, Option<String>) {
  let cookies = request
    .headers()
    .iter()
    .filter_map(|(key, value)| {
      if key.as_str() == COOKIE_ID {
        Some(value.to_str().unwrap().to_string())
      } else {
        None
      }
      .and_then(|cookie_header| {
        // Split the cookie header by "; " to get individual cookies
        cookie_header.split("; ").find_map(|cookie| {
          cookie
            .strip_prefix(&format!("{SESSION_COOKIE_NAME}="))
            .map(std::string::ToString::to_string)
        })
      })
    })
    .collect::<Vec<String>>();

  // If we found a cookie, find if there's a valid email address to go with it.
  // This happens in the case where we get disconnected and the client reconnects.
  let valid_pair = {
    let unlocked_session_keys = session_keys.lock().expect("Unable to get lock session keys.");

    let mut emails = cookies
      .iter()
      .filter_map(|cookie| {
        unlocked_session_keys
          .get(cookie)
          .and_then(|email| email.as_ref().map(|email| (cookie.clone(), email.clone())))
      })
      .collect::<Vec<(String, String)>>();

    if emails.len() > 1 {
      warn!("(on_request) Found multiple valid emails for session key.");
    }
    emails.pop()
  };

  if let Some((key, email)) = valid_pair {
    // We have a logged in user with a valid email, so record that.
    debug!("(on_request) Found valid email {}", email,);
    (response, key.clone(), Some(email.clone()))
  } else if !cookies.is_empty() {
    // We have cookies but they don't have a valid email yet (e.g., reconnection after server restart or logout).
    // Reuse the first cookie's session key instead of creating a new one.
    // This allows the client to maintain the same session key across reconnections.
    debug!(
      "(on_request) Found {} cookie(s) but none have valid email. Reusing first cookie.",
      cookies.len()
    );
    let session_key = cookies[0].clone();

    // Ensure the session key is in the map (it might not be if this is a fresh reconnection)
    let mut session_keys_lock = session_keys.lock().expect("Unable to get lock session keys.");
    session_keys_lock.entry(session_key.clone()).or_insert(None);
    drop(session_keys_lock);

    (response, session_key, None)
  } else {
    // No cookies found, create a new session key.
    debug!("(on_request) No cookies found. Creating new session key.");
    let mut response = response.clone();

    let session_key = generate_session_key();
    session_keys
      .lock()
      .expect("Unable to get lock session keys.")
      .insert(session_key.clone(), None);

    let cookie_value = if cfg!(feature = "no_tls_upgrade") {
      // For local development without TLS, don't set Secure flag
      format!("{SESSION_COOKIE_NAME}={session_key}; HttpOnly; SameSite=Lax")
    } else {
      // For production with TLS, set Secure flag
      format!("{SESSION_COOKIE_NAME}={session_key}; HttpOnly; SameSite=None; Secure")
    };

    response.headers_mut().insert("Set-Cookie", cookie_value.parse().unwrap());

    (response, session_key, None)
  }
}

#[async_trait]
impl Authenticator for GoogleAuthenticator {
  fn get_web_server(&self) -> String {
    self.web_server.clone()
  }

  fn get_email(&self) -> Option<String> {
    self.email.clone()
  }

  fn get_session_key(&self) -> Option<String> {
    self.session_key.clone()
  }

  fn set_email(&mut self, email: Option<&String>) {
    self.email = email.cloned();
  }

  fn set_session_key(&mut self, session_key: &str) {
    self.session_key = Some(session_key.to_string());
  }

  async fn authenticate_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>> {
    // Reload the directory before checking — we want CLI edits (including
    // mid-session blacklists) to bite the next login attempt.
    let directory = self.maybe_reload_user_directory().await;

    // Validate token, lowercase the email.
    let email = self.validate_google_token(code).await?;

    // Blacklist takes precedence over "not in active list" — must come first.
    if directory.is_blacklisted(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Blacklisted");
      return Err(Box::new(BlacklistedUserError {}));
    }

    if !directory.is_active(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Failure");
      return Err(Box::new(UnauthorizedUserError {}));
    }

    // Associate with session key.
    if self.session_key.is_none() {
      warn!("(authenticate_google_user) No session key found.");
    } else {
      session_keys
        .lock()
        .unwrap()
        .insert(self.session_key.clone().unwrap(), Some(email.clone()));
    }

    event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Success");
    self.email = Some(email.clone());
    Ok(email)
  }

  async fn register_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>> {
    self
      .register(code, session_keys)
      .await
      .map_err(|e| Box::new(e) as Box<dyn Error>)
  }

  fn directory_snapshot(&self) -> Option<Arc<UserDirectory>> {
    Some(self.directory())
  }

  /**
   * Check if a user is logged in on this session.
   *
   * # Returns
   * `true` if the user is logged in, `false` otherwise.
   */
  fn validated_user(&self) -> bool {
    self.email.is_some()
  }
}

/// Mock auth code → email map. `test_code` keeps the legacy single-user mapping.
/// Add more codes here to give tests distinct identities without touching the
/// existing `MockAuthenticator::mock_valid_code()` API.
static MOCK_CODES: Lazy<HashMap<&'static str, &'static str>> = Lazy::new(|| {
  let mut m = HashMap::new();
  m.insert("test_code", "test@example.com");
  m.insert("test_code_alice", "alice@example.com");
  m.insert("test_code_bob", "bob@example.com");
  m.insert("test_code_eve", "eve@example.com");
  m.insert("test_code_new", "newuser@example.com");
  m
});

// Mock authenticator for testing
#[derive(Debug, Clone)]
pub struct MockAuthenticator {
  email: Option<String>,
  session_key: Option<String>,
  web_server: String,
  /// Optional shared directory cell — mirrors `GoogleAuthenticator` so tests
  /// can pre-seed blacklist / active state and exercise the same gates.
  directory: Option<Arc<RwLock<Arc<UserDirectory>>>>,
}

impl MockAuthenticator {
  #[must_use]
  pub fn new(web_server: &str) -> Self {
    MockAuthenticator {
      email: None,
      session_key: None,
      web_server: web_server.to_string(),
      directory: None,
    }
  }

  /// Inject a shared user directory for blacklist / active enforcement.
  #[must_use]
  pub fn with_directory(mut self, directory: Arc<RwLock<Arc<UserDirectory>>>) -> Self {
    self.directory = Some(directory);
    self
  }

  #[must_use]
  pub fn mock_valid_code() -> String {
    "test_code".to_string()
  }

  fn directory_snapshot_inner(&self) -> Option<Arc<UserDirectory>> {
    self
      .directory
      .as_ref()
      .map(|cell| cell.read().expect("(MockAuthenticator.directory) lock poisoned").clone())
  }

  /// Resolve a mock code to an email. Returns `None` for unknown codes.
  fn resolve_code(code: &str) -> Option<&'static str> {
    MOCK_CODES.get(code).copied()
  }
}

#[async_trait]
impl Authenticator for MockAuthenticator {
  fn get_web_server(&self) -> String {
    self.web_server.clone()
  }

  fn get_email(&self) -> Option<String> {
    self.email.clone()
  }

  fn get_session_key(&self) -> Option<String> {
    self.session_key.clone()
  }

  fn set_email(&mut self, email: Option<&String>) {
    self.email = email.cloned();
  }

  fn set_session_key(&mut self, session_key: &str) {
    self.session_key = Some(session_key.to_string());
  }

  async fn authenticate_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>> {
    let Some(email) = Self::resolve_code(code).map(ToString::to_string) else {
      return Err(Box::new(InvalidKeyError {}));
    };
    let email = email.to_lowercase();

    // Blacklist always wins, even for the well-known mock codes — that's the
    // whole point of the blacklist test. Active-list enforcement only kicks
    // in when the test explicitly seeded the directory with the email under
    // test (e.g. seeded `alice@example.com` to exercise an already-registered
    // path); other mock codes (`test@example.com`) skip the active gate so
    // existing tests that don't touch the users file keep working.
    if let Some(dir) = self.directory_snapshot_inner() {
      if dir.is_blacklisted(&email) {
        event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Blacklisted");
        return Err(Box::new(BlacklistedUserError {}));
      }
    }

    self.email = Some(email.clone());

    if self.session_key.is_none() {
      warn!("(MockAuthenticator.authenticate_user) Mock authenticator authenticated user but no session key found.");
    } else {
      session_keys
        .lock()
        .unwrap()
        .insert(self.session_key.clone().unwrap(), self.email.clone());
    }

    debug!(
      "(MockAuthenticator.authenticate_user) Mock authenticator authenticated user {}.",
      email
    );
    Ok(email)
  }

  async fn register_user(
    &mut self, code: &str, session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
  ) -> Result<GoogleProfile, Box<dyn Error>> {
    let Some(email) = Self::resolve_code(code).map(ToString::to_string) else {
      return Err(Box::new(RegisterError::AuthFailed));
    };
    let email = email.to_lowercase();

    // No directory wired in → mock acts like an open-registration sandbox:
    // a successful "login" suffices and there's nothing to persist.
    let Some(directory) = self.directory.clone() else {
      self.email = Some(email.clone());
      if let Some(key) = self.session_key.clone() {
        session_keys.lock().unwrap().insert(key, Some(email.clone()));
      }
      return Ok(email);
    };

    let snapshot = {
      let guard = directory.read().expect("(MockAuthenticator.register) lock poisoned");
      guard.clone()
    };
    if snapshot.is_blacklisted(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Blacklisted");
      return Err(Box::new(RegisterError::NotAuthorized));
    }
    if snapshot.is_active(&email) {
      event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "AlreadyRegistered");
      return Err(Box::new(RegisterError::AlreadyRegistered));
    }

    let now = SystemTime::now()
      .duration_since(UNIX_EPOCH)
      .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));

    let mut new_records = snapshot.raw.clone();
    new_records.push(UserRecord {
      email: email.clone(),
      status: UserStatus::Active,
      registered_at: now,
      blacklisted_at: None,
    });
    let new_dir = build_directory_from_records(new_records, snapshot.generation, snapshot.last_modified);
    *directory.write().expect("(MockAuthenticator.register) lock poisoned for write") = Arc::new(new_dir);

    if let Some(key) = self.session_key.clone() {
      session_keys.lock().unwrap().insert(key, Some(email.clone()));
    }

    event!(target: LOG_AUTH_RESULT, Level::INFO, email = email.as_str(), result = "Registered");
    self.email = Some(email.clone());
    Ok(email)
  }

  fn directory_snapshot(&self) -> Option<Arc<UserDirectory>> {
    self.directory_snapshot_inner()
  }

  fn validated_user(&self) -> bool {
    self.email.is_some()
  }
}

fn generate_session_key() -> String {
  let mut session_key: String = "Bearer ".to_string();
  session_key.push_str(&general_purpose::URL_SAFE_NO_PAD.encode(rand::thread_rng().gen::<[u8; 32]>()));

  session_key
}

// Error types.
#[derive(Debug)]
pub struct TokenTimeoutError {}

impl Error for TokenTimeoutError {}

impl std::fmt::Display for TokenTimeoutError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Token has expired")
  }
}

#[derive(Debug)]
pub struct InvalidKeyError {}

impl std::fmt::Display for InvalidKeyError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Invalid session key")
  }
}

impl Error for InvalidKeyError {}

#[derive(Debug)]
pub struct UnauthorizedUserError {}

impl std::fmt::Display for UnauthorizedUserError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Unauthorized user")
  }
}

impl Error for UnauthorizedUserError {}

/// Returned by `authenticate_user` when the supplied email is blacklisted.
/// The processor maps `Display` of this error to the wire-pinned
/// `"NOT_AUTHORIZED"` string.
#[derive(Debug)]
pub struct BlacklistedUserError {}

impl std::fmt::Display for BlacklistedUserError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "NOT_AUTHORIZED")
  }
}

impl Error for BlacklistedUserError {}

// These structs are all used as message structures to/from Google
#[derive(Debug, Serialize, Deserialize)]
struct GoogleClaims {
  iss: String,
  sub: String,
  azp: String,
  aud: String,
  iat: u64,
  exp: u64,
  email: String,
  email_verified: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize)]
struct GoogleTokenResponse {
  access_token: String,
  expires_in: u32,
  refresh_token: String,
  scope: String,
  token_type: String,
  id_token: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct GooglePublicKeys {
  keys: Vec<GooglePublicKey>,
}

#[derive(Deserialize, Debug, Clone)]
struct GooglePublicKey {
  kid: String,
  n: String,
  e: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug, Clone)]
pub struct GoogleCredentials {
  client_id: String,
  project_id: String,
  auth_uri: String,
  token_uri: String,
  auth_provider_x509_cert_url: String,
  client_secret: String,
  redirect_uris: Vec<String>,
}

#[derive(Deserialize)]
struct GoogleCredsJson {
  web: GoogleCredentials,
}

#[cfg(test)]
pub(crate) mod tests {
  use super::*;

  const GOOGLE_CREDENTIALS_FILE: &str = "google_credentials.json";

  #[tokio::test]
  async fn test_mock_authenticator() {
    let mut mock_auth = MockAuthenticator::new("http://web.test.com");
    mock_auth.set_session_key("test_session_key");

    let session_keys = Arc::new(Mutex::new(HashMap::new()));
    // Test authentication flow
    let email = mock_auth
      .authenticate_user("test_code", &session_keys)
      .await
      .expect("Authentication should succeed");

    assert_eq!(email, "test@example.com");
    assert_eq!(session_keys.lock().unwrap().len(), 1, "Session keys should have one entry");

    // Test if user is now validated.
    assert!(mock_auth.validated_user(), "User should be validated");
  }

  const LOCAL_SECRETS_DIR: &str = "./secrets";
  const LOCAL_TEST_FILE: &str = "./config/authorized_users.json";
  const GCS_TEST_FILE: &str = "gs://callisto-be-user-profiles/authorized_users.json";

  #[test_log::test(tokio::test)]
  #[cfg_attr(feature = "ci", ignore = "Not testable in CI environment.")]
  #[should_panic = "No such file or directory"]
  async fn test_bad_credentials_file() {
    const BAD_FILE: &str = "./not_there_file.json";
    let authorized_users = load_authorized_users(BAD_FILE).await;
    assert!(
      authorized_users.is_empty(),
      "Should not get any contents on a bad authorized user file."
    );
  }

  // This test cannot work in the GitHub Actions CI environment, so skip in that case.
  #[test_log::test(tokio::test)]
  #[cfg_attr(feature = "ci", ignore = "Not testable in CI environment.")]
  async fn test_load_authorized_users_from_gcs() {
    let directory = load_user_directory(GCS_TEST_FILE).await.unwrap();
    assert!(!directory.active.is_empty(), "Authorized users file is empty");
  }

  // This test cannot work in the GitHub Actions CI environment, so skip in that case.
  #[test_log::test(tokio::test)]
  #[cfg_attr(feature = "ci", ignore = "Not testable in CI environment.")]
  async fn test_load_authorized_users_from_file() {
    let directory = load_user_directory(LOCAL_TEST_FILE).await.unwrap();
    assert!(!directory.active.is_empty(), "Authorized users file is empty");
  }

  #[test_log::test(tokio::test)]
  #[cfg_attr(feature = "ci", ignore = "Not testable in CI environment.")]
  async fn test_load_google_credentials_from_file() {
    let credentials =
      GoogleAuthenticator::load_google_credentials(format!("{LOCAL_SECRETS_DIR}/{GOOGLE_CREDENTIALS_FILE}").as_str());
    assert!(!credentials.client_id.is_empty());
    assert!(!credentials.client_secret.is_empty());
  }

  #[test_log::test(tokio::test)]
  #[cfg_attr(feature = "ci", ignore = "Not testable in CI environment.")]
  async fn test_fetch_google_public_keys() {
    let _keys = GoogleAuthenticator::fetch_google_public_keys().await;
  }

  #[test]
  fn test_user_directory_parse_v1() {
    let body = br#"{
      "version": 1,
      "users": [
        {"email":"Alice@Example.com","status":"active","registered_at":1000},
        {"email":"troll@example.com","status":"blacklisted","registered_at":1000,"blacklisted_at":2000}
      ]
    }"#;
    let records = parse_user_directory_body(body).unwrap();
    assert_eq!(records.len(), 2);
    let dir = build_directory_from_records(records, Some(7), 12345);
    assert!(dir.is_active("alice@example.com"));
    assert!(!dir.is_active("troll@example.com"));
    assert!(dir.is_blacklisted("troll@example.com"));
    assert_eq!(dir.generation, Some(7));
    assert_eq!(dir.last_modified, 12345);
  }

  #[test]
  fn test_user_directory_parse_legacy_vec_string_rejected() {
    // Legacy `Vec<String>` shape is no longer auto-migrated. The parser must
    // reject it with an error that tells the operator how to migrate.
    let body = br#"["Alice@Example.com","bob@example.com"]"#;
    let err = parse_user_directory_body(body).unwrap_err();
    let msg = err.to_string();
    assert!(
      msg.contains("Legacy users file format") && msg.contains("migrate"),
      "Expected migration-pointing error, got: {msg}"
    );
  }

  #[tokio::test]
  async fn test_register_lock_serializes_concurrent_registrations() {
    // Two MockAuthenticator clones sharing one directory cell. Fire a pair of
    // concurrent register_user calls; both should succeed (different emails),
    // and the final directory should hold both. This exercises the
    // "register-write swaps the Arc; clones see it" property without GCS.
    let directory: Arc<RwLock<Arc<UserDirectory>>> = Arc::new(RwLock::new(Arc::new(UserDirectory::default())));
    let session_keys: Arc<Mutex<HashMap<String, Option<String>>>> = Arc::new(Mutex::new(HashMap::new()));

    let mut a = MockAuthenticator::new("http://test").with_directory(directory.clone());
    a.set_session_key("session_a");
    let mut b = MockAuthenticator::new("http://test").with_directory(directory.clone());
    b.set_session_key("session_b");

    let (ra, rb) = tokio::join!(
      a.register_user("test_code_alice", &session_keys),
      b.register_user("test_code_bob", &session_keys)
    );
    ra.expect("alice register failed");
    rb.expect("bob register failed");

    let snapshot = directory.read().unwrap().clone();
    assert!(snapshot.is_active("alice@example.com"), "alice should be active");
    assert!(snapshot.is_active("bob@example.com"), "bob should be active");
  }
}

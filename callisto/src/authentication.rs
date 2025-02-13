use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::fmt::Debug;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use dyn_clone::DynClone;

use tokio_tungstenite::tungstenite::handshake::server::{
    Callback, ErrorResponse, Request, Response,
};

use crate::read_local_or_cloud_file;

#[allow(unused_imports)]
use crate::{debug, error, info, warn};

type GoogleProfile = String;

const GOOGLE_X509_CERT_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";
const SESSION_COOKIE_NAME: &str = "callisto-session-key";
const COOKIE_ID: &str = "cookie";

/// Trait defining the authentication behavior for the application
#[async_trait]
pub trait Authenticator: Send + Sync + DynClone + Debug {
    /// Returns the web server URL
    fn get_web_server(&self) -> String;

    /// Authenticates a Google user with the provided code
    /// Returns a tuple of (session_key, user_profile) on success
    async fn authenticate_user(
        &mut self,
        code: &str,
        session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    ) -> Result<GoogleProfile, Box<dyn Error>>;

    /// Checks if this socket has been validated, i.e. has successfully logged in.
    /// # Returns
    /// `true` if the user is validated, `false` otherwise.
    fn validated_user(&self) -> bool;

    fn set_email(&mut self, email: Option<&String>);
    fn set_session_key(&mut self, session_key: &str);
    fn get_email(&self) -> Option<String>;
    fn get_session_key(&self) -> Option<String>;
}

/// Load the list of authorized users from a file.  The file is a JSON array of strings.
///
/// # Arguments
/// * `users_file` - The name of the file to load.
///
/// # Returns
/// A list of all the authorized users.  If the file is invalid, then note the warning and return the empty list.
///
/// # Panics
/// If the file cannot be read.
pub async fn load_authorized_users(users_file: &str) -> Vec<String> {
    load_authorized_users_from_file(users_file)
        .await
        .unwrap_or_else(|_| {
            panic!("(load_authorized_users) Unable to load authorized users file. No such file or directory.");
        })
}

pub struct HeaderCallback {
    pub session_keys: Arc<Mutex<HashMap<String, Option<String>>>>,
    // First is the session key, second is the email
    pub auth_info: Arc<Mutex<(String, Option<String>)>>,
}

impl Callback for HeaderCallback {
    fn on_request(self, request: &Request, response: Response) -> Result<Response, ErrorResponse> {
        let (new_response, new_session_key, new_email) =
            generic_on_request(&self.session_keys, request, response);
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
     * The list of authorized users.
     */
    authorized_users: Arc<Vec<String>>,
}

impl GoogleAuthenticator {
    /// Creates a new `GoogleAuthenticator` instance
    ///
    /// # Arguments
    /// * `url` - The URL of the node server (this server).
    /// * `web_server` - The URL of the web server (front end).
    /// * `credentials` - The Google credentials for this server's domain (for oauth2).
    /// * `google_keys` - A copy of the previously fetched Google public keys.
    /// * `authorized_users` - A pointer to the (possibly long) list of authorized users.
    ///
    /// # Panics
    /// If the `users_file` or `secret_file` cannot be read.
    #[must_use]
    pub fn new(
        web_server: &str,
        credentials: Arc<GoogleCredentials>,
        google_keys: Arc<GooglePublicKeys>,
        authorized_users: Arc<Vec<String>>,
    ) -> Self {
        GoogleAuthenticator {
            credentials,
            email: None,
            session_key: None,
            authorized_users,
            google_keys,
            web_server: web_server.to_string(),
        }
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
            serde_json::from_str::<GooglePublicKeys>(&text).unwrap_or_else(|e| {
                panic!("(validate_google_token) Error: Unable to parse Google public keys: {e:?}")
            }),
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
    session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    request: &Request,
    response: Response,
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
            .and_then(|cookie| {
                cookie
                    .strip_prefix(&format!("{SESSION_COOKIE_NAME}="))
                    .map(std::string::ToString::to_string)
            })
        })
        .collect::<Vec<String>>();

    // If we found a cookie, find if there's a valid email address to go with it.
    // This happens in the case where we get disconnected and the client reconnects.
    let valid_pair = {
        let unlocked_session_keys = session_keys
            .lock()
            .expect("Unable to get lock session keys.");

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
        debug!(
            "(on_request) Found valid email {} for session key: {}",
            email, key
        );
        (response, key.clone(), Some(email.clone()))
    } else if cookies.is_empty() {
        // The case where we need to create a new session key.
        debug!("(on_request) No valid email found for session key.");
        let mut response = response.clone();

        let session_key = generate_session_key();
        session_keys
            .lock()
            .expect("Unable to get lock session keys.")
            .insert(session_key.clone(), None);

        response.headers_mut().insert(
            "Set-Cookie",
            format!("{SESSION_COOKIE_NAME}={session_key}; SameSite=None; Secure")
                .parse()
                .unwrap(),
        );

        (response, session_key, None)
    } else {
        // We aren't logged in, but there is already a set cookie. So just use that vs
        // creating another session key.
        (response, cookies.first().unwrap().clone(), None)
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
        &mut self,
        code: &str,
        session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    ) -> Result<GoogleProfile, Box<dyn Error>> {
        // Call the Google Auth provider with the code.  Decode it and validate it.  Create a session key.
        // Look up the profile.  Then return the session key and profile.
        const GRANT_TYPE: &str = "authorization_code";
        let redirect_uri = &self.get_web_server();

        let token_request = [
            ("code", &code.to_string()),
            ("client_id", &self.credentials.client_id.clone()),
            ("client_secret", &self.credentials.client_secret.clone()),
            ("redirect_uri", &redirect_uri.to_string()),
            ("access_type", &"offline".to_string()),
            ("grant_type", &GRANT_TYPE.to_string()),
        ];

        debug!(
            "(authenticate_google_user) Make request of Google with client_id {:?}, redirect_uri {:?}, access_type {:?}, grant_type {:?}.",
            self.credentials.client_id, redirect_uri, "offline", GRANT_TYPE
        );

        let client = reqwest::Client::new();

        let token_response = client
            .post(&self.credentials.token_uri)
            .form(&token_request)
            .send()
            .await
            .expect("(authenticate_google_user) Unable to fetch Google token");

        debug!("(authenticate_google_user) Fetched token response.");
        let body = token_response.text().await.unwrap_or_else(|e| {
            panic!("(authenticate_google_user) Unable to get text from token response: {e:?}")
        });

        let token_response_json: GoogleTokenResponse =
            serde_json::from_str(&body).unwrap_or_else(|e| {
                panic!("(authenticate_google_user) Unable to parse token response: {e:?} {body}")
            });

        let token = token_response_json.id_token;

        // Get the key ID from the token header
        let header = decode_header(&token).unwrap_or_else(|e| {
            panic!("(authenticate_google_user) Unable to decode token header: {e:?}")
        });
        let kid = header.kid.unwrap_or_else(|| {
            panic!("(authenticate_google_user) Unable to get key ID from token header")
        });

        // Find the matching public key
        let public_key = self
            .google_keys
            .keys
            .iter()
            .find(|k| k.kid == kid)
            .ok_or("No matching public key found")?;

        debug!(
            "(authenticate_google_user) Found matching public key {:?}.",
            public_key
        );

        // Create the decoding key
        let decoding_key = DecodingKey::from_rsa_components(&public_key.n, &public_key.e).unwrap();

        debug!("(authenticate_google_user) Created decoding key and now validating.");

        // Set up validation
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_audience(&[self.credentials.client_id.clone()]);
        validation.set_issuer(&["https://accounts.google.com"]);

        // Decode and validate the token
        let token_data = decode::<GoogleClaims>(&token, &decoding_key, &validation)?;
        // Check if the token is expired
        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        if token_data.claims.exp < now {
            debug!(
                "(authenticate_google_user) Token expired at time {} vs now {}",
                token_data.claims.exp, now
            );
            return Err(Box::new(TokenTimeoutError {}));
        }
        debug!("(authenticate_google_user) Token validated.");

        let email = token_data.claims.email.clone();

        // Check if email is an authorized user
        if !self.authorized_users.contains(&email) {
            return Err(Box::new(UnauthorizedUserError {}));
        }

        // We now have a valid email address.  Associate it with our session key.
        if self.session_key.is_none() {
            warn!("(authenticate_google_user) No session key found.");
        } else {
            session_keys
                .lock()
                .unwrap()
                .insert(self.session_key.clone().unwrap(), Some(email.clone()));
        }

        info!(
            "(Authenticator.authenticate_google_user) Validated login for user {}",
            email
        );
        self.email = Some(email.clone());
        Ok(email)
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

/// Load the list of authorized users from a file.  The file is a JSON array of strings.
///
/// # Arguments
/// * `file_name` - The name of the file to load.
///
/// # Returns
/// A list of all the authorized users.
///
/// # Errors
/// Returns `Err` if the file cannot be read or the file cannot be parsed (e.g. bad JSON)
async fn load_authorized_users_from_file(
    file_name: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let data = read_local_or_cloud_file(file_name).await?;
    serde_json::from_slice::<Vec<String>>(&data)
        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
}

// Mock authenticator for testing
#[derive(Debug, Clone)]
pub struct MockAuthenticator {
    email: Option<String>,
    session_key: Option<String>,
    web_server: String,
}

impl MockAuthenticator {
    #[must_use]
    pub fn new(web_server: &str) -> Self {
        MockAuthenticator {
            email: None,
            session_key: None,
            web_server: web_server.to_string(),
        }
    }

    #[must_use]
    pub fn mock_valid_code() -> String {
        "test_code".to_string()
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
        &mut self,
        code: &str,
        session_keys: &Arc<Mutex<HashMap<String, Option<String>>>>,
    ) -> Result<GoogleProfile, Box<dyn Error>> {
        if code != Self::mock_valid_code() {
            return Err(Box::new(InvalidKeyError {}));
        }

        self.email = Some("test@example.com".to_string());

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
            self.email.as_ref().unwrap()
        );
        self.email.clone().ok_or_else(|| "No email".into())
    }

    fn validated_user(&self) -> bool {
        self.email.is_some()
    }
}

fn generate_session_key() -> String {
    let mut session_key: String = "Bearer ".to_string();
    session_key
        .push_str(&general_purpose::URL_SAFE_NO_PAD.encode(rand::thread_rng().gen::<[u8; 32]>()));

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
struct GoogleTokenRequest {
    code: String,
    client_id: String,
    client_secret: String,
    redirect_uri: String,
    grant_type: String,
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
        assert_eq!(
            session_keys.lock().unwrap().len(),
            1,
            "Session keys should have one entry"
        );

        // Test if user is now validated.
        assert!(mock_auth.validated_user(), "User should be validated");
    }

    const LOCAL_SECRETS_DIR: &str = "./secrets";
    const LOCAL_TEST_FILE: &str = "./config/authorized_users.json";
    const GCS_TEST_FILE: &str = "gs://callisto-be-user-profiles/authorized_users.json";

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
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
    #[cfg_attr(feature = "ci", ignore)]
    async fn test_load_authorized_users_from_gcs() {
        let authorized_users = load_authorized_users_from_file(GCS_TEST_FILE)
            .await
            .unwrap();
        assert!(
            !authorized_users.is_empty(),
            "Authorized users file is empty"
        );
    }

    // This test cannot work in the GitHub Actions CI environment, so skip in that case.
    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
    async fn test_load_authorized_users_from_file() {
        let authorized_users = load_authorized_users_from_file(LOCAL_TEST_FILE)
            .await
            .unwrap();
        assert!(
            !authorized_users.is_empty(),
            "Authorized users file is empty"
        );
    }

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
    async fn test_load_google_credentials_from_file() {
        let credentials = GoogleAuthenticator::load_google_credentials(
            format!("{LOCAL_SECRETS_DIR}/{GOOGLE_CREDENTIALS_FILE}").as_str(),
        );
        assert!(!credentials.client_id.is_empty());
        assert!(!credentials.client_secret.is_empty());
    }

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
    async fn test_fetch_google_public_keys() {
        let _keys = GoogleAuthenticator::fetch_google_public_keys().await;
    }
}

use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use headers::{Cookie, HeaderMapExt};
use hyper::body::Incoming;
use hyper::{Request, StatusCode};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::read_local_or_cloud_file;
use crate::{debug, error, info, warn};

type GoogleProfile = String;

const GOOGLE_CREDENTIALS_FILE: &str = "google_credentials.json";
const GOOGLE_X509_CERT_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

/// Trait defining the authentication behavior for the application
#[async_trait]
pub trait Authenticator: Send + Sync {
    /// Returns the web server URL
    fn get_web_server(&self) -> String;

    /// Authenticates a Google user with the provided code
    /// Returns a tuple of (session_key, user_profile) on success
    async fn authenticate_user(
        &self,
        code: &str,
    ) -> Result<(String, GoogleProfile), Box<dyn Error>>;

    /// Validates a session key and returns the associated email if valid, InvalidKeyError otherwise
    fn validate_session_key(&self, session_key: &str) -> Result<String, InvalidKeyError>;

    /// Checks authorization for an incoming request
    /// Returns the authorized email on success, or an error tuple with status code and message
    async fn check_authorization(
        &self,
        req: &Request<Incoming>,
    ) -> Result<String, (StatusCode, String)>;
}

pub struct GoogleAuthenticator {
    credentials: GoogleCredentials,
    session_keys: RwLock<HashMap<String, String>>,
    authorized_users: Vec<String>,
    node_server_url: String,
    public_keys: Option<GooglePublicKeys>,
    web_server: String,
}

impl GoogleAuthenticator {
    pub async fn new(url: &str, secret: String, users_file: &str, web_server: String) -> Self {
        let credentials = load_google_credentials_from_file(&secret).unwrap_or_else(|e| {
            panic!(
                "Error {:?} loading Google credentials file {}",
                e, GOOGLE_CREDENTIALS_FILE
            )
        });
        let authorized_users = load_authorized_users_from_file(users_file)
            .await
            .expect("Unable to load authorized users file.");
        let mut authenticator = GoogleAuthenticator {
            credentials,
            session_keys: RwLock::new(HashMap::new()),
            authorized_users,
            node_server_url: url.to_string(),
            public_keys: None,
            web_server,
        };

        authenticator.fetch_google_public_keys().await;

        authenticator
    }

    async fn fetch_google_public_keys(&mut self) {
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

        let public_keys = serde_json::from_str::<GooglePublicKeys>(&text).unwrap_or_else(|e| {
            panic!(
                "(validate_google_token) Error: Unable to parse Google public keys: {:?}",
                e
            )
        });

        self.public_keys = Some(public_keys);
    }
}

#[async_trait]
impl Authenticator for GoogleAuthenticator {
    fn get_web_server(&self) -> String {
        self.web_server.clone()
    }

    async fn authenticate_user(
        &self,
        code: &str,
    ) -> Result<(String, GoogleProfile), Box<dyn Error>> {
        // Call the Google Auth provider with the code.  Decode it and validate it.  Create a session key.
        // Look up the profile.  Then return the session key and profile.
        const GRANT_TYPE: &str = "authorization_code";
        let redirect_uri = &self.node_server_url;

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
            panic!(
                "(authenticate_google_user) Unable to get text from token response: {:?}",
                e
            )
        });

        let token_response_json: GoogleTokenResponse =
            serde_json::from_str(&body).unwrap_or_else(|e| {
                panic!(
                    "(authenticate_google_user) Unable to parse token response: {:?}",
                    e
                )
            });

        let token = token_response_json.id_token;

        // Get the key ID from the token header
        let header = decode_header(&token).unwrap_or_else(|e| {
            panic!(
                "(authenticate_google_user) Unable to decode token header: {:?}",
                e
            )
        });
        let kid = header.kid.unwrap_or_else(|| {
            panic!("(authenticate_google_user) Unable to get key ID from token header")
        });

        // Find the matching public key
        let public_key = self
            .public_keys
            .as_ref()
            .unwrap()
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

        // Generate a cryptographically secure session key.
        // Record it with the email from token_data.email.
        // Then return the session key and the email.

        // Generate a cryptographically secure session key.
        //let mut session_key: String = "Bearer ".to_string();
        let mut session_key: String = "".to_string();
        session_key.push_str(
            &general_purpose::URL_SAFE_NO_PAD.encode(rand::thread_rng().gen::<[u8; 32]>()),
        );

        // Record it with the email from token_data.email.
        let email = token_data.claims.email.clone();

        // Check if email is an authorized user
        if !self.authorized_users.contains(&email) {
            return Err(Box::new(UnauthorizedUserError {}));
        }

        self.session_keys
            .write()
            .unwrap()
            .insert(session_key.clone(), email.clone());

        info!("Created session key for user: {}", email);

        // Then return the session key and the email.
        Ok((session_key, email))
    }

    // Given a session key, make sure we have it in our table and thus
    // there is a corresponding email address.  Lack of an entry
    // means this cookie is old or made up.
    // Return Ok(the email address) or an InvalidKeyError.
    fn validate_session_key(&self, session_key: &str) -> Result<String, InvalidKeyError> {
        if let Some(email) = self.session_keys.read().unwrap().get(session_key) {
            Ok(email.to_string())
        } else {
            Err(InvalidKeyError {})
        }
    }

    async fn check_authorization(
        &self,
        req: &Request<Incoming>,
    ) -> Result<String, (StatusCode, String)> {
        if let Some(cookies) = req.headers().typed_get::<Cookie>() {
            match cookies.get("callisto-session-key") {
                Some(cookie) => {
                    debug!(
                        "(Authenticator.check_authorization) Found session key cookie: {:?}",
                        cookie
                    );

                    self.validate_session_key(cookie).map_err(|e| {
                        error!(
                            "(Authenticator.check_authorization) Invalid session key: {:?}",
                            e
                        );
                        (StatusCode::UNAUTHORIZED, "Invalid session key".to_string())
                    })
                }
                None => Err((
                    StatusCode::UNAUTHORIZED,
                    "No session key cookie".to_string(),
                )),
            }
        } else {
            Err((
                StatusCode::UNAUTHORIZED,
                "No session key cookie".to_string(),
            ))
        }
    }
}

fn load_google_credentials_from_file(file_name: &str) -> Result<GoogleCredentials, Box<dyn Error>> {
    let file = std::fs::File::open(file_name).unwrap_or_else(|e| {
        panic!(
            "Error {:?} opening Google credentials file {}",
            e, file_name
        )
    });
    let reader = std::io::BufReader::new(file);
    let credentials: GoogleCredsJson = serde_json::from_reader(reader).unwrap_or_else(|e| {
        panic!(
            "Error {:?} parsing Google credentials file {}",
            e, file_name
        )
    });
    debug!("Load Google credentials file \"{}\".", file_name);
    Ok(credentials.web)
}

pub async fn load_authorized_users_from_file(
    file_name: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let data = read_local_or_cloud_file(file_name).await?;
    serde_json::from_slice::<Vec<String>>(&data)
        .map_err(|e| panic!("Error {:?} parsing authorized users file {}", e, file_name))
}

// Mock authenticator for testing
pub struct MockAuthenticator {
    session_keys: RwLock<HashMap<String, String>>,
    web_server: String,
}

impl MockAuthenticator {
    pub async fn new(_url: &str, _secret: String, _users_file: &str, web_server: String) -> Self {
        MockAuthenticator {
            session_keys: RwLock::new(HashMap::new()),
            web_server,
        }
    }
}

#[async_trait]
impl Authenticator for MockAuthenticator {
    fn get_web_server(&self) -> String {
        self.web_server.clone()
    }

    async fn authenticate_user(
        &self,
        _code: &str,
    ) -> Result<(String, GoogleProfile), Box<dyn Error>> {
        let session_key = "TeSt_KeY".to_string();
        let email = "test@example.com".to_string();
        self.session_keys
            .write()
            .unwrap()
            .insert(session_key.clone(), email.clone());
        Ok((session_key, email))
    }

    fn validate_session_key(&self, session_key: &str) -> Result<String, InvalidKeyError> {
        if let Some(email) = self.session_keys.read().unwrap().get(session_key) {
            debug!(
                "(MockAuthenticator.validate_session_key) Session key validated: {}",
                session_key
            );
            Ok(email.to_string())
        } else {
            warn!("(MockAuthenticator.validate_session_key) Unexpected: MockAuthenticator failing to authenticate key {}",
                session_key);
            Err(InvalidKeyError {})
        }
    }

    async fn check_authorization(
        &self,
        req: &Request<Incoming>,
    ) -> Result<String, (StatusCode, String)> {
        if let Some(cookies) = req.headers().typed_get::<Cookie>() {
            if let Some(session_key) = cookies.get("callisto-session-key") {
                return self
                    .validate_session_key(session_key)
                    .map_err(|_| (StatusCode::UNAUTHORIZED, "Invalid session key".to_string()));
            }
        }
        Err((StatusCode::UNAUTHORIZED, "No session key found".to_string()))
    }
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

#[derive(Deserialize)]
struct GooglePublicKeys {
    keys: Vec<GooglePublicKey>,
}

#[derive(Deserialize, Debug)]
struct GooglePublicKey {
    kid: String,
    n: String,
    e: String,
}

#[allow(dead_code)]
#[derive(Deserialize, Debug)]
struct GoogleCredentials {
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

    #[tokio::test]
    async fn test_mock_authenticator() {
        let mock_auth = MockAuthenticator::new(
            "http://test.com",
            "secret".to_string(),
            "users.txt",
            "http://web.test.com".to_string(),
        )
        .await;

        // Test authentication flow
        let (session_key, email) = mock_auth
            .authenticate_user("test_code")
            .await
            .expect("Authentication should succeed");

        assert_eq!(email, "test@example.com");

        // Test session key validation
        let validated_email = mock_auth
            .validate_session_key(&session_key)
            .expect("Session key should be valid");
        assert_eq!(validated_email, "test@example.com");

        // Test invalid session key
        assert!(mock_auth.validate_session_key("invalid_key").is_err());
    }

    const LOCAL_SECRETS_DIR: &str = "./secrets";
    const LOCAL_TEST_FILE: &str = "./config/authorized_users.json";
    const GCS_TEST_FILE: &str = "gs://callisto-be-user-profiles/authorized_users.json";
    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
    #[should_panic]
    async fn test_bad_credentials_file() {
        const BAD_FILE: &str = "./not_there_file.json";
        let authenticator = GoogleAuthenticator::new(
            "http://localhost:3000",
            BAD_FILE.to_string(),
            "",
            "http://localhost:3000".to_string(),
        )
        .await; // This should fail.
        assert!(authenticator.credentials.client_id.is_empty());
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
        let credentials = load_google_credentials_from_file(
            format!("{}/{}", LOCAL_SECRETS_DIR, GOOGLE_CREDENTIALS_FILE).as_str(),
        )
        .unwrap();
        assert!(!credentials.client_id.is_empty());
        assert!(!credentials.client_secret.is_empty());
    }

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci", ignore)]
    async fn test_fetch_google_public_keys() {
        let mut authenticator = GoogleAuthenticator::new(
            "http://localhost:3000",
            format!("{}/{}", LOCAL_SECRETS_DIR, GOOGLE_CREDENTIALS_FILE),
            LOCAL_TEST_FILE,
            "http://localhost:3000".to_string(),
        )
        .await;
        authenticator.fetch_google_public_keys().await;
        assert!(authenticator.public_keys.is_some());
    }
}

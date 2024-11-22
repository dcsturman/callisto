use base64::{engine::general_purpose, Engine as _};
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use hyper::body::Incoming;
use hyper::Request;
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use rand::Rng;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::error::Error;
use std::sync::RwLock;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::{debug, error, info};

type GoogleProfile = String;

const GOOGLE_CREDENTIALS_FILE: &str = "google_credentials.json";
const DEFAULT_CONFIG_DIRECTORY: &str = "./config";

const DEFAULT_AUTHORIZED_USERS_FILE: &str = "authorized_users.json";
const GOOGLE_X509_CERT_URL: &str = "https://www.googleapis.com/oauth2/v3/certs";

#[allow(dead_code)]
pub struct Authenticator {
    credentials: GoogleCredentials,
    session_keys: RwLock<HashMap<String, String>>,
    authorized_users: Vec<String>,
    node_server_url: String,
    public_keys: Option<GooglePublicKeys>,
}

impl Authenticator {
    pub async fn new(url: &str, secret: String, gcs_bucket: Option<String>) -> Self {
        let credentials = load_google_credentials_from_file(&secret).unwrap_or_else(|e| {
            panic!(
                "Error {:?} loading Google credentials file {}",
                e, GOOGLE_CREDENTIALS_FILE
            )
        });
        let authorized_users =
            load_authorized_users_from_file(DEFAULT_AUTHORIZED_USERS_FILE, gcs_bucket)
                .await
                .expect("Unable to load authorized users file.");
        Authenticator {
            credentials,
            session_keys: RwLock::new(HashMap::new()),
            authorized_users,
            node_server_url: url.to_string(),
            public_keys: None,
        }
    }

    pub async fn authenticate_google_user(
        &self,
        code: &str,
    ) -> Result<(String, GoogleProfile), Box<dyn Error>> {
        info!(
            "(authenticate_google_user) Received and processing login request. {:?}",
            code
        );

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

        debug!(
            "(authenticate_google_user) **** NOT SAFE **** Body of token response is :{:?}",
            body
        );

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
        let mut session_key: String = "Bearer ".to_string();

        session_key.push_str(
            &general_purpose::URL_SAFE_NO_PAD.encode(rand::thread_rng().gen::<[u8; 32]>()),
        );

        // Record it with the email from token_data.email.
        let email = token_data.claims.email.clone();
        self.session_keys
            .write()
            .unwrap()
            .insert(session_key.clone(), email.clone());

        info!("Created session key for user: {}", email);

        // Then return the session key and the email.
        Ok((session_key, email))
    }

    pub fn validate_session_key(&self, session_key: &str) -> Result<String, InvalidKeyError> {
        if let Some(email) = self.session_keys.read().unwrap().get(session_key) {
            Ok(email.to_string())
        } else {
            Err(InvalidKeyError {})
        }
    }

    pub async fn check_authorization(
        &self,
        req: &Request<Incoming>,
    ) -> Result<String, (hyper::StatusCode, String)> {
        let auth_header = req.headers().get("Authorization");
        debug!("(Authenticator.check_authorization) Authorization header found.",);

        // Need to check if email address is valid and if it on our list of accepted users
        match auth_header {
            Some(header) => {
                let token = header.to_str().unwrap();
                let valid = self.validate_session_key(token);

                match valid {
                    Ok(email) => {
                        if self.authorized_users.contains(&email) {
                            Ok(email)
                        } else {
                            Err((
                                hyper::StatusCode::FORBIDDEN,
                                "Unauthorized user".to_string(),
                            ))
                        }
                    }
                    Err(e) => {
                        error!(
                            "(Authenticator.check_authorization) Invalid session key: {:?}",
                            e
                        );
                        Err((
                            hyper::StatusCode::UNAUTHORIZED,
                            "Invalid session key".to_string(),
                        ))
                    }
                }
            }
            None => Err((
                hyper::StatusCode::UNAUTHORIZED,
                "No Authorization header".to_string(),
            )),
        }
    }
    pub async fn fetch_google_public_keys(&mut self) {
        // Fetch Google's public keys
        let client = reqwest::Client::new();
        let public_keys_response = client
            .get(GOOGLE_X509_CERT_URL)
            .send()
            .await
            .expect("(validate_google_token) Unable to fetch Google public keys");

        debug!("(validate_google_token) Fetched Google public keys.",);

        let text = public_keys_response.text().await.unwrap();
        debug!(
            "(validate_google_token) Body of key response is :{:?}",
            text
        );

        let public_keys = serde_json::from_str::<GooglePublicKeys>(&text).unwrap_or_else(|e| {
            panic!(
                "(validate_google_token) Error: Unable to parse Google public keys: {:?}",
                e
            )
        });

        self.public_keys = Some(public_keys);
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
    gcs_bucket: Option<String>,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    if let Some(bucket) = gcs_bucket {
        let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
            panic!(
                "Error {:?} authenticating with Google Cloud Storage. Did you do `gcloud auth application-default login` before running?",
                e
            )
        });
        let client = Client::new(config);

        debug!("(load_authorized_users_from_file) Attempting to load authorized users file \"{}\" from GCS bucket {}.", file_name, bucket);
        let data = client
            .download_object(
                &GetObjectRequest {
                    bucket: bucket.clone(),
                    object: file_name.to_string(),
                    ..Default::default()
                },
                &Range::default(),
            )
            .await
            .unwrap_or_else(|e| {
                panic!(
                    "Error {:?} downloading authorized users file {} from GCS bucket {}",
                    e, file_name, bucket
                )
            });

        debug!("(load_authorized_users_from_file) Loaded authorized users file \"{}\" from GCS bucket {}.", file_name, bucket);
        Ok(
            serde_json::from_slice::<Vec<String>>(&data).unwrap_or_else(|e| {
                panic!(
                    "Error {:?} parsing Google credentials file in GCS {}",
                    e, file_name
                )
            }),
        )
    } else {
        let file = std::fs::File::open(format!("{}/{}", DEFAULT_CONFIG_DIRECTORY, file_name))?;

        let reader = std::io::BufReader::new(file);
        let mut templates: Vec<String> = serde_json::from_reader(reader)?;
        templates.sort();

        Ok(templates)
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
mod tests {
    use super::*;

    const GCS_TEST_BUCKET: &str = "callisto-be-user-profiles";

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci",ignore)]
    async fn test_load_authorized_users_from_gcs() {
        let bucket_name = GCS_TEST_BUCKET;
        let authorized_users = load_authorized_users_from_file(
            DEFAULT_AUTHORIZED_USERS_FILE,
            Some(bucket_name.to_string()),
        )
        .await
        .unwrap();
        assert_eq!(authorized_users.len(), 1);
    }

    #[test_log::test(tokio::test)]
    #[cfg_attr(feature = "ci",ignore)]
    async fn test_load_authorized_users_from_file() {
        let authorized_users = load_authorized_users_from_file(DEFAULT_AUTHORIZED_USERS_FILE, None)
            .await
            .unwrap();
        assert_eq!(authorized_users.len(), 1);
    }
}

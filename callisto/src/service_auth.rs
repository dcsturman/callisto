//! How a service, rather than a person, logs in.
//!
//! A person logs in with a Google OAuth code from a browser redirect, which a
//! service cannot obtain. The AI service instead presents a token, and this
//! module decides whether to believe it. Two kinds are accepted:
//!
//! * **A Google identity token**, on Cloud Run. The service asks the metadata
//!   server for a fresh one for each connection, naming this backend as the
//!   audience. We check Google's signature, the issuer, the audience and the
//!   expiry, and then that the service account's email is an active entry in
//!   the authorized-users file -- the same list people are checked against.
//!   Nothing is stored on either side, so restarts and redeploys change
//!   nothing.
//! * **A shared secret**, for local Docker Compose, where there is no metadata
//!   server. Read from the environment rather than the command line, so it
//!   does not show up in a process listing. Never baked into an image.
//!
//! Either may be configured, both, or neither; with neither, `ServiceLogin` is
//! refused.

use std::sync::Arc;

use jsonwebtoken::{decode, decode_header, Algorithm, Validation};
use serde::Deserialize;

use crate::authentication::{GooglePublicKeys, UserDirectory};

/// The identity a shared-secret login is given. Its local part is what the
/// Users list shows.
pub const SHARED_SECRET_IDENTITY: &str = "ai-ships@callisto.local";

const GOOGLE_ISSUERS: [&str; 2] = ["https://accounts.google.com", "accounts.google.com"];

/// Why a service token was turned away. The wire only ever sees
/// `SERVICE_AUTH_FAILED`; the detail is for the log.
#[derive(Debug, PartialEq, Eq)]
pub enum ServiceAuthError {
  /// Neither kind of service login is configured on this backend.
  NotConfigured,
  /// The token is neither the shared secret nor a valid identity token.
  BadToken(String),
  /// A valid identity token for an account not on the authorized list.
  NotAuthorized(String),
}

impl std::fmt::Display for ServiceAuthError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      ServiceAuthError::NotConfigured => write!(f, "service login is not configured"),
      ServiceAuthError::BadToken(why) => write!(f, "bad service token: {why}"),
      ServiceAuthError::NotAuthorized(email) => write!(f, "{email} is not an authorized service account"),
    }
  }
}

impl std::error::Error for ServiceAuthError {}

#[derive(Debug, Deserialize)]
struct IdentityClaims {
  email: String,
  #[serde(default)]
  email_verified: Option<bool>,
}

/// What this backend will accept from a service.
#[derive(Clone, Default)]
pub struct ServiceAuth {
  shared_secret: Option<String>,
  /// The audience an identity token must name: this backend's URL.
  audience: Option<String>,
  google_keys: Option<Arc<GooglePublicKeys>>,
}

impl std::fmt::Debug for ServiceAuth {
  // Never print the secret.
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("ServiceAuth")
      .field("shared_secret", &self.shared_secret.as_ref().map(|_| "<set>"))
      .field("audience", &self.audience)
      .field("google_keys", &self.google_keys.is_some())
      .finish()
  }
}

impl ServiceAuth {
  /// Configure service login. An empty secret counts as none, so an unset
  /// environment variable passed through Compose as `""` does not let every
  /// empty token in.
  #[must_use]
  pub fn new(
    shared_secret: Option<String>, audience: Option<String>, google_keys: Option<Arc<GooglePublicKeys>>,
  ) -> Self {
    ServiceAuth {
      shared_secret: shared_secret.filter(|secret| !secret.is_empty()),
      audience: audience.filter(|audience| !audience.is_empty()),
      google_keys,
    }
  }

  /// Whether any kind of service login is possible.
  #[must_use]
  pub fn enabled(&self) -> bool {
    self.shared_secret.is_some() || (self.audience.is_some() && self.google_keys.is_some())
  }

  /// Decide whether to believe a service token, and if so who it is.
  ///
  /// # Errors
  /// Returns why the token was refused.
  pub fn verify(&self, token: &str, directory: &UserDirectory) -> Result<String, ServiceAuthError> {
    if !self.enabled() {
      return Err(ServiceAuthError::NotConfigured);
    }
    if let Some(secret) = &self.shared_secret {
      if constant_time_eq(secret.as_bytes(), token.as_bytes()) {
        return Ok(SHARED_SECRET_IDENTITY.to_string());
      }
    }
    let (Some(audience), Some(keys)) = (&self.audience, &self.google_keys) else {
      return Err(ServiceAuthError::BadToken("not the shared secret".to_string()));
    };

    let header = decode_header(token).map_err(|e| ServiceAuthError::BadToken(e.to_string()))?;
    let kid = header.kid.ok_or_else(|| ServiceAuthError::BadToken("no key id".to_string()))?;
    let key = keys
      .decoding_key(&kid)
      .ok_or_else(|| ServiceAuthError::BadToken(format!("unknown key id {kid}")))?;

    // `Validation` checks the expiry itself.
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[audience]);
    validation.set_issuer(&GOOGLE_ISSUERS);
    let claims = decode::<IdentityClaims>(token, &key, &validation)
      .map_err(|e| ServiceAuthError::BadToken(e.to_string()))?
      .claims;

    if claims.email_verified == Some(false) {
      return Err(ServiceAuthError::BadToken("email not verified".to_string()));
    }
    let email = claims.email.to_lowercase();
    if directory.is_blacklisted(&email) || !directory.is_active(&email) {
      return Err(ServiceAuthError::NotAuthorized(email));
    }
    Ok(email)
  }
}

/// Compare without stopping at the first difference, so the time taken says
/// nothing about how much of a guess was right.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
  a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
  use super::*;
  use jsonwebtoken::{encode, EncodingKey, Header};
  use serde::Serialize;
  use std::time::{SystemTime, UNIX_EPOCH};

  const KEY_PEM: &str = include_str!("../tests/fixtures/service_auth_test_key.pem");
  const KEY_N: &str = include_str!("../tests/fixtures/service_auth_test_key.n");
  const KID: &str = "test-key";
  const AUDIENCE: &str = "https://callisto-be.example.run.app";
  const ROBOT: &str = "ai-ships@callisto-test.iam.gserviceaccount.com";

  #[derive(Serialize)]
  struct Claims<'a> {
    iss: &'a str,
    aud: &'a str,
    email: &'a str,
    email_verified: bool,
    exp: u64,
  }

  fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs()
  }

  fn token(audience: &str, email: &str, exp: u64) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(KID.to_string());
    let claims = Claims {
      iss: "https://accounts.google.com",
      aud: audience,
      email,
      email_verified: true,
      exp,
    };
    encode(&header, &claims, &EncodingKey::from_rsa_pem(KEY_PEM.as_bytes()).unwrap()).unwrap()
  }

  fn cloud_run() -> ServiceAuth {
    ServiceAuth::new(
      None,
      Some(AUDIENCE.to_string()),
      Some(Arc::new(GooglePublicKeys::single(KID, KEY_N.trim(), "AQAB"))),
    )
  }

  fn directory_with(active: &[&str], blacklisted: &[&str]) -> UserDirectory {
    UserDirectory {
      active: active.iter().map(ToString::to_string).collect(),
      blacklisted: blacklisted.iter().map(ToString::to_string).collect(),
      ..UserDirectory::default()
    }
  }

  #[test]
  fn nothing_configured_refuses_everything() {
    let auth = ServiceAuth::new(Some(String::new()), None, None);
    assert!(!auth.enabled(), "an empty secret is no secret");
    assert_eq!(auth.verify("", &UserDirectory::default()), Err(ServiceAuthError::NotConfigured));
  }

  #[test]
  fn the_shared_secret_logs_in_as_the_ai_service() {
    let auth = ServiceAuth::new(Some("hunter2".to_string()), None, None);
    let directory = UserDirectory::default();
    assert_eq!(auth.verify("hunter2", &directory), Ok(SHARED_SECRET_IDENTITY.to_string()));
    assert!(matches!(auth.verify("hunter3", &directory), Err(ServiceAuthError::BadToken(_))));
    assert!(!format!("{auth:?}").contains("hunter2"), "the secret never reaches a log");
  }

  #[test]
  fn an_identity_token_for_an_authorized_account_logs_in() {
    let directory = directory_with(&[ROBOT], &[]);
    let fresh = token(AUDIENCE, ROBOT, now() + 600);
    assert_eq!(cloud_run().verify(&fresh, &directory), Ok(ROBOT.to_string()));
  }

  #[test]
  fn an_identity_token_is_refused_for_the_wrong_audience_expiry_or_account() {
    let auth = cloud_run();
    let directory = directory_with(&[ROBOT], &["banned@callisto-test.iam.gserviceaccount.com"]);

    let elsewhere = token("https://someone-else.run.app", ROBOT, now() + 600);
    assert!(matches!(
      auth.verify(&elsewhere, &directory),
      Err(ServiceAuthError::BadToken(_))
    ));

    let stale = token(AUDIENCE, ROBOT, now() - 600);
    assert!(matches!(auth.verify(&stale, &directory), Err(ServiceAuthError::BadToken(_))));

    let stranger = token(AUDIENCE, "stranger@callisto-test.iam.gserviceaccount.com", now() + 600);
    assert!(matches!(
      auth.verify(&stranger, &directory),
      Err(ServiceAuthError::NotAuthorized(_))
    ));

    let banned = token(AUDIENCE, "banned@callisto-test.iam.gserviceaccount.com", now() + 600);
    assert!(matches!(
      auth.verify(
        &banned,
        &directory_with(
          &["banned@callisto-test.iam.gserviceaccount.com"],
          &["banned@callisto-test.iam.gserviceaccount.com"]
        )
      ),
      Err(ServiceAuthError::NotAuthorized(_))
    ));
  }

  #[test]
  fn a_forged_token_is_refused() {
    let genuine = token(AUDIENCE, ROBOT, now() + 600);
    let mut parts: Vec<&str> = genuine.split('.').collect();
    let forged_claims = base64_url(
      &serde_json::to_vec(&Claims {
        iss: "https://accounts.google.com",
        aud: AUDIENCE,
        email: "someone-important@example.com",
        email_verified: true,
        exp: now() + 600,
      })
      .unwrap(),
    );
    parts[1] = &forged_claims;
    let forged = parts.join(".");
    assert!(matches!(
      cloud_run().verify(&forged, &directory_with(&["someone-important@example.com"], &[])),
      Err(ServiceAuthError::BadToken(_))
    ));
  }

  fn base64_url(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
  }
}

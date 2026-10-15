//! The credential the service presents when it logs in to the backend.

use std::time::Duration;

const METADATA_IDENTITY_URL: &str =
  "http://metadata.google.internal/computeMetadata/v1/instance/service-accounts/default/identity";

/// How the service proves to the backend who it is.
#[derive(Clone)]
pub enum Credential {
  /// The shared secret, for local Docker Compose.
  SharedSecret(String),
  /// A Google identity token from the Cloud Run metadata server, naming the
  /// backend as audience. Fetched fresh for every login.
  CloudRun { audience: String },
}

impl std::fmt::Debug for Credential {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Credential::SharedSecret(_) => f.write_str("SharedSecret(<set>)"),
      Credential::CloudRun { audience } => write!(f, "CloudRun {{ audience: {audience} }}"),
    }
  }
}

impl Credential {
  /// The token to send in `ServiceLogin`.
  ///
  /// # Errors
  /// Returns an error if the metadata server cannot be reached.
  pub async fn token(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    match self {
      Credential::SharedSecret(secret) => Ok(secret.clone()),
      Credential::CloudRun { audience } => Ok(
        reqwest::Client::builder()
          .timeout(Duration::from_secs(10))
          .build()?
          .get(METADATA_IDENTITY_URL)
          .query(&[("audience", audience.as_str())])
          .header("Metadata-Flavor", "Google")
          .send()
          .await?
          .error_for_status()?
          .text()
          .await?,
      ),
    }
  }
}

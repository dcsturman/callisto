/// Lib for callisto
///
/// Most of our logic is in `main.rs` or `processor.rs`.  This files allows us to build the crate as a library for use
/// in integration tests. It also holds any general utility functions that don't have a logical home elsewhere.
pub mod action;
pub mod authentication;
pub mod combat;
mod computer;
pub mod crew;
pub mod entity;
pub mod missile;
pub mod payloads;
pub mod planet;
pub mod player;
pub mod processor;
mod rules_tables;
pub mod server;
pub mod ship;

#[macro_use]
mod cov_util;

#[cfg(test)]
pub mod unit_tests;

use entity::MetaData;
use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
use google_cloud_storage::http::Error as GcsHttpError;
use once_cell::sync::OnceCell;
use serde::Deserialize;
use std::fs::File;
use std::io::{BufReader, Read};
use std::sync::{Arc, RwLock};

pub type ScenarioMetadataList = Vec<(String, MetaData)>;
type SharedScenarioMetadataList = Arc<ScenarioMetadataList>;

/// A single scenario file that failed to load. Surfaced server-side to the
/// owner (when known) so they see their broken scenario instead of silently
/// dropping it from the picker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScenarioFailure {
  pub filename: String,
  pub owner: String,
  pub error: String,
}

pub type ScenarioFailureList = Vec<ScenarioFailure>;
type SharedScenarioFailureList = Arc<ScenarioFailureList>;

pub static SCENARIOS: OnceCell<RwLock<SharedScenarioMetadataList>> = OnceCell::new();
pub static SCENARIO_FAILURES: OnceCell<RwLock<SharedScenarioFailureList>> = OnceCell::new();
pub const LOG_FILE_USE: &str = "READ_FILE";
pub const LOG_AUTH_RESULT: &str = "LOGIN_ATTEMPT";
pub const LOGOUT: &str = "LOGOUT";
pub const LOG_SCENARIO_ACTIVITY: &str = "SCENARIO";

/// Maximum number of files read concurrently when loading a whole directory.
///
/// A directory load fans out one read per file. Against GCS each of those is a
/// separate HTTPS request, and issuing all of them simultaneously (79 designs,
/// 79 sockets) makes a handful fail at the transport layer on essentially every
/// attempt. A single read error fails the whole load, so the reload watcher
/// retries until one attempt happens to win: startup takes minutes and users
/// briefly see a spurious "failed to load" banner.
///
/// This is the same unbounded fan-out that made every file fetch its own OAuth
/// token before the clients were shared (see [`gcs_client`]), one layer down.
/// Eight in flight is enough to keep the pipe full while staying well inside
/// what the endpoint will accept. Local directories are fast either way.
pub const MAX_CONCURRENT_DIR_FILE_READS: usize = 8;

/// Replace the current global scenario metadata snapshot.
///
/// # Panics
///
/// Panics if the write lock is poisoned.
pub fn replace_scenarios(scenarios: ScenarioMetadataList) {
  let scenarios = Arc::new(scenarios);
  let scenarios_lock = SCENARIOS.get_or_init(|| RwLock::new(scenarios.clone()));
  *scenarios_lock.write().expect("(replace_scenarios) Unable to update scenarios") = scenarios;
}

/// Return the current global scenario metadata snapshot.
///
/// # Panics
///
/// Panics if scenarios have not been initialized yet or if the read lock is poisoned.
#[must_use]
pub fn get_scenarios_snapshot() -> SharedScenarioMetadataList {
  SCENARIOS
    .get()
    .expect("(get_scenarios_snapshot) Scenarios not loaded")
    .read()
    .expect("(get_scenarios_snapshot) Unable to read scenarios")
    .clone()
}

/// Replace the current global scenario-failure snapshot.
///
/// # Panics
///
/// Panics if the write lock is poisoned.
pub fn replace_scenario_failures(failures: ScenarioFailureList) {
  let failures = Arc::new(failures);
  let failures_lock = SCENARIO_FAILURES.get_or_init(|| RwLock::new(failures.clone()));
  *failures_lock
    .write()
    .expect("(replace_scenario_failures) Unable to update scenario failures") = failures;
}

/// Return the current global scenario-failure snapshot. Returns an empty
/// list if the registry has not been initialized yet (e.g. before the first
/// scenario load completes).
///
/// # Panics
///
/// Panics if the read lock is poisoned.
#[must_use]
pub fn get_scenario_failures_snapshot() -> SharedScenarioFailureList {
  SCENARIO_FAILURES.get().map_or_else(
    || Arc::new(Vec::new()),
    |lock| {
      lock
        .read()
        .expect("(get_scenario_failures_snapshot) Unable to read scenario failures")
        .clone()
    },
  )
}

/// Lenient owner extraction. Used when full scenario parse fails so we can
/// still tell which user owns the broken file. Reads only `metadata.owner`
/// from the JSON; if even that fails (truly malformed JSON), returns an
/// empty string. Never errors — best-effort by design.
#[must_use]
pub fn extract_scenario_owner(scenario_contents: &[u8]) -> String {
  #[derive(Deserialize)]
  struct OwnerOnly {
    #[serde(default)]
    metadata: MetaDataOwnerOnly,
  }
  #[derive(Deserialize, Default)]
  struct MetaDataOwnerOnly {
    #[serde(default)]
    owner: String,
  }
  serde_json::from_slice::<OwnerOnly>(scenario_contents)
    .map(|o| o.metadata.owner)
    .unwrap_or_default()
}

/// Process-wide GCS client, built once and shared by every caller.
///
/// Constructing a client calls `ClientConfig::with_auth()`, which on Cloud Run
/// fetches an OAuth token from the instance metadata server at 169.254.169.254.
/// Building one per operation is what made loading a directory of designs a
/// burst of simultaneous token requests - one per file - which the metadata
/// server refuses under load. At 19 designs that failed intermittently; at 79
/// it failed for roughly half of them on every attempt.
///
/// A `tokio::sync::OnceCell` is used rather than `once_cell::sync::OnceCell`
/// because initialisation is async, and specifically for its failure
/// semantics: see [`gcs_client`].
static GCS_CLIENT: tokio::sync::OnceCell<Client> = tokio::sync::OnceCell::const_new();

/// Force the shared GCS client to be built now, rather than on first use.
///
/// Called at startup so the token fetch happens once, up front, while the
/// instance still has its startup CPU boost - instead of racing a directory
/// load. Safe to call more than once; after the first success it is a no-op.
///
/// Only worth calling when a `gs://` path is actually configured. Local
/// scenario/design directories never touch GCS, and building a client without
/// credentials would fail for no reason.
///
/// # Errors
/// Returns an error if the client cannot be built or authenticated. Nothing is
/// cached on failure, so a later call - or the lazy path - will retry.
pub async fn ensure_gcs_client() -> Result<(), Box<dyn std::error::Error>> {
  gcs_client().await.map(|_| ())
}

/// Return the shared GCS client, building and authenticating it on first use.
///
/// Recovery from a failed build is the point of `get_or_try_init`:
///
/// - On error **nothing is cached**. The cell stays empty and the very next
///   caller retries. A transient metadata-server failure at startup therefore
///   cannot poison the process, which a plain `OnceCell` holding a
///   `Result` would do.
/// - Concurrent callers do not each attempt initialisation. The first one runs
///   it while the rest await the same attempt, so N concurrent file reads
///   produce **one** token request rather than N. That is what removes the
///   thundering herd, independent of any caching benefit.
/// - On success the client is reused for the life of the process. The client
///   owns a token source that refreshes expiring tokens internally, so a
///   long-lived client does not go stale.
async fn gcs_client() -> Result<&'static Client, Box<dyn std::error::Error>> {
  GCS_CLIENT
    .get_or_try_init(|| async {
      let config = ClientConfig::default().with_auth().await.map_err(|e| {
        Box::new(std::io::Error::other(format!(
          "Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?"
        ))) as Box<dyn std::error::Error>
      })?;
      Ok(Client::new(config))
    })
    .await
}

/// Build a deterministic fingerprint of a directory by listing files and their last-modified timestamps.
///
/// This runs every [`RELOAD_POLL_INTERVAL`](crate) tick, so it takes its
/// timestamps from the directory listing itself rather than issuing a metadata
/// request per file — one GCS request per poll instead of one per design.
///
/// # Errors
///
/// Returns an error if the directory cannot be listed.
pub async fn get_local_or_cloud_dir_fingerprint(
  dir: &str,
) -> Result<Vec<(String, Option<i64>)>, Box<dyn std::error::Error>> {
  let mut fingerprint = list_local_or_cloud_dir_with_timestamps(dir).await?;
  fingerprint.sort_unstable();

  Ok(fingerprint)
}

/**
 * Read a file from the local filesystem or GCS.
 * Given this function returns all the content in the file, its not great for large files, but 100% okay
 * for config files and scenarios (as is our case).
 * General utility routine to be used in a few places.
 *
 * # Errors
 *
 * Will return `Err` if the file cannot be read or if GCS cannot be reached (depending on url of file)
 *
 */
pub async fn read_local_or_cloud_file(filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
  debug!("(read_local_or_cloud_file) Reading file {filename}");
  // Check if the filename is a GCS path
  if filename.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = filename.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    let client = gcs_client().await?;

    // Read the file from GCS
    let data = client
      .download_object(
        &GetObjectRequest {
          bucket: bucket_name.to_string(),
          object: object_name,
          ..Default::default()
        },
        &Range::default(),
      )
      .await?;
    Ok(data)
  } else {
    // Read the file locally
    let file = File::open(filename)?;
    let mut buf_reader = BufReader::new(file);
    let mut content: Vec<u8> = Vec::with_capacity(1024);
    buf_reader.read_to_end(&mut content)?;
    Ok(content)
  }
}

/// Read a file's bytes plus its GCS generation number (or `None` for local
/// files). Used by callers that intend to follow up with a generation-guarded
/// write so they can detect concurrent modification.
///
/// # Errors
/// Returns an error if the file cannot be read or if GCS cannot be reached.
pub async fn read_local_or_cloud_file_with_generation(
  filename: &str,
) -> Result<(Vec<u8>, Option<i64>), Box<dyn std::error::Error>> {
  if filename.starts_with("gs://") {
    let parts: Vec<&str> = filename.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    let client = gcs_client().await?;

    // Fetch metadata first so we can capture the generation. If the object
    // doesn't exist we still want a clean (empty bytes, None) result so the
    // caller can write a fresh object with `if_generation_match=0`.
    let object_meta = client
      .get_object(&GetObjectRequest {
        bucket: bucket_name.to_string(),
        object: object_name.clone(),
        ..Default::default()
      })
      .await;

    match object_meta {
      Ok(meta) => {
        let data = client
          .download_object(
            &GetObjectRequest {
              bucket: bucket_name.to_string(),
              object: object_name,
              generation: Some(meta.generation),
              ..Default::default()
            },
            &Range::default(),
          )
          .await?;
        Ok((data, Some(meta.generation)))
      }
      Err(GcsHttpError::Response(err)) if err.code == 404 => Ok((Vec::new(), None)),
      Err(e) => Err(Box::new(e)),
    }
  } else {
    match tokio::fs::read(filename).await {
      Ok(data) => Ok((data, None)),
      Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok((Vec::new(), None)),
      Err(e) => Err(Box::new(e)),
    }
  }
}

/// Error returned by [`write_local_or_cloud_file_if_generation_match`].
/// `PreconditionFailed` indicates the GCS generation precondition (HTTP 412)
/// was rejected and the caller should re-read and retry. `Other` wraps any
/// non-precondition error.
#[derive(Debug)]
pub enum GenerationWriteError {
  PreconditionFailed,
  Other(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for GenerationWriteError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      GenerationWriteError::PreconditionFailed => write!(f, "GCS precondition failed (generation mismatch)"),
      GenerationWriteError::Other(e) => write!(f, "{e}"),
    }
  }
}

impl std::error::Error for GenerationWriteError {}

/// Write bytes to a file with an optional GCS generation precondition. For
/// local paths the precondition is a no-op. For `gs://` paths the request
/// uses `if_generation_match`: `Some(gen)` requires the live object to be at
/// generation `gen`; `None` means "create-only" (`if_generation_match=0`),
/// which only succeeds when there is no live version. Returns the new
/// generation on success.
///
/// # Errors
/// Returns `GenerationWriteError::PreconditionFailed` on HTTP 412 from GCS.
/// Returns `GenerationWriteError::Other` for any other failure (network,
/// auth, local I/O).
pub async fn write_local_or_cloud_file_if_generation_match(
  filename: &str, contents: Vec<u8>, expected: Option<i64>,
) -> Result<i64, GenerationWriteError> {
  debug!(
    "(write_local_or_cloud_file_if_generation_match) Writing {} bytes to {filename} (expected gen={:?})",
    contents.len(),
    expected
  );

  if let Some(rest) = filename.strip_prefix("gs://") {
    let mut parts = rest.splitn(2, '/');
    let bucket_name = parts.next().ok_or_else(|| {
      GenerationWriteError::Other(Box::new(std::io::Error::other(format!("Malformed GCS path: {filename}"))))
    })?;
    let object_name = parts.next().ok_or_else(|| {
      GenerationWriteError::Other(Box::new(std::io::Error::other(format!(
        "Malformed GCS path (missing object): {filename}"
      ))))
    })?;

    let client = gcs_client()
      .await
      .map_err(|e| GenerationWriteError::Other(Box::new(std::io::Error::other(e.to_string()))))?;
    let upload_type = UploadType::Simple(Media::new(object_name.to_string()));

    // Setting `if_generation_match=Some(0)` only succeeds when there is no live
    // object. Some non-zero value requires the current generation to match.
    let if_generation_match = Some(expected.unwrap_or(0));

    let result = client
      .upload_object(
        &UploadObjectRequest {
          bucket: bucket_name.to_string(),
          if_generation_match,
          ..Default::default()
        },
        contents,
        &upload_type,
      )
      .await;

    match result {
      Ok(obj) => Ok(obj.generation),
      Err(GcsHttpError::Response(err)) if err.code == 412 => Err(GenerationWriteError::PreconditionFailed),
      Err(e) => Err(GenerationWriteError::Other(Box::new(std::io::Error::other(e.to_string())))),
    }
  } else {
    // Local file: no precondition enforcement. Single-replica deployment plus
    // the in-process register lock makes this safe within the server; CLI vs
    // server races on local filesystem are the user's problem.
    tokio::fs::write(filename, contents)
      .await
      .map_err(|e| GenerationWriteError::Other(Box::new(e)))?;
    Ok(0)
  }
}

/// Write bytes to a file on the local filesystem or to a GCS object.
///
/// Mirrors the behavior of [`read_local_or_cloud_file`]: if the path begins
/// with `gs://` it's uploaded to GCS, otherwise it's written to disk.
/// Existing files / objects are overwritten unconditionally — caller is
/// responsible for any "are you sure" / ownership checks.
///
/// # Errors
/// Returns an error if the local file cannot be written or if the GCS upload fails.
pub async fn write_local_or_cloud_file(filename: &str, contents: Vec<u8>) -> Result<(), Box<dyn std::error::Error>> {
  debug!("(write_local_or_cloud_file) Writing {} bytes to {filename}", contents.len());
  if let Some(rest) = filename.strip_prefix("gs://") {
    let mut parts = rest.splitn(2, '/');
    let bucket_name = parts
      .next()
      .ok_or_else(|| std::io::Error::other(format!("Malformed GCS path: {filename}")))?;
    let object_name = parts
      .next()
      .ok_or_else(|| std::io::Error::other(format!("Malformed GCS path (missing object): {filename}")))?;

    let client = gcs_client().await?;
    let upload_type = UploadType::Simple(Media::new(object_name.to_string()));
    client
      .upload_object(
        &UploadObjectRequest {
          bucket: bucket_name.to_string(),
          ..Default::default()
        },
        contents,
        &upload_type,
      )
      .await?;
    Ok(())
  } else {
    tokio::fs::write(filename, contents).await?;
    Ok(())
  }
}

/// List the files in a directory.  The directory can be local or on Google cloud storage (encoded in filename)
///
/// # Errors
/// If the directory cannot be read or if GCS cannot be reached (depending on url of file)
///
pub async fn list_local_or_cloud_dir(dir: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
  Ok(
    list_local_or_cloud_dir_with_timestamps(dir)
      .await?
      .into_iter()
      .map(|(name, _)| name)
      .collect(),
  )
}

/// List the files in a directory along with each file's last-modified time.
///
/// The GCS listing already carries every object's `updated` field, so pairing
/// the two here costs one request for the whole directory. That is what lets
/// [`get_local_or_cloud_dir_fingerprint`] poll without a `get_object` per file.
///
/// `None` for a timestamp means the backing store did not report one; it is not
/// an error.
///
/// # Errors
/// If the directory cannot be read or if GCS cannot be reached (depending on url of file)
async fn list_local_or_cloud_dir_with_timestamps(
  dir: &str,
) -> Result<Vec<(String, Option<i64>)>, Box<dyn std::error::Error>> {
  if dir.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = dir.split('/').collect();
    let bucket_name = parts[2];

    let client = gcs_client().await?;

    // List the files in the directory
    let objects = client
      .list_objects(&ListObjectsRequest {
        bucket: bucket_name.to_string(),
        ..Default::default()
      })
      .await?;

    // `items` is absent rather than empty for an empty bucket.
    Ok(
      objects
        .items
        .unwrap_or_default()
        .into_iter()
        .map(|object| (object.name, object.updated.map(time::OffsetDateTime::unix_timestamp)))
        .collect(),
    )
  } else {
    // List the files locally
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
      let entry = entry?;
      // Stat through the path rather than the `DirEntry` so symlinked files are
      // followed, matching what `Path::is_file` used to do here.
      let metadata = std::fs::metadata(entry.path())?;
      if metadata.is_file() {
        files.push((entry.file_name().to_string_lossy().into_owned(), file_modified_unix(&metadata)));
      }
    }
    Ok(files)
  }
}

/// Last-modified time of a local file as a Unix timestamp, or `None` if the
/// platform does not report one (or it predates the epoch / overflows `i64`).
fn file_modified_unix(metadata: &std::fs::Metadata) -> Option<i64> {
  metadata
    .modified()
    .ok()
    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
    .and_then(|d| i64::try_from(d.as_secs()).ok())
}

/// Get the last modified timestamp for a file, supporting both local files and Google Cloud Storage files.
/// Google Cloud Storage files are denoted by starting with "gs://" similar to `read_local_or_cloud_file`.
///
/// # Arguments
/// * `filename` - The path to the file. Local paths are used as-is, GCS paths should start with "gs://"
///
/// # Returns
/// The last modified timestamp as a Unix timestamp (seconds since epoch), or `None` if the timestamp is not available
///
/// # Errors
/// Returns `Err` if the file cannot be accessed or if GCS cannot be reached (depending on the file URL)
///
/// # Examples
///
/// ```rust,no_run
/// use callisto::get_file_last_modified_timestamp;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     // Get timestamp for a local file
///     let local_timestamp = get_file_last_modified_timestamp("./config/settings.json").await?;
///     if let Some(timestamp) = local_timestamp {
///         println!("Local file last modified: {}", timestamp);
///     }
///
///     // Get timestamp for a Google Cloud Storage file
///     let gcs_timestamp = get_file_last_modified_timestamp("gs://my-bucket/config/settings.json").await?;
///     if let Some(timestamp) = gcs_timestamp {
///         println!("GCS file last modified: {}", timestamp);
///     }
///
///     Ok(())
/// }
/// ```
pub async fn get_file_last_modified_timestamp(filename: &str) -> Result<Option<i64>, Box<dyn std::error::Error>> {
  // Check if the filename is a GCS path
  if filename.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = filename.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    let client = gcs_client().await?;

    // Get the object metadata from GCS
    let object = client
      .get_object(&GetObjectRequest {
        bucket: bucket_name.to_string(),
        object: object_name,
        ..Default::default()
      })
      .await?;

    // Return the updated timestamp (last modified time) as Unix timestamp
    if let Some(updated) = object.updated {
      Ok(Some(updated.unix_timestamp()))
    } else {
      Ok(None)
    }
  } else {
    // Get the file metadata locally
    let metadata = std::fs::metadata(filename)?;
    let modified_time = metadata.modified()?;

    // Convert SystemTime to Unix timestamp
    let duration_since_epoch = modified_time.duration_since(std::time::UNIX_EPOCH)?;
    #[allow(clippy::cast_possible_wrap)]
    Ok(Some(duration_since_epoch.as_secs() as i64))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_replace_scenarios_updates_snapshot() {
    replace_scenarios(vec![(
      "scenario-a".to_string(),
      MetaData {
        name: "Scenario A".to_string(),
        description: "first".to_string(),
        owner: "test-user".to_string(),
      },
    )]);
    assert_eq!(get_scenarios_snapshot().len(), 1);

    replace_scenarios(vec![(
      "scenario-b".to_string(),
      MetaData {
        name: "Scenario B".to_string(),
        description: "second".to_string(),
        owner: "test-user".to_string(),
      },
    )]);

    let scenarios = get_scenarios_snapshot();
    assert_eq!(scenarios.len(), 1);
    assert_eq!(scenarios[0].0, "scenario-b");
    assert_eq!(scenarios[0].1.name, "Scenario B");
  }

  #[tokio::test]
  async fn test_get_file_last_modified_timestamp_local() {
    // Test with a local file that should exist (Cargo.toml)
    let result = get_file_last_modified_timestamp("Cargo.toml").await;
    assert!(result.is_ok());
    let timestamp = result.unwrap();
    assert!(timestamp.is_some());
    assert!(timestamp.unwrap() > 0);
  }

  #[tokio::test]
  async fn test_get_file_last_modified_timestamp_nonexistent() {
    // Test with a file that doesn't exist
    let result = get_file_last_modified_timestamp("nonexistent_file.txt").await;
    assert!(result.is_err());
  }

  #[test]
  fn test_extract_scenario_owner_well_formed() {
    let json = br#"{"metadata":{"name":"S","description":"","owner":"alice@example.com"},"ships":[]}"#;
    assert_eq!(extract_scenario_owner(json), "alice@example.com");
  }

  #[test]
  fn test_extract_scenario_owner_missing_owner_field() {
    let json = br#"{"metadata":{"name":"S","description":""},"ships":[]}"#;
    assert_eq!(extract_scenario_owner(json), "");
  }

  #[test]
  fn test_extract_scenario_owner_missing_metadata_block() {
    let json = br#"{"ships":[]}"#;
    assert_eq!(extract_scenario_owner(json), "");
  }

  #[test]
  fn test_extract_scenario_owner_malformed_json() {
    let json = b"not even json";
    assert_eq!(extract_scenario_owner(json), "");
  }

  /// Scratch directory helper for the directory-listing tests. Returns a fresh
  /// empty directory that the caller is responsible for removing.
  fn make_scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .expect("clock before epoch")
      .as_nanos();
    let dir = std::env::temp_dir().join(format!("callisto_{tag}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("unable to create scratch dir");
    dir
  }

  /// The fingerprint is sorted by name and carries a timestamp per file. It is
  /// polled every reload tick, so it must be cheap AND stable: two calls with
  /// nothing touched have to compare equal or the watcher reloads forever.
  #[tokio::test]
  async fn test_dir_fingerprint_is_sorted_and_stable() {
    let dir = make_scratch_dir("fingerprint");
    for name in ["c.json", "a.json", "b.json"] {
      std::fs::write(dir.join(name), b"{}").expect("unable to write scratch file");
    }
    // A subdirectory must not appear: only files are fingerprinted.
    std::fs::create_dir(dir.join("nested")).expect("unable to create nested dir");

    let dir_str = dir.to_str().expect("non-utf8 scratch path");
    let fingerprint = get_local_or_cloud_dir_fingerprint(dir_str).await.unwrap();

    let names: Vec<&str> = fingerprint.iter().map(|(name, _)| name.as_str()).collect();
    assert_eq!(names, vec!["a.json", "b.json", "c.json"], "Fingerprint must be name-sorted");
    assert!(
      fingerprint.iter().all(|(_, ts)| ts.is_some()),
      "Local files must report a last-modified timestamp"
    );

    let again = get_local_or_cloud_dir_fingerprint(dir_str).await.unwrap();
    assert_eq!(fingerprint, again, "An untouched directory must fingerprint identically");

    std::fs::remove_dir_all(&dir).ok();
  }

  /// A changed file must change the fingerprint, otherwise nothing ever reloads.
  #[tokio::test]
  async fn test_dir_fingerprint_changes_when_a_file_is_touched() {
    let dir = make_scratch_dir("fingerprint_touch");
    let file = dir.join("a.json");
    std::fs::write(&file, b"{}").expect("unable to write scratch file");
    let dir_str = dir.to_str().expect("non-utf8 scratch path");

    let before = get_local_or_cloud_dir_fingerprint(dir_str).await.unwrap();

    // Timestamps are whole seconds, so set the mtime explicitly rather than
    // rewriting and hoping the clock ticked.
    let bumped = std::time::SystemTime::now() + std::time::Duration::from_secs(120);
    std::fs::File::options()
      .write(true)
      .open(&file)
      .expect("unable to open scratch file")
      .set_modified(bumped)
      .expect("unable to set mtime");

    let after = get_local_or_cloud_dir_fingerprint(dir_str).await.unwrap();
    assert_ne!(before, after, "A touched file must change the directory fingerprint");

    std::fs::remove_dir_all(&dir).ok();
  }

  /// `list_local_or_cloud_dir` is now a projection of the timestamped listing.
  /// It must still return bare file names, and still skip directories.
  #[tokio::test]
  async fn test_list_local_dir_returns_file_names_only() {
    let dir = make_scratch_dir("listdir");
    std::fs::write(dir.join("only.json"), b"{}").expect("unable to write scratch file");
    std::fs::create_dir(dir.join("nested")).expect("unable to create nested dir");

    let files = list_local_or_cloud_dir(dir.to_str().expect("non-utf8 scratch path"))
      .await
      .unwrap();
    assert_eq!(files, vec!["only.json".to_string()]);

    std::fs::remove_dir_all(&dir).ok();
  }

  #[test]
  fn test_replace_and_get_scenario_failures() {
    replace_scenario_failures(vec![ScenarioFailure {
      filename: "Broken.json".to_string(),
      owner: "alice@example.com".to_string(),
      error: "missing design".to_string(),
    }]);
    let snap = get_scenario_failures_snapshot();
    assert_eq!(snap.len(), 1);
    assert_eq!(snap[0].filename, "Broken.json");

    replace_scenario_failures(Vec::new());
    assert_eq!(get_scenario_failures_snapshot().len(), 0);
  }

  /// Pins the failure semantics [`gcs_client`] depends on.
  ///
  /// The shared GCS client must not cache a failed build: a transient
  /// metadata-server error at startup would otherwise poison the process for
  /// its whole life. `tokio::sync::OnceCell::get_or_try_init` gives us that -
  /// on `Err` nothing is stored and the next caller retries. This test exists
  /// so that swapping in a plain `OnceCell`, or caching a `Result`, fails
  /// loudly rather than silently reintroducing the bug.
  #[tokio::test]
  async fn once_cell_does_not_cache_initialisation_failures() {
    let cell: tokio::sync::OnceCell<u32> = tokio::sync::OnceCell::const_new();
    let attempts = std::sync::atomic::AtomicU32::new(0);

    let init = || async {
      let n = attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
      if n < 3 {
        Err("metadata server unavailable")
      } else {
        Ok(n)
      }
    };

    assert!(cell.get_or_try_init(init).await.is_err(), "first attempt should fail");
    assert!(cell.get().is_none(), "a failed attempt must not be cached");
    assert!(cell.get_or_try_init(init).await.is_err(), "second attempt should fail");
    assert_eq!(*cell.get_or_try_init(init).await.unwrap(), 3, "third attempt should succeed");
    assert_eq!(*cell.get_or_try_init(init).await.unwrap(), 3, "success must now be cached");
    assert_eq!(
      attempts.load(std::sync::atomic::Ordering::SeqCst),
      3,
      "once initialised, no further attempts should be made"
    );
  }

  /// Concurrent callers must share a single initialisation rather than each
  /// starting their own. This is what collapses an N-file directory load into
  /// one token request instead of N, which is the actual fix for the metadata
  /// server refusing a burst of simultaneous requests.
  #[tokio::test]
  async fn once_cell_initialises_once_under_concurrency() {
    let cell: std::sync::Arc<tokio::sync::OnceCell<u32>> = std::sync::Arc::new(tokio::sync::OnceCell::const_new());
    let attempts = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

    let mut handles = Vec::new();
    for _ in 0..64 {
      let cell = cell.clone();
      let attempts = attempts.clone();
      handles.push(tokio::spawn(async move {
        *cell
          .get_or_init(|| async {
            attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            7_u32
          })
          .await
      }));
    }
    for h in handles {
      assert_eq!(h.await.unwrap(), 7);
    }
    assert_eq!(
      attempts.load(std::sync::atomic::Ordering::SeqCst),
      1,
      "64 concurrent callers must produce exactly one initialisation"
    );
  }
}

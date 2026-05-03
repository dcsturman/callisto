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
use std::fs::File;
use std::io::{BufReader, Read};
use std::sync::{Arc, RwLock};

pub type ScenarioMetadataList = Vec<(String, MetaData)>;
type SharedScenarioMetadataList = Arc<ScenarioMetadataList>;

pub static SCENARIOS: OnceCell<RwLock<SharedScenarioMetadataList>> = OnceCell::new();
pub const LOG_FILE_USE: &str = "READ_FILE";
pub const LOG_AUTH_RESULT: &str = "LOGIN_ATTEMPT";
pub const LOGOUT: &str = "LOGOUT";
pub const LOG_SCENARIO_ACTIVITY: &str = "SCENARIO";

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

fn join_dir_entry_path(dir: &str, entry: &str) -> String {
  format!("{}/{entry}", dir.trim_end_matches('/'))
}

async fn create_gcs_client() -> Result<Client, Box<dyn std::error::Error>> {
  let config = ClientConfig::default().with_auth().await.map_err(|e| {
    Box::new(std::io::Error::other(format!(
      "Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?"
    ))) as Box<dyn std::error::Error>
  })?;

  Ok(Client::new(config))
}

/// Build a deterministic fingerprint of a directory by listing files and their last-modified timestamps.
///
/// # Errors
///
/// Returns an error if the directory cannot be listed or if any file timestamp cannot be read.
pub async fn get_local_or_cloud_dir_fingerprint(
  dir: &str,
) -> Result<Vec<(String, Option<i64>)>, Box<dyn std::error::Error>> {
  let mut files = list_local_or_cloud_dir(dir).await?;
  files.sort_unstable();

  let mut fingerprint = Vec::with_capacity(files.len());
  for file in files {
    let full_path = join_dir_entry_path(dir, &file);
    let last_modified = get_file_last_modified_timestamp(&full_path).await?;
    fingerprint.push((file, last_modified));
  }

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

    let client = create_gcs_client().await?;

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

    let client = create_gcs_client().await?;

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

    let client = create_gcs_client()
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

    let client = create_gcs_client().await?;
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
/// # Panics
/// Panics if the GCS list response omits the `items` field.
///
pub async fn list_local_or_cloud_dir(dir: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
  if dir.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = dir.split('/').collect();
    let bucket_name = parts[2];

    let client = create_gcs_client().await?;

    // List the files in the directory
    let objects = client
      .list_objects(&ListObjectsRequest {
        bucket: bucket_name.to_string(),
        ..Default::default()
      })
      .await?;
    let mut files = Vec::new();
    for object in objects.items.unwrap() {
      files.push(object.name);
    }
    Ok(files)
  } else {
    // List the files locally
    let mut files = Vec::new();
    for entry in std::fs::read_dir(dir)? {
      let entry = entry?;
      let path = entry.path();
      if path.is_file() {
        files.push(entry.file_name().to_string_lossy().into_owned());
      }
    }
    Ok(files)
  }
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

    let client = create_gcs_client().await?;

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
}

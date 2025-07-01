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
use once_cell::sync::OnceCell;
use std::fs::File;
use std::io::{BufReader, Read};

pub static SCENARIOS: OnceCell<Vec<(String, MetaData)>> = OnceCell::new();
pub const LOG_FILE_USE: &str = "READ_FILE";
pub const LOG_AUTH_RESULT: &str = "LOGIN_ATTEMPT";
pub const LOG_SCENARIO_ACTIVITY: &str = "SCENARIO";

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
 * # Panics
 *
 * Will panic with a helpful message if GCS authentication fails.  GCS authentication needs to be handled outside (and prior to)
 * this function.
 */
pub async fn read_local_or_cloud_file(filename: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
  debug!("(read_local_or_cloud_file) Reading file {filename}");
  // Check if the filename is a GCS path
  if filename.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = filename.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    // Create a GCS client
    let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
      panic!("Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?")
    });

    let client = Client::new(config);

    // Read the file from GCS
    let data = client
      .download_object(
        &GetObjectRequest {
          bucket: bucket_name.to_string(),
          object: object_name.to_string(),
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

/// List the files in a directory.  The directory can be local or on Google cloud storage (encoded in filename)
///
/// # Errors
/// If the directory cannot be read or if GCS cannot be reached (depending on url of file)
///
/// # Panics
/// If GCS authentication fails.  GCS authentication needs to be handled outside (and prior to) this function.
pub async fn list_local_or_cloud_dir(dir: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
  if dir.starts_with("gs://") {
    // Create a GCS client
    let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
      panic!("Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?")
    });
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = dir.split('/').collect();
    let bucket_name = parts[2];

    let client = Client::new(config);

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
/// # Panics
/// Will panic with a helpful message if GCS authentication fails. GCS authentication needs to be handled outside (and prior to) this function.
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
  debug!("(get_file_last_modified_timestamp) Getting last modified timestamp for file {filename}");

  // Check if the filename is a GCS path
  if filename.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = filename.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    // Create a GCS client
    let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
      panic!("Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?")
    });

    let client = Client::new(config);

    // Get the object metadata from GCS
    let object = client
      .get_object(&GetObjectRequest {
        bucket: bucket_name.to_string(),
        object: object_name.to_string(),
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

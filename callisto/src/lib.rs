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
pub mod tests;

use google_cloud_storage::client::{Client, ClientConfig};
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use std::fs::File;
use std::io::{BufReader, Read};
use once_cell::sync::OnceCell;

pub static SCENARIOS: OnceCell<Vec<String>> = OnceCell::new();

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

pub async fn list_local_or_cloud_dir(dir: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
  if dir.starts_with("gs://") {
    // Extract bucket name from the GCS URI
    let parts: Vec<&str> = dir.split('/').collect();
    let bucket_name = parts[2];
    let object_name = parts[3..].join("/");

    // Create a GCS client
    let config = ClientConfig::default().with_auth().await.unwrap_or_else(|e| {
      panic!("Error {e} authenticating with GCS. Did you do `gcloud auth application-default login` before running?")
    });

    let client = Client::new(config);

    // List the files in the directory
    let objects = client.list_objects(&ListObjectsRequest{bucket: dir.to_string(), ..Default::default()}).await?;
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
        files.push(path.to_string_lossy().into_owned());
      }
    }
    Ok(files)
  }
  
}

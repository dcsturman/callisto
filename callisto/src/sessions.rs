//! Sign-ins that outlast the server.
//!
//! A session key lives in the browser's cookie and, once its owner signs in,
//! maps to their email in the processor's in-memory `session_keys`. That map
//! dies with the process, and Cloud Run ends the process on every deploy and
//! whenever the server sits idle, so without this module every one of those
//! sent players back through Google. Each Google sign-in costs the player a
//! "Keep track of your Google Account data" email.
//!
//! Here each sign-in is also written to a small file beside the users file
//! (`sessions.json`, local or `gs://`), keyed by a SHA-256 of the session key,
//! never the key itself, so the file does not hold usable credentials. A fresh
//! server reads it at start, and the WebSocket handshake recalls an unknown
//! cookie from it. Sign-ins last [`SESSION_LIFETIME_SECS`]; the cookie carries
//! the same lifetime.
//!
//! Nothing here is set up in tests or `--test` mode, and every call is then a
//! no-op, so callers need not care.

use std::collections::HashMap;
use std::fmt::Write;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{debug, info, warn};

use crate::{
  read_local_or_cloud_file_with_generation, write_local_or_cloud_file_if_generation_match, GenerationWriteError,
};

/// How long a sign-in lasts: 30 days.
pub const SESSION_LIFETIME_SECS: i64 = 30 * 24 * 60 * 60;

/// Attempts at a write when another writer got in first.
const WRITE_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
  email: String,
  /// Unix seconds.
  expires: i64,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionsFile {
  version: u32,
  /// Hashed session key to its sign-in.
  sessions: HashMap<String, Entry>,
}

struct Store {
  file: String,
  known: Mutex<HashMap<String, Entry>>,
}

static STORE: OnceLock<Store> = OnceLock::new();

fn now() -> i64 {
  SystemTime::now()
    .duration_since(UNIX_EPOCH)
    .map_or(0, |since| i64::try_from(since.as_secs()).unwrap_or(i64::MAX))
}

fn hash(session_key: &str) -> String {
  Sha256::digest(session_key.as_bytes())
    .iter()
    .fold(String::with_capacity(64), |mut hex, byte| {
      let _ = write!(hex, "{byte:02x}");
      hex
    })
}

fn parse(body: &[u8]) -> SessionsFile {
  if body.is_empty() {
    return SessionsFile::default();
  }
  serde_json::from_slice(body).unwrap_or_else(|e| {
    warn!("(sessions) Unreadable sessions file, starting it afresh: {e:?}");
    SessionsFile::default()
  })
}

/// The sessions file that sits beside a users file.
#[must_use]
pub fn file_beside(users_file: &str) -> String {
  users_file
    .rsplit_once('/')
    .map_or_else(|| "sessions.json".to_string(), |(dir, _)| format!("{dir}/sessions.json"))
}

/// Read the sessions file and start remembering sign-ins. A missing or
/// unreadable file starts empty: players then just sign in again.
pub async fn init(file: &str) {
  let known = match read_local_or_cloud_file_with_generation(file).await {
    Ok((body, _)) => {
      let now = now();
      parse(&body)
        .sessions
        .into_iter()
        .filter(|(_, entry)| entry.expires > now)
        .collect()
    }
    Err(e) => {
      warn!("(sessions) Could not read {file}; starting with no remembered sign-ins: {e:?}");
      HashMap::new()
    }
  };
  info!("(sessions) Remembering sign-ins in {file}; {} current.", known.len());
  if STORE
    .set(Store {
      file: file.to_string(),
      known: Mutex::new(known),
    })
    .is_err()
  {
    warn!("(sessions) Already set up; ignoring {file}.");
  }
}

/// The email signed in under this session key, if a sign-in is remembered
/// and has not expired.
///
/// # Panics
/// If the store's lock is poisoned.
#[must_use]
pub fn recall(session_key: &str) -> Option<String> {
  let store = STORE.get()?;
  let known = store.known.lock().unwrap();
  known
    .get(&hash(session_key))
    .filter(|entry| entry.expires > now())
    .map(|entry| entry.email.clone())
}

/// Remember that `email` signed in under this session key, for
/// [`SESSION_LIFETIME_SECS`]. The file is written in the background.
///
/// # Panics
/// If the store's lock is poisoned.
pub fn remember(session_key: &str, email: &str) {
  let Some(store) = STORE.get() else { return };
  let key = hash(session_key);
  let entry = Entry {
    email: email.to_string(),
    expires: now() + SESSION_LIFETIME_SECS,
  };
  store.known.lock().unwrap().insert(key.clone(), entry.clone());
  // Cloned per attempt: a retried write applies the change again.
  tokio::spawn(persist(&store.file, move |sessions| {
    sessions.insert(key.clone(), entry.clone());
  }));
}

/// Forget the sign-in under this session key: signing out. The file is
/// written in the background.
///
/// # Panics
/// If the store's lock is poisoned.
pub fn forget(session_key: &str) {
  let Some(store) = STORE.get() else { return };
  let key = hash(session_key);
  if store.known.lock().unwrap().remove(&key).is_none() {
    return;
  }
  tokio::spawn(persist(&store.file, move |sessions| {
    sessions.remove(&key);
  }));
}

/// Apply one change to the file as it is now, dropping expired sign-ins on
/// the way. Another server can write between our read and our write (two
/// revisions overlap during a deploy), so the write only lands if the file is
/// unchanged, and otherwise re-reads and tries again.
async fn persist(file: &str, change: impl Fn(&mut HashMap<String, Entry>)) {
  for attempt in 1..=WRITE_ATTEMPTS {
    let (body, generation) = match read_local_or_cloud_file_with_generation(file).await {
      Ok(read) => read,
      Err(e) => {
        warn!("(sessions) Could not read {file} to update it: {e:?}");
        return;
      }
    };
    let mut contents = parse(&body);
    let now = now();
    contents.version = 1;
    contents.sessions.retain(|_, entry| entry.expires > now);
    change(&mut contents.sessions);
    let bytes = serde_json::to_vec(&contents).expect("(sessions) A sessions file always serializes.");
    match write_local_or_cloud_file_if_generation_match(file, bytes, generation).await {
      Ok(_) => {
        debug!("(sessions) Wrote {file}: {} sign-ins.", contents.sessions.len());
        return;
      }
      Err(GenerationWriteError::PreconditionFailed) => {
        debug!("(sessions) {file} changed under us (attempt {attempt}/{WRITE_ATTEMPTS}); re-reading.");
      }
      Err(GenerationWriteError::Other(e)) => {
        warn!("(sessions) Could not write {file}: {e:?}");
        return;
      }
    }
  }
  warn!("(sessions) Gave up writing {file} after {WRITE_ATTEMPTS} attempts.");
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn the_file_sits_beside_the_users_file() {
    assert_eq!(
      file_beside("gs://callisto-be-user-profiles/authorized_users.json"),
      "gs://callisto-be-user-profiles/sessions.json"
    );
    assert_eq!(file_beside("./config/authorized_users.json"), "./config/sessions.json");
    assert_eq!(file_beside("authorized_users.json"), "sessions.json");
  }

  #[test]
  fn keys_are_stored_hashed() {
    let key = "Bearer abc";
    assert_eq!(hash(key).len(), 64);
    assert!(!hash(key).contains("abc"));
    assert_eq!(hash(key), hash(key));
    assert_ne!(hash(key), hash("Bearer abd"));
  }

  #[test]
  fn an_unreadable_file_starts_empty() {
    assert!(parse(b"").sessions.is_empty());
    assert!(parse(b"not json").sessions.is_empty());
    let file = parse(br#"{"version":1,"sessions":{"h":{"email":"a@b.c","expires":5}}}"#);
    assert_eq!(file.sessions["h"].email, "a@b.c");
  }

  /// The whole round trip through a real file: a sign-in survives a "restart"
  /// (a fresh read of the file), and signing out removes it. One test, since
  /// the store is process-wide.
  #[tokio::test(flavor = "multi_thread")]
  async fn a_sign_in_outlasts_the_server_and_signing_out_ends_it() {
    let dir = std::env::temp_dir().join(format!("callisto-sessions-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("sessions.json").to_string_lossy().to_string();
    let _ = std::fs::remove_file(&file);

    // A stale sign-in already in the file is not recalled.
    let stale = SessionsFile {
      version: 1,
      sessions: HashMap::from([(
        hash("old"),
        Entry {
          email: "old@x.y".into(),
          expires: now() - 1,
        },
      )]),
    };
    std::fs::write(&file, serde_json::to_vec(&stale).unwrap()).unwrap();

    init(&file).await;
    assert_eq!(recall("old"), None);
    assert_eq!(recall("Bearer k1"), None);

    remember("Bearer k1", "player@example.com");
    assert_eq!(recall("Bearer k1").as_deref(), Some("player@example.com"));
    // The background write lands.
    let written = || parse(&std::fs::read(&file).unwrap_or_default());
    for _ in 0..100 {
      if written().sessions.contains_key(&hash("Bearer k1")) {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    let on_disk = written();
    assert_eq!(on_disk.sessions[&hash("Bearer k1")].email, "player@example.com");
    assert!(
      !on_disk.sessions.contains_key(&hash("old")),
      "expired sign-ins are dropped on write"
    );
    assert!(
      !std::fs::read_to_string(&file).unwrap().contains("Bearer k1"),
      "the raw key is never written"
    );

    forget("Bearer k1");
    assert_eq!(recall("Bearer k1"), None);
    for _ in 0..100 {
      if !written().sessions.contains_key(&hash("Bearer k1")) {
        break;
      }
      tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert!(!written().sessions.contains_key(&hash("Bearer k1")));
    let _ = std::fs::remove_dir_all(&dir);
  }
}

//! The models a commander can ask, all on Vertex AI.
//!
//! Gemini and Claude are both reached through the GCP project's own
//! credentials and billed to it: a service account's on Cloud Run, the
//! developer's application-default credentials locally. They differ in request
//! shape (`generateContent` against the Anthropic Messages API), in which
//! region serves them, and in how the forced tool call comes back. Both are
//! asked the same thing: here is the situation, call `plan_round`.

use std::sync::Arc;
use std::time::Duration;

use google_cloud_auth::project::Config;
use google_cloud_auth::token::DefaultTokenSourceProvider;
use google_cloud_token::{TokenSource, TokenSourceProvider};
use serde_json::{json, Value};
use tracing::warn;

use crate::decision::{self, TeamDecision};

/// Which publisher's model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Family {
  Gemini,
  Claude,
}

/// A model, named as `gemini:<model>` or `claude:<model>`; `gemini` and
/// `claude` alone pick the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
  pub family: Family,
  pub model: String,
}

/// The cheap, fast tier of each, for the harness's hundreds of games.
pub const DEFAULT_GEMINI: &str = "gemini-3.8-flash";
pub const DEFAULT_CLAUDE: &str = "claude-sonnet-5-5";

impl ModelSpec {
  /// `None` if the name is not a model's.
  #[must_use]
  pub fn parse(name: &str) -> Option<Self> {
    let (family, model) = name.split_once(':').unwrap_or((name, ""));
    let (family, default) = match family {
      "gemini" => (Family::Gemini, DEFAULT_GEMINI),
      "claude" => (Family::Claude, DEFAULT_CLAUDE),
      _ => return None,
    };
    Some(ModelSpec {
      family,
      model: if model.is_empty() { default } else { model }.to_string(),
    })
  }

  /// The name `parse` reads back.
  #[must_use]
  pub fn name(&self) -> String {
    let family = match self.family {
      Family::Gemini => "gemini",
      Family::Claude => "claude",
    };
    format!("{family}:{}", self.model)
  }
}

impl std::fmt::Display for ModelSpec {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.name())
  }
}

/// Why a call produced no decision.
#[derive(Debug)]
pub enum Error {
  /// No credential, or the request never got an answer.
  Transport(String),
  /// The endpoint answered with an error status.
  Refused { status: u16, body: String },
  /// The model answered, but not with a decision we could read.
  BadReply { why: String, raw: String },
}

impl std::fmt::Display for Error {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Error::Transport(why) => write!(f, "could not reach the model: {why}"),
      Error::Refused { status, body } => write!(f, "the model endpoint refused ({status}): {body}"),
      Error::BadReply { why, raw } => write!(f, "could not read the model's answer ({why}): {raw}"),
    }
  }
}

impl std::error::Error for Error {}

/// What came back from one call.
#[derive(Debug, Clone)]
pub struct Reply {
  pub decision: TeamDecision,
  /// The tool call's arguments, as the model sent them.
  pub raw: String,
  pub input_tokens: u64,
  pub cached_tokens: u64,
  pub output_tokens: u64,
}

/// A connection to Vertex AI for one project and region.
pub struct Vertex {
  client: reqwest::Client,
  token: Arc<dyn TokenSource>,
  project: String,
  region: String,
}

const SCOPE: &str = "https://www.googleapis.com/auth/cloud-platform";
const ATTEMPTS: u32 = 3;

impl Vertex {
  /// Connect with the application-default credentials. `project` is the GCP
  /// project the calls are billed to; `None` takes the credential's own.
  ///
  /// # Errors
  /// Returns an error if there are no credentials, or no project.
  pub async fn new(project: Option<&str>, region: &str) -> Result<Self, Error> {
    let config = Config::default().with_scopes(&[SCOPE]);
    let provider = DefaultTokenSourceProvider::new(config)
      .await
      .map_err(|e| Error::Transport(format!("no Google credentials: {e}")))?;
    let project = project
      .map(str::to_string)
      .or(provider.project_id.clone())
      .ok_or_else(|| Error::Transport("no GCP project: set GOOGLE_CLOUD_PROJECT".to_string()))?;
    let client = reqwest::Client::builder()
      .timeout(Duration::from_secs(120))
      .build()
      .map_err(|e| Error::Transport(e.to_string()))?;
    Ok(Vertex {
      client,
      token: provider.token_source(),
      project,
      region: region.to_string(),
    })
  }

  #[must_use]
  pub fn project(&self) -> &str {
    &self.project
  }

  fn endpoint(&self, publisher: &str, model: &str, method: &str) -> String {
    let host = if self.region == "global" {
      "aiplatform.googleapis.com".to_string()
    } else {
      format!("{}-aiplatform.googleapis.com", self.region)
    };
    format!(
      "https://{host}/v1/projects/{}/locations/{}/publishers/{publisher}/models/{model}:{method}",
      self.project, self.region
    )
  }

  /// Ask `model` for this round's decision.
  ///
  /// # Errors
  /// Returns an error if the call fails after retries, or the answer is not a
  /// decision.
  pub async fn plan_round(&self, spec: &ModelSpec, system: &str, situation: &str) -> Result<Reply, Error> {
    let (url, body) = match spec.family {
      Family::Gemini => (
        self.endpoint("google", &spec.model, "generateContent"),
        gemini_request(system, situation),
      ),
      Family::Claude => (
        self.endpoint("anthropic", &spec.model, "rawPredict"),
        claude_request(system, situation),
      ),
    };
    let answer = self.post(&url, &body).await?;
    match spec.family {
      Family::Gemini => gemini_reply(&answer),
      Family::Claude => claude_reply(&answer),
    }
  }

  /// Post, retrying a few times on a rate limit, a server error, or a
  /// dropped connection.
  async fn post(&self, url: &str, body: &Value) -> Result<Value, Error> {
    let mut wait = Duration::from_secs(2);
    for attempt in 1..=ATTEMPTS {
      match self.post_once(url, body).await {
        Ok(answer) => return Ok(answer),
        Err(e) if attempt < ATTEMPTS && e.is_transient() => {
          warn!("(provider) {e}; retrying in {wait:?}.");
          tokio::time::sleep(wait).await;
          wait *= 2;
        }
        Err(e) => return Err(e),
      }
    }
    unreachable!("the loop returns on its last attempt")
  }

  async fn post_once(&self, url: &str, body: &Value) -> Result<Value, Error> {
    let token = self
      .token
      .token()
      .await
      .map_err(|e| Error::Transport(format!("no access token: {e}")))?;
    let response = self
      .client
      .post(url)
      .header("Authorization", token)
      .header("x-goog-user-project", &self.project)
      .json(body)
      .send()
      .await
      .map_err(|e| Error::Transport(e.to_string()))?;
    let status = response.status().as_u16();
    if !response.status().is_success() {
      return Err(Error::Refused {
        status,
        body: response.text().await.unwrap_or_default(),
      });
    }
    response.json().await.map_err(|e| Error::BadReply {
      why: format!("not JSON: {e}"),
      raw: String::new(),
    })
  }
}

impl Error {
  /// Worth trying again: a rate limit, a server error, or the connection.
  #[must_use]
  pub fn is_transient(&self) -> bool {
    match self {
      Error::Transport(_) => true,
      Error::Refused { status, .. } => *status == 429 || *status >= 500,
      Error::BadReply { .. } => false,
    }
  }
}

/// Gemini's schema language is `OpenAPI`'s, with upper-case type names.
fn uppercase_types(schema: &mut Value) {
  match schema {
    Value::Object(map) => {
      if let Some(Value::String(kind)) = map.get_mut("type") {
        *kind = kind.to_uppercase();
      }
      for value in map.values_mut() {
        uppercase_types(value);
      }
    }
    Value::Array(items) => items.iter_mut().for_each(uppercase_types),
    _ => {}
  }
}

fn gemini_request(system: &str, situation: &str) -> Value {
  let mut schema = decision::schema();
  uppercase_types(&mut schema);
  json!({
    "systemInstruction": {"parts": [{"text": system}]},
    "contents": [{"role": "user", "parts": [{"text": situation}]}],
    "tools": [{"functionDeclarations": [{
      "name": decision::TOOL_NAME,
      "description": decision::TOOL_DESCRIPTION,
      "parameters": schema
    }]}],
    "toolConfig": {"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": [decision::TOOL_NAME]}},
    "generationConfig": {"temperature": 0.3, "maxOutputTokens": 8192}
  })
}

fn gemini_reply(answer: &Value) -> Result<Reply, Error> {
  let usage = &answer["usageMetadata"];
  let parts = answer["candidates"][0]["content"]["parts"]
    .as_array()
    .ok_or_else(|| bad("no candidate content", answer))?;
  let args = parts
    .iter()
    .find_map(|part| part.get("functionCall"))
    .filter(|call| call["name"] == decision::TOOL_NAME)
    .map(|call| call["args"].clone())
    .ok_or_else(|| bad("no plan_round call", answer))?;
  Ok(Reply {
    decision: serde_json::from_value(args.clone()).map_err(|e| bad(&e.to_string(), &args))?,
    raw: args.to_string(),
    input_tokens: usage["promptTokenCount"].as_u64().unwrap_or(0),
    cached_tokens: usage["cachedContentTokenCount"].as_u64().unwrap_or(0),
    output_tokens: usage["candidatesTokenCount"].as_u64().unwrap_or(0),
  })
}

fn claude_request(system: &str, situation: &str) -> Value {
  json!({
    "anthropic_version": "vertex-2023-10-16",
    "max_tokens": 8192,
    "temperature": 0.3,
    // The fixed part is cached; the situation is not.
    "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}],
    "messages": [{"role": "user", "content": situation}],
    "tools": [{
      "name": decision::TOOL_NAME,
      "description": decision::TOOL_DESCRIPTION,
      "input_schema": decision::schema()
    }],
    "tool_choice": {"type": "tool", "name": decision::TOOL_NAME}
  })
}

fn claude_reply(answer: &Value) -> Result<Reply, Error> {
  let usage = &answer["usage"];
  let content = answer["content"].as_array().ok_or_else(|| bad("no content", answer))?;
  let input = content
    .iter()
    .find(|block| block["type"] == "tool_use" && block["name"] == decision::TOOL_NAME)
    .map(|block| block["input"].clone())
    .ok_or_else(|| bad("no plan_round call", answer))?;
  Ok(Reply {
    decision: serde_json::from_value(input.clone()).map_err(|e| bad(&e.to_string(), &input))?,
    raw: input.to_string(),
    input_tokens: usage["input_tokens"].as_u64().unwrap_or(0),
    cached_tokens: usage["cache_read_input_tokens"].as_u64().unwrap_or(0),
    output_tokens: usage["output_tokens"].as_u64().unwrap_or(0),
  })
}

fn bad(why: &str, raw: &Value) -> Error {
  Error::BadReply {
    why: why.to_string(),
    raw: raw.to_string(),
  }
}

/// Read a provider's answer without a network: for tests, and for replaying
/// recorded calls.
///
/// # Errors
/// Returns an error if the answer is not a decision.
pub fn parse_reply(family: Family, answer: &Value) -> Result<Reply, Error> {
  match family {
    Family::Gemini => gemini_reply(answer),
    Family::Claude => claude_reply(answer),
  }
}

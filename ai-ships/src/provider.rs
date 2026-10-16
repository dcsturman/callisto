//! The models a commander can ask, all on Vertex AI.
//!
//! Gemini, Claude and Vertex's pay-per-call partner models (`DeepSeek`, Qwen
//! and the rest) are all reached through the GCP project's own credentials
//! and billed to it: a service account's on Cloud Run, the developer's
//! application-default credentials locally. They differ in request shape
//! (`generateContent`, the Anthropic Messages API, and the OpenAI-style chat
//! completions endpoint Vertex serves partner models through), in which region
//! serves them, and in how the forced tool call comes back. All are asked the
//! same thing: here is the situation, call `plan_round`.

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
  /// A partner model Vertex serves as a model (pay per call) through its
  /// OpenAI-compatible chat completions endpoint: `DeepSeek`, Qwen, GLM and
  /// the like. Named by publisher and model, `deepseek-ai/deepseek-v4.1-flash-maas`.
  Maas,
}

/// A model, named as `gemini:<model>`, `claude:<model>` or
/// `maas:<publisher>/<model>`; `gemini`, `claude` and `deepseek` alone pick
/// the defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
  pub family: Family,
  pub model: String,
}

/// The cheap, fast tier of each, for the harness's hundreds of games.
pub const DEFAULT_GEMINI: &str = "gemini-3.8-flash";
pub const DEFAULT_CLAUDE: &str = "claude-sonnet-5-5";
pub const DEFAULT_DEEPSEEK: &str = "deepseek-ai/deepseek-v4.1-flash-maas";

impl ModelSpec {
  /// `None` if the name is not a model's.
  #[must_use]
  pub fn parse(name: &str) -> Option<Self> {
    let (family, model) = name.split_once(':').unwrap_or((name, ""));
    let (family, default) = match family {
      "gemini" => (Family::Gemini, DEFAULT_GEMINI),
      "claude" => (Family::Claude, DEFAULT_CLAUDE),
      "maas" | "deepseek" => (Family::Maas, DEFAULT_DEEPSEEK),
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
      Family::Maas => "maas",
    };
    format!("{family}:{}", self.model)
  }
}

impl std::fmt::Display for ModelSpec {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.write_str(&self.name())
  }
}

/// What a model costs on Vertex, in US dollars per million tokens: input,
/// cached input, and output (thinking included). Only models whose price we
/// have confirmed; for the rest a call's cost is not estimated.
#[must_use]
pub fn price(spec: &ModelSpec) -> Option<(f64, f64, f64)> {
  match spec.model.as_str() {
    "gemini-3.8-flash" => Some((0.75, 0.075, 3.75)),
    // Published rates for Flash-Lite; cached input assumed at a tenth, as for
    // Flash.
    "gemini-3.5-flash-lite" => Some((0.30, 0.03, 2.50)),
    // As listed for Vertex; no cached rate, so repeated input is full price.
    "deepseek-ai/deepseek-v4.1-flash-maas" => Some((0.30, 0.30, 1.20)),
    _ => None,
  }
}

impl Reply {
  /// What this call cost, in US dollars, if the model's price is known.
  #[must_use]
  pub fn cost(&self, spec: &ModelSpec) -> Option<f64> {
    #[allow(clippy::cast_precision_loss)]
    price(spec).map(|(input, cached, output)| {
      let fresh = self.input_tokens.saturating_sub(self.cached_tokens) as f64;
      (fresh * input + self.cached_tokens as f64 * cached + self.output_tokens as f64 * output) / 1_000_000.0
    })
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
  /// Gemini's explicit caches of the fixed part of the prompt, by model:
  /// the cache's resource name and when to stop trusting it.
  caches: tokio::sync::Mutex<std::collections::HashMap<String, (String, std::time::Instant)>>,
}

/// How long a cache lives, and how long we use it before making a new one.
const CACHE_TTL: Duration = Duration::from_hours(1);
const CACHE_TRUSTED: Duration = Duration::from_mins(55);

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
    // A round's answer normally takes 4 to 10 seconds. Now and then a
    // request stalls -- most often on a pooled connection the far end has
    // quietly dropped -- and the whole side waits on it, so give up on one
    // after 40 seconds and retry on a fresh connection, rather than after two
    // minutes. Idle connections are let go after 20 seconds for the same
    // reason: a round between AI turns can be longer than the far end keeps
    // them.
    let client = reqwest::Client::builder()
      .connect_timeout(Duration::from_secs(10))
      .timeout(Duration::from_secs(40))
      .pool_idle_timeout(Duration::from_secs(20))
      .build()
      .map_err(|e| Error::Transport(e.to_string()))?;
    Ok(Vertex {
      client,
      token: provider.token_source(),
      project,
      region: region.to_string(),
      caches: tokio::sync::Mutex::new(std::collections::HashMap::new()),
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
    match spec.family {
      Family::Gemini => self.gemini_round(&spec.model, system, situation).await,
      Family::Claude => {
        let url = self.endpoint("anthropic", &spec.model, "rawPredict");
        claude_reply(&self.post(&url, &claude_request(system, situation)).await?)
      }
      Family::Maas => {
        let url = self.chat_completions();
        maas_reply(&self.post(&url, &maas_request(&spec.model, system, situation)).await?)
      }
    }
  }

  /// Vertex's OpenAI-compatible endpoint, which serves the partner models.
  fn chat_completions(&self) -> String {
    let host = if self.region == "global" {
      "aiplatform.googleapis.com".to_string()
    } else {
      format!("{}-aiplatform.googleapis.com", self.region)
    };
    format!(
      "https://{host}/v1/projects/{}/locations/{}/endpoints/openapi/chat/completions",
      self.project, self.region
    )
  }

  /// A Gemini round, through an explicit cache of the system prompt and tool
  /// when there is one: those tokens are billed at a fraction of the rate.
  /// If the cache cannot be made, or has vanished, the round goes without.
  async fn gemini_round(&self, model: &str, system: &str, situation: &str) -> Result<Reply, Error> {
    let url = self.endpoint("google", model, "generateContent");
    if let Some(cache) = self.gemini_cache(model, system).await {
      match self.post(&url, &gemini_cached_request(&cache, model, situation)).await {
        Ok(answer) => return gemini_reply(&answer),
        Err(Error::Refused { status, body }) if status == 400 || status == 404 || status == 403 => {
          warn!("(provider) The prompt cache was refused ({status}); going without it this round: {body}");
          self.caches.lock().await.remove(model);
        }
        Err(e) => return Err(e),
      }
    }
    gemini_reply(&self.post(&url, &gemini_request(system, situation, model)).await?)
  }

  /// The cache for this model, made if there is none or it is about to lapse.
  async fn gemini_cache(&self, model: &str, system: &str) -> Option<String> {
    let mut caches = self.caches.lock().await;
    if let Some((name, until)) = caches.get(model) {
      if std::time::Instant::now() < *until {
        return Some(name.clone());
      }
    }
    let host = if self.region == "global" {
      "aiplatform.googleapis.com".to_string()
    } else {
      format!("{}-aiplatform.googleapis.com", self.region)
    };
    let url = format!(
      "https://{host}/v1/projects/{}/locations/{}/cachedContents",
      self.project, self.region
    );
    let mut body = gemini_fixed(system);
    body["model"] = json!(format!(
      "projects/{}/locations/{}/publishers/google/models/{model}",
      self.project, self.region
    ));
    body["ttl"] = json!(format!("{}s", CACHE_TTL.as_secs()));
    match self.post(&url, &body).await {
      Ok(made) => {
        let name = made["name"].as_str()?.to_string();
        caches.insert(model.to_string(), (name.clone(), std::time::Instant::now() + CACHE_TRUSTED));
        Some(name)
      }
      Err(e) => {
        warn!("(provider) Could not cache the prompt for {model}; sending it whole. {e}");
        None
      }
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

/// The fixed part of a Gemini request: the system prompt, the one tool, and
/// that it must be called. What the cache holds.
fn gemini_fixed(system: &str) -> Value {
  let mut schema = decision::schema();
  uppercase_types(&mut schema);
  json!({
    "systemInstruction": {"parts": [{"text": system}]},
    "tools": [{"functionDeclarations": [{
      "name": decision::TOOL_NAME,
      "description": decision::TOOL_DESCRIPTION,
      "parameters": schema
    }]}],
    "toolConfig": {"functionCallingConfig": {"mode": "ANY", "allowedFunctionNames": [decision::TOOL_NAME]}}
  })
}

/// How much the model may think. Thinking is billed as output, and a round's
/// decision does not need much: Gemini 3 takes a level, 2.5 a token budget.
fn gemini_generation(model: &str) -> Value {
  let thinking = if model.starts_with("gemini-2") {
    json!({"thinkingBudget": 2048})
  } else {
    json!({"thinkingLevel": "LOW"})
  };
  json!({"temperature": 0.3, "maxOutputTokens": 32768, "thinkingConfig": thinking})
}

fn gemini_request(system: &str, situation: &str, model: &str) -> Value {
  let mut body = gemini_fixed(system);
  body["contents"] = json!([{"role": "user", "parts": [{"text": situation}]}]);
  body["generationConfig"] = gemini_generation(model);
  body
}

fn gemini_cached_request(cache: &str, model: &str, situation: &str) -> Value {
  json!({
    "cachedContent": cache,
    "contents": [{"role": "user", "parts": [{"text": situation}]}],
    "generationConfig": gemini_generation(model)
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
    // Thinking is billed as output, so it is counted as output.
    output_tokens: usage["candidatesTokenCount"].as_u64().unwrap_or(0)
      + usage["thoughtsTokenCount"].as_u64().unwrap_or(0),
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

fn maas_request(model: &str, system: &str, situation: &str) -> Value {
  json!({
    "model": model,
    "max_tokens": 8192,
    "temperature": 0.3,
    "messages": [
      {"role": "system", "content": system},
      {"role": "user", "content": situation}
    ],
    "tools": [{
      "type": "function",
      "function": {
        "name": decision::TOOL_NAME,
        "description": decision::TOOL_DESCRIPTION,
        "parameters": decision::schema()
      }
    }],
    "tool_choice": {"type": "function", "function": {"name": decision::TOOL_NAME}}
  })
}

fn maas_reply(answer: &Value) -> Result<Reply, Error> {
  let usage = &answer["usage"];
  let arguments = answer["choices"][0]["message"]["tool_calls"]
    .as_array()
    .and_then(|calls| calls.iter().find(|call| call["function"]["name"] == decision::TOOL_NAME))
    .and_then(|call| call["function"]["arguments"].as_str())
    .ok_or_else(|| bad("no plan_round call", answer))?;
  let input: Value = serde_json::from_str(arguments).map_err(|e| bad(&e.to_string(), answer))?;
  Ok(Reply {
    decision: serde_json::from_value(input.clone()).map_err(|e| bad(&e.to_string(), &input))?,
    raw: input.to_string(),
    input_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0),
    cached_tokens: usage["prompt_tokens_details"]["cached_tokens"].as_u64().unwrap_or(0),
    // Reasoning, where a model reports it, is part of the completion.
    output_tokens: usage["completion_tokens"].as_u64().unwrap_or(0),
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
    Family::Maas => maas_reply(answer),
  }
}

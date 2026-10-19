//! The commander that asks a language model.
//!
//! Each round it renders the side's [`Observation`] with [`prompt::situation`],
//! sends it after the fixed system prompt, and reads back one `plan_round`
//! call. The plan in that call is kept and shown to the model next round, so
//! its intentions survive between calls without a transcript. Every ship's
//! rationale goes to the log at `info`, which is where an admiral reads it
//! until there is a panel.
//!
//! When the model does not answer -- down, slow, or talking nonsense -- the
//! round gets no orders: the ships keep their plans, as a human crew that
//! stepped away would, and the log says so. No scripted brain steps in.

use std::io::Write;
use std::sync::Arc;

use async_trait::async_trait;
use callisto::tactics::Intent;
use tracing::{info, warn};

use crate::commander::Commander;
use crate::decision::{self, Conversion};
use crate::observation::Observation;
use crate::prompt;
use crate::provider::{ModelSpec, Vertex};

/// Where a commander writes everything it said and heard, for reading
/// afterwards. Markdown: the system prompt once, then each round's situation,
/// reply and notes.
pub type Transcript = Box<dyn Write + Send>;

pub struct Llm {
  vertex: Arc<Vertex>,
  spec: ModelSpec,
  plan: Option<String>,
  transcript: Option<Transcript>,
  rounds_asked: u32,
  memory: prompt::Memory,
  answered: bool,
  /// Every call, as JSON lines, for the game's record.
  calls: Option<Transcript>,
  /// Which game and side, for the record and the log.
  game: String,
  /// What the calls so far have cost, in US dollars, where the price is known.
  spent: f64,
  /// The fixed part of the prompt: the built-in one unless the commander was
  /// named with a prompt file of its own.
  system: String,
}

impl Llm {
  #[must_use]
  pub fn new(vertex: Arc<Vertex>, spec: ModelSpec) -> Self {
    Llm {
      vertex,
      spec,
      plan: None,
      transcript: None,
      rounds_asked: 0,
      memory: prompt::Memory::default(),
      answered: true,
      calls: None,
      game: String::new(),
      spent: 0.0,
      system: prompt::system().to_string(),
    }
  }

  /// Give the model this system prompt instead of the built-in one.
  #[must_use]
  pub fn with_system(mut self, system: String) -> Self {
    self.system = system;
    self
  }

  /// Record every call as a JSON line here, for the game's record: what the
  /// side was shown, what the model answered, the tokens and the cost.
  #[must_use]
  pub fn with_call_log(mut self, game: &str, calls: Transcript) -> Self {
    self.game = game.to_string();
    self.calls = Some(calls);
    self
  }

  fn record(&mut self, entry: &serde_json::Value) {
    if let Some(calls) = self.calls.as_mut() {
      let _ = writeln!(calls, "{entry}");
      let _ = calls.flush();
    }
  }

  /// Also write every prompt and reply here.
  #[must_use]
  pub fn with_transcript(mut self, transcript: Transcript) -> Self {
    self.transcript = Some(transcript);
    self
  }

  fn note(&mut self, text: &str) {
    if let Some(transcript) = self.transcript.as_mut() {
      let _ = writeln!(transcript, "{text}");
      let _ = transcript.flush();
    }
  }
}

#[async_trait]
impl Commander for Llm {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let system = self.system.clone();
    let system = system.as_str();
    self.memory.update(view);
    let situation = prompt::situation(view, self.plan.as_deref(), &self.memory);
    if self.rounds_asked == 0 {
      self.note(&format!("# Model {}\n\n## System prompt\n\n{system}\n", self.spec));
    }
    self.rounds_asked += 1;
    self.note(&format!("## Round {}\n\n### Situation\n\n{situation}", view.round));

    let side = view.team.map_or_else(|| "unaligned".to_string(), |team| format!("{team:?}"));
    match self.vertex.plan_round(&self.spec, system, &situation).await {
      Ok(reply) => {
        self.note(&format!(
          "### Reply ({} in, {} cached, {} out)\n\n```json\n{}\n```\n",
          reply.input_tokens, reply.cached_tokens, reply.output_tokens, reply.raw
        ));
        let Conversion { intents, notes } = decision::to_intents(&reply.decision, view);
        for note in &notes {
          warn!("(llm {side}) {note}");
        }
        if !notes.is_empty() {
          self.note(&format!("### Dropped\n\n- {}\n", notes.join("\n- ")));
        }
        let cost = reply.cost(&self.spec);
        self.spent += cost.unwrap_or(0.0);
        self.record(&serde_json::json!({
          "game": self.game,
          "side": side,
          "round": view.round,
          "model": self.spec.name(),
          "answered": true,
          "input_tokens": reply.input_tokens,
          "cached_tokens": reply.cached_tokens,
          "output_tokens": reply.output_tokens,
          "cost_usd": cost,
          "situation": situation,
          "reply": serde_json::from_str::<serde_json::Value>(&reply.raw).unwrap_or(serde_json::Value::String(reply.raw.clone())),
          "dropped": notes,
        }));
        info!(
          "(llm {side}) Round {}: {} in ({} cached), {} out, ${:.4}; ${:.3} so far this game",
          view.round,
          reply.input_tokens,
          reply.cached_tokens,
          reply.output_tokens,
          cost.unwrap_or(0.0),
          self.spent
        );
        info!("(llm {side}) Round {}: plan: {}", view.round, reply.decision.plan);
        if let Some(line) = crate::decision::squadrons_line(&reply.decision.squadrons) {
          info!("(llm {side}) Round {}: {line}", view.round);
        }
        for (ship, intent) in &intents {
          info!("(llm {side}) {ship}: {}", intent.rationale);
        }
        // The squadrons go with the plan: the model sees them next round,
        // and so does anyone reading Fleet Comms.
        self.plan = Some(match crate::decision::squadrons_line(&reply.decision.squadrons) {
          Some(line) => format!("{}\n{line}", reply.decision.plan),
          None => reply.decision.plan,
        });
        self.answered = true;
        intents
      }
      Err(e) => {
        warn!("(llm {side}) Round {}: no decision; ships keep their plans. {e}", view.round);
        self.record(&serde_json::json!({
          "game": self.game,
          "side": side,
          "round": view.round,
          "model": self.spec.name(),
          "answered": false,
          "situation": situation,
          "error": e.to_string(),
        }));
        self.note(&format!("### No reply\n\n{e}\n"));
        self.answered = false;
        vec![]
      }
    }
  }

  fn plan(&self) -> Option<&str> {
    self.plan.as_deref()
  }

  fn answered(&self) -> bool {
    self.answered
  }

  fn spent(&self) -> Option<f64> {
    crate::provider::price(&self.spec).map(|_| self.spent)
  }
}

/// The system prompt for a commander named `name`: the built-in one, or for
/// `model@path` the file at `path` -- so a prompt can be tried on one model
/// while another keeps the built-in one, and edited without a rebuild.
///
/// # Errors
/// Returns why, if the file cannot be read.
pub fn system_for(name: &str) -> Result<String, String> {
  match name.split_once('@') {
    None => Ok(prompt::system().to_string()),
    Some((_, path)) => std::fs::read_to_string(path).map_err(|e| format!("cannot read the prompt {path}: {e}")),
  }
}

/// A commander by name: a model's (`gemini`, `claude:<model>`), given a
/// Vertex connection, or any of [`crate::network::commander::named`]'s: the
/// built-in network, a network from a file, or a baseline.
#[must_use]
pub fn named(name: &str, seed: u64, vertex: Option<&Arc<Vertex>>) -> Option<Box<dyn Commander>> {
  match (ModelSpec::parse(name), vertex) {
    (Some(spec), Some(vertex)) => {
      Some(Box::new(Llm::new(Arc::clone(vertex), spec).with_system(system_for(name).ok()?)))
    }
    (Some(_), None) => None,
    (None, _) => crate::network::commander::named(name, seed),
  }
}

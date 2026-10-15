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

use crate::commander::{self, Commander};
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
    let system = prompt::system();
    let situation = prompt::situation(view, self.plan.as_deref());
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
        info!("(llm {side}) Round {}: plan: {}", view.round, reply.decision.plan);
        for (ship, intent) in &intents {
          info!("(llm {side}) {ship}: {}", intent.rationale);
        }
        self.plan = Some(reply.decision.plan);
        intents
      }
      Err(e) => {
        warn!("(llm {side}) Round {}: no decision; ships keep their plans. {e}", view.round);
        self.note(&format!("### No reply\n\n{e}\n"));
        vec![]
      }
    }
  }

  fn plan(&self) -> Option<&str> {
    self.plan.as_deref()
  }
}

/// A commander by name: a model's (`gemini`, `claude:<model>`), given a
/// Vertex connection, or any of [`commander::named`]'s.
#[must_use]
pub fn named(name: &str, seed: u64, vertex: Option<&Arc<Vertex>>) -> Option<Box<dyn Commander>> {
  match (ModelSpec::parse(name), vertex) {
    (Some(spec), Some(vertex)) => Some(Box::new(Llm::new(Arc::clone(vertex), spec))),
    (Some(_), None) => None,
    (None, _) => commander::named(name, seed),
  }
}

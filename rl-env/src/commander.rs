//! The trained network as a [`Commander`], so it can play in the harness and
//! the AI service like any other: against the baselines and the models.
//!
//! Each round it encodes every one of its ships' views exactly as training
//! did, picks each ship's choice, and turns the choices into orders as the
//! environment does, squadrons included. The rationale is made from the
//! orders themselves: no model is involved.

use std::sync::Arc;

use ai_ships::commander::Commander;
use ai_ships::observation::Observation;
use async_trait::async_trait;
use callisto::tactics::{EngineerVerb, Helm, Intent, SensorVerb};
use rand::SeedableRng;

use crate::action::{side_intents, Choice, MASK_LEN};
use crate::encode::{encode_ship, Context, Memory, Picture, OBS_LEN};
use crate::game::{believed_hull, remember, side_goal};
use crate::net::Policy;

/// The rounds a game is assumed to last, for the round-progress input, as in
/// training.
pub const MAX_ROUNDS: u32 = 40;

pub struct Net {
  policy: Arc<Policy>,
  memory: Memory,
  hull_at_start: Option<u32>,
  /// Choices drawn as in training, rather than always the best scored.
  rng: Option<rand::rngs::SmallRng>,
}

impl Net {
  /// Plays its best-scored choice every time.
  #[must_use]
  pub fn new(policy: Arc<Policy>) -> Self {
    Net {
      policy,
      memory: Memory::new(),
      hull_at_start: None,
      rng: None,
    }
  }

  /// Draws its choices in proportion to the network's scores, as in training.
  #[must_use]
  pub fn sampling(mut self, seed: u64) -> Self {
    self.rng = Some(rand::rngs::SmallRng::seed_from_u64(seed));
    self
  }
}

/// A sentence on what a ship was ordered to do, from the order itself.
fn rationale(intent: &Intent) -> String {
  let helm = match &intent.helm {
    Helm::Hold => "holds her course".to_string(),
    Helm::Intercept { target, band } => format!("closes on {target} to {band:?}"),
    Helm::HoldRange { target, band } => format!("holds {band:?} range of {target}"),
    Helm::OpenRange { target } => format!("opens the range from {target}"),
    Helm::Station { .. } => "keeps station".to_string(),
    Helm::Follow { guide, .. } => format!("follows {guide}"),
  };
  let fire = intent
    .fire
    .first()
    .map_or_else(String::new, |engage| format!(", firing on {}", engage.target));
  let dodge = intent
    .evade
    .filter(|g| *g > 0)
    .map_or_else(String::new, |g| format!(", {g} G kept to dodge"));
  let jamming: String = intent
    .sensors
    .iter()
    .filter_map(|verb| match verb {
      SensorVerb::JamComms { target } => Some(format!(", jamming {target}'s comms")),
      SensorVerb::JamMissiles => Some(", jamming missiles".to_string()),
      _ => None,
    })
    .collect();
  let withdrawing = intent
    .engineering
    .iter()
    .any(|verb| matches!(verb, EngineerVerb::Jump | EngineerVerb::Power { online: false, .. }))
    || intent
      .software
      .iter()
      .any(|order| order.running && order.program == callisto::software::SoftwareKind::JumpControl);
  let lead = if withdrawing { "Withdrawing to jump: " } else { "" };
  format!(
    "{lead}{}{fire}{dodge}{jamming}.",
    if withdrawing { helm } else { capitalise(&helm) }
  )
}

fn capitalise(text: &str) -> String {
  let mut chars = text.chars();
  chars
    .next()
    .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

#[async_trait]
impl Commander for Net {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    remember(&mut self.memory, view, view.round);
    let hull: u32 = view.own.iter().map(|ship| ship.current_hull).sum();
    let start = *self.hull_at_start.get_or_insert(hull);
    let ctx = Context {
      view,
      memory: &self.memory,
      round: view.round,
      max_rounds: MAX_ROUNDS,
      own_hull_share: hull as f32 / start.max(1) as f32,
      enemy_hull_share: believed_hull(&self.memory),
      goal: side_goal(&view.own),
    };
    let mut obs = vec![0.0; OBS_LEN];
    let mut mask = vec![false; MASK_LEN];
    let decided: Vec<(Choice, Picture)> = view
      .own
      .iter()
      .map(|ship| {
        let picture = encode_ship(&ctx, ship, &mut obs, &mut mask);
        let choice = self
          .policy
          .choose(&obs, &mask, self.rng.as_mut().map(|rng| rng as &mut dyn rand::RngCore));
        (choice, picture)
      })
      .collect();
    let own: Vec<_> = view
      .own
      .iter()
      .zip(&decided)
      .map(|(ship, (choice, picture))| (ship, choice, picture))
      .collect();
    side_intents(&own, view)
      .into_iter()
      .map(|(ship, mut intent)| {
        intent.rationale = rationale(&intent);
        (ship, intent)
      })
      .collect()
  }
}

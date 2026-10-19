//! The trained network as a [`Commander`], so it can play in the harness and
//! the AI service like any other: against the baselines and the models.
//!
//! Each round it encodes every one of its ships' views exactly as training
//! did, picks each ship's choice, and turns the choices into orders as the
//! environment does, squadrons included. The rationale is made from the
//! orders themselves: no model is involved.

use std::sync::Arc;

use crate::commander::Commander;
use crate::observation::Observation;
use async_trait::async_trait;
use callisto::tactics::{EngineerVerb, Helm, Intent, SensorVerb};
use rand::SeedableRng;

use crate::network::action::{side_intents, Choice, Head, Verb, HELM_BANDS, MASK_LEN};
use crate::network::encode::{believed_hull, remember, side_goal};
use crate::network::encode::{encode_ship, Context, Memory, Picture, OBS_LEN};
use crate::network::history::History;
use crate::network::net::Policy;
use crate::network::orders;
use crate::network::side_net::SideNet;
use callisto::entity::Entity;

/// The rounds a game is assumed to last, for the round-progress input, as in
/// training.
pub const MAX_ROUNDS: u32 = 40;

pub struct Net {
  policy: Arc<Policy>,
  memory: Memory,
  hull_at_start: Option<u32>,
  /// Choices drawn as in training, rather than always the best scored.
  rng: Option<rand::rngs::SmallRng>,
  /// The goal to play for, in place of the scenario's.
  goal: Option<callisto::ship::Goal>,
  /// The last decision, explained: per ship, each head's choice, how likely
  /// the network thought it, and the runner-up.
  last: serde_json::Value,
  /// What the side remembers from the last round, as in training.
  history: History,
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
      goal: None,
      last: serde_json::Value::Null,
      history: History::default(),
    }
  }

  /// Plays for this goal, whatever the scenario gives its side.
  #[must_use]
  pub fn with_goal(mut self, goal: callisto::ship::Goal) -> Self {
    self.goal = Some(goal);
    self
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

/// What a head's choice means, in words.
fn label(head: Head, choice: usize, picture: &Picture) -> String {
  let enemy = |slot: usize| picture.enemies.get(slot).cloned().unwrap_or_else(|| format!("slot {slot}"));
  let enemy_or_none = |c: usize| if c == 0 { "none".to_string() } else { enemy(c - 1) };
  match head {
    Head::Verb => format!("{:?}", Verb::ALL[choice]),
    Head::HelmTarget => enemy(choice),
    Head::Band => format!("{:?}", HELM_BANDS[choice]),
    Head::Guide => picture.friends.get(choice).cloned().unwrap_or_else(|| format!("slot {choice}")),
    Head::Fire | Head::Lock | Head::Target2 => enemy_or_none(choice),
    Head::Posture => ["silent", "sharing", "active"][choice].to_string(),
    Head::Dodge => format!("{choice} G"),
    Head::Assist | Head::Jump => ["no", "yes"][choice].to_string(),
    Head::Sensor1 | Head::Sensor2 => format!("{:?}", orders::SENSOR_ORDERS[choice]),
    Head::Captain => format!("{:?}", orders::RANKINGS[choice]),
    Head::Computer => ["fight", "link"][choice].to_string(),
  }
}

/// One ship's decision: per head, what was chosen, its probability, and the
/// runner-up.
fn explain(choice: &Choice, probabilities: &[Vec<(usize, f32)>], picture: &Picture) -> serde_json::Value {
  let heads: serde_json::Map<String, serde_json::Value> = Head::ALL
    .iter()
    .zip(probabilities)
    .enumerate()
    .map(|(index, (head, options))| {
      let chosen = usize::try_from(choice[index]).unwrap_or(0);
      let p = options.iter().find(|(c, _)| *c == chosen).map_or(0.0, |(_, p)| *p);
      let runner_up = options.iter().find(|(c, _)| *c != chosen);
      (
        format!("{head:?}").to_lowercase(),
        serde_json::json!({
          "choice": label(*head, chosen, picture),
          "p": (f64::from(p) * 1000.0).round() / 1000.0,
          "runner_up": runner_up.map(|(c, q)| serde_json::json!([label(*head, *c, picture), (f64::from(*q) * 1000.0).round() / 1000.0])),
          "open": options.len(),
        }),
      )
    })
    .collect();
  serde_json::Value::Object(heads)
}

#[async_trait]
impl Commander for Net {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    remember(&mut self.memory, view, view.round);
    self.history.look(view);
    let hull: u32 = view.own.iter().map(|ship| ship.current_hull).sum();
    let start = *self.hull_at_start.get_or_insert(hull);
    let ctx = Context {
      view,
      memory: &self.memory,
      round: view.round,
      max_rounds: MAX_ROUNDS,
      own_hull_share: hull as f32 / start.max(1) as f32,
      enemy_hull_share: believed_hull(&self.memory),
      goal: self.goal.unwrap_or_else(|| side_goal(&view.own)),
      history: &self.history,
    };
    let mut obs = vec![0.0; OBS_LEN];
    let mut mask = vec![false; MASK_LEN];
    let mut explained = serde_json::Map::new();
    let decided: Vec<(Choice, Picture)> = view
      .own
      .iter()
      .map(|ship| {
        let picture = encode_ship(&ctx, ship, &mut obs, &mut mask);
        let choice = self
          .policy
          .choose(&obs, &mask, self.rng.as_mut().map(|rng| rng as &mut dyn rand::RngCore));
        explained.insert(
          ship.get_name().to_string(),
          explain(&choice, &self.policy.probabilities(&obs, &mask), &picture),
        );
        (choice, picture)
      })
      .collect();
    self.last = serde_json::Value::Object(explained);
    let own: Vec<_> = view
      .own
      .iter()
      .zip(&decided)
      .map(|(ship, (choice, picture))| (ship, choice, picture))
      .collect();
    let intents: Vec<(String, Intent)> = side_intents(&own, view)
      .into_iter()
      .map(|(ship, mut intent)| {
        intent.rationale = rationale(&intent);
        (ship, intent)
      })
      .collect();
    self.history.record(&intents);
    intents
  }

  fn explain(&self) -> Option<serde_json::Value> {
    Some(self.last.clone())
  }
}

/// The per-side network as a [`Commander`]: the whole side encoded as
/// tokens, as in training, one pass for every ship, the choices turned into
/// orders as for the per-ship network.
pub struct SideCommander {
  net: Arc<SideNet>,
  memory: Memory,
  history: History,
  hull_at_start: Option<u32>,
  rng: Option<rand::rngs::SmallRng>,
  goal: Option<callisto::ship::Goal>,
  last: serde_json::Value,
}

impl SideCommander {
  #[must_use]
  pub fn new(net: Arc<SideNet>) -> Self {
    SideCommander {
      net,
      memory: Memory::new(),
      history: History::default(),
      hull_at_start: None,
      rng: None,
      goal: None,
      last: serde_json::Value::Null,
    }
  }
}

#[async_trait]
impl Commander for SideCommander {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    use crate::network::side::{
      Buffers, SideContext, FOES, FOE_FEATS, MASK_LEN as SIDE_MASK, OWN, OWN_FEATS, PAIR_FEATS, SIDE_FEATS,
    };
    remember(&mut self.memory, view, view.round);
    self.history.look(view);
    let hull: u32 = view.own.iter().map(|ship| ship.current_hull).sum();
    let start = *self.hull_at_start.get_or_insert(hull);
    let ctx = SideContext {
      view,
      memory: &self.memory,
      history: &self.history,
      round: view.round,
      max_rounds: MAX_ROUNDS,
      own_hull_share: hull as f32 / start.max(1) as f32,
      enemy_hull_share: believed_hull(&self.memory),
      goal: self.goal.unwrap_or_else(|| side_goal(&view.own)),
    };
    let (mut own, mut foes, mut side, mut pairs, mut masks) = (
      vec![0.0; OWN * OWN_FEATS],
      vec![0.0; FOES * FOE_FEATS],
      vec![0.0; SIDE_FEATS],
      vec![0.0; OWN * FOES * PAIR_FEATS],
      vec![false; OWN * SIDE_MASK],
    );
    let tokens = crate::network::side::encode_side(
      &ctx,
      &mut Buffers {
        own: &mut own,
        foes: &mut foes,
        side: &mut side,
        pairs: &mut pairs,
        masks: &mut masks,
      },
    );
    let logits = self.net.logits(&own, &foes, &side, &pairs);
    let picture = tokens.picture();
    let mut explained = serde_json::Map::new();
    let choices: Vec<(String, Choice)> = tokens
      .own
      .iter()
      .enumerate()
      .map(|(i, name)| {
        let row = &logits[i * SIDE_MASK..(i + 1) * SIDE_MASK];
        let mask = &masks[i * SIDE_MASK..(i + 1) * SIDE_MASK];
        let choice = SideNet::choose(row, mask, self.rng.as_mut().map(|rng| rng as &mut dyn rand::RngCore));
        explained.insert(name.clone(), explain(&choice, &SideNet::probabilities(row, mask), &picture));
        (name.clone(), choice)
      })
      .collect();
    self.last = serde_json::Value::Object(explained);
    let own_ships: Vec<_> = choices
      .iter()
      .filter_map(|(name, choice)| {
        view
          .own
          .iter()
          .find(|ship| ship.get_name() == name)
          .map(|ship| (ship, choice, &picture))
      })
      .collect();
    let intents: Vec<(String, Intent)> = side_intents(&own_ships, view)
      .into_iter()
      .map(|(ship, mut intent)| {
        intent.rationale = rationale(&intent);
        (ship, intent)
      })
      .collect();
    self.history.record(&intents);
    intents
  }

  fn explain(&self) -> Option<serde_json::Value> {
    Some(self.last.clone())
  }
}

/// Networks read once and shared, by file.
static LOADED: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Arc<Policy>>>> =
  std::sync::OnceLock::new();
static LOADED_SIDE: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Arc<SideNet>>>> =
  std::sync::OnceLock::new();

fn cached<T>(
  table: &std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<String, Arc<T>>>>, path: &str,
  load: impl Fn(&str) -> Result<T, String>,
) -> Option<Arc<T>> {
  let mut loaded = table.get_or_init(Default::default).lock().ok()?;
  if let Some(found) = loaded.get(path) {
    return Some(Arc::clone(found));
  }
  let made = Arc::new(load(path).map_err(|e| eprintln!("{e}")).ok()?);
  loaded.insert(path.to_string(), Arc::clone(&made));
  Some(made)
}

/// The per-side network the service plays with, built in: side-20-6
/// (`rl-env/checkpoints/side-20-6`, exported by `scripts/export_side.py`).
/// Replace the file to change it; its layout must match this build's.
pub const BUILT_IN: &[u8] = include_bytes!("../../networks/side.cside");

/// A commander by name: `network`, the built-in per-side network; a baseline
/// (`close`, `hold`, ...); or a trained network from its exported file --
/// `net:<file.cpol>` for the per-ship network, `side:<file.cside>` for the
/// per-side one, each playing its best choice, or with `~` (`net~:`,
/// `side~:`) drawing as in training. A goal after the file
/// (`side:<file>#destroy`) plays for that goal rather than the scenario's.
/// Each file is read once.
#[must_use]
pub fn named(name: &str, seed: u64) -> Option<Box<dyn Commander>> {
  if name == "network" {
    let net = cached(&LOADED_SIDE, name, |_| SideNet::from_bytes(BUILT_IN, "the built-in network"))?;
    return Some(Box::new(SideCommander::new(net)));
  }
  let Some((kind, rest)) = name.split_once(':') else {
    return crate::commander::named(name, seed);
  };
  let (sampling, kind) = kind.strip_suffix('~').map_or((false, kind), |kind| (true, kind));
  if kind != "net" && kind != "side" {
    return crate::commander::named(name, seed);
  }
  let (path, goal) = match rest.split_once('#') {
    Some((path, goal)) => {
      let goal: callisto::ship::Goal = serde_json::from_value(serde_json::Value::String(goal.to_string()))
        .map_err(|_| eprintln!("no goal {goal}: destroy, hold, raid or escape"))
        .ok()?;
      (path, Some(goal))
    }
    None => (rest, None),
  };
  if kind == "side" {
    let net = cached(&LOADED_SIDE, path, SideNet::load)?;
    let mut commander = SideCommander::new(net);
    commander.goal = goal;
    if sampling {
      commander.rng = Some(rand::rngs::SmallRng::seed_from_u64(seed));
    }
    return Some(Box::new(commander));
  }
  let policy = cached(&LOADED, path, Policy::load)?;
  let mut net = Net::new(policy);
  if let Some(goal) = goal {
    net = net.with_goal(goal);
  }
  Some(Box::new(if sampling { net.sampling(seed) } else { net }))
}

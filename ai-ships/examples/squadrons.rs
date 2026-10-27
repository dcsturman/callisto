//! An experiment: the harness, with a side split into squadrons, each played
//! by its own pass of the built-in per-side network and shown only some of
//! the enemy.
//!
//! ```text
//! cargo run --release --example squadrons -- --scenario "../callisto/scenarios/Fighter Screen.json" \
//!   --side "Blue=split:Lance>Spear+8;CS>*" --side Red=network --games 100
//! ```
//!
//! `split:` takes squadrons separated by `;`. Each is `<own>><shown>`: our
//! ships whose names start with `<own>`, shown the enemies whose names start
//! with `<shown>` (`*` for every enemy), and, with `+N`, up to N others,
//! nearest first, that can hit one of the squadron where they are now.
//! `<own>>><shown>` shows every enemy instead, and lets the squadron fire
//! only on those. `<own>&` also shows the rest of the side, as own ships
//! whose orders are dropped.

use async_trait::async_trait;
use callisto::entity::Entity;
use callisto::tactics::{self, Intent};
use cgmath::InnerSpace;

use ai_ships::commander::Commander;
use ai_ships::observation::Observation;

struct Squadron {
  own: String,
  shown: String,
  threats: usize,
  /// Show every enemy, and only fire on the assigned ones and the threats.
  allow_only: bool,
  /// Show the rest of the side too, as own ships whose orders are dropped.
  with_allies: bool,
  commander: Box<dyn Commander>,
}

struct Split {
  squadrons: Vec<Squadron>,
}

impl Squadron {
  /// The view this squadron's pass is given.
  fn view(&self, view: &Observation) -> Observation {
    let mut part = view.clone();
    if !self.with_allies {
      part.own.retain(|ship| ship.get_name().starts_with(&self.own));
    }
    if self.shown == "*" {
      return part;
    }
    let allowed = self.allowed(view, &part);
    if self.allow_only {
      return part;
    }
    let team = view.team;
    part
      .contacts
      .retain(|contact| (team.is_some() && contact.team == team) || allowed.contains(&contact.name));
    let assigned = |name: &str| name.starts_with(&self.shown);
    part.blips.retain(|blip| assigned(&blip.name));
    part
  }

  /// The enemies this squadron may engage: the assigned ones, and up to
  /// `threats` others that can hit one of the squadron where they are now.
  fn allowed(&self, view: &Observation, part: &Observation) -> Vec<String> {
    let assigned = |name: &str| name.starts_with(&self.shown);
    // Enemies that can hit one of the squadron at the range they are at now.
    let threat = |contact: &ai_ships::observation::Contact| {
      part.own.iter().any(|ship| {
        let band = tactics::band_at((contact.position - ship.get_position()).magnitude());
        view
          .engagement(ship.get_name(), &contact.name)
          .and_then(|table| table.they_hit.iter().find(|odds| odds.band == band))
          .is_some_and(|odds| odds.expected_damage > 0.0)
      })
    };
    let mut others: Vec<String> = view
      .enemies()
      .into_iter()
      .filter(|contact| !assigned(&contact.name) && threat(contact))
      .map(|contact| contact.name.clone())
      .collect();
    others.truncate(self.threats);
    others.extend(view.enemies().into_iter().filter(|c| assigned(&c.name)).map(|c| c.name.clone()));
    others
  }
}

#[async_trait]
impl Commander for Split {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let mut intents = Vec::new();
    for squadron in &mut self.squadrons {
      let part = squadron.view(view);
      if !part.own.iter().any(|ship| ship.get_name().starts_with(&squadron.own)) {
        continue;
      }
      let mut decided = squadron.commander.decide(&part).await;
      decided.retain(|(name, _)| name.starts_with(&squadron.own));
      if squadron.allow_only && squadron.shown != "*" {
        let allowed = squadron.allowed(view, &part);
        for (name, intent) in &mut decided {
          intent.fire.retain(|engage| allowed.contains(&engage.target));
          if intent.fire.is_empty() {
            // The nearest allowed enemy this ship holds, with every gun.
            let me = part.own.iter().find(|ship| ship.get_name() == name.as_str());
            if let Some(nearest) = me.and_then(|me| {
              view.enemies().into_iter().filter(|c| allowed.contains(&c.name)).min_by(|a, b| {
                (a.position - me.get_position())
                  .magnitude()
                  .total_cmp(&(b.position - me.get_position()).magnitude())
              })
            }) {
              intent.fire.push(callisto::tactics::Engage {
                target: nearest.name.clone(),
                ..Default::default()
              });
            }
          }
        }
      }
      intents.extend(decided);
    }
    intents
  }
}

fn split(spec: &str, seed: u64) -> Option<Box<dyn Commander>> {
  let squadrons = spec
    .split(';')
    .map(|part| {
      let (own, shown) = part.split_once('>')?;
      let (with_allies, own) = own.strip_suffix('&').map_or((false, own), |own| (true, own));
      let (allow_only, shown) = shown.strip_prefix('>').map_or((false, shown), |rest| (true, rest));
      let (shown, threats) = shown
        .split_once('+')
        .map_or((shown, 0), |(shown, n)| (shown, n.parse().unwrap_or(0)));
      Some(Squadron {
        own: own.to_string(),
        shown: shown.to_string(),
        threats,
        allow_only,
        with_allies,
        commander: ai_ships::network::commander::named("network", seed)?,
      })
    })
    .collect::<Option<Vec<_>>>()?;
  Some(Box::new(Split { squadrons }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  ai_ships::cli::run(&|name, seed| {
    name
      .strip_prefix("split:")
      .and_then(|spec| split(spec, seed))
      .or_else(|| ai_ships::network::commander::named(name, seed))
  })
  .await
}

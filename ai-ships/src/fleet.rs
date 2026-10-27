//! A fleet under orders: squadrons, each following a script of steps, each
//! step carried out by its own pass of the per-side network.
//!
//! The network does the fighting; a [`Directive`] says what each squadron
//! is for. A step can give a squadron
//!
//! - a goal, and a post to hold;
//! - who it may engage: a rule naming designs, ships or tonnage. The pass
//!   is shown every enemy it has room for -- the assigned ones first, then
//!   those that can hit the squadron, then the nearest -- and may target only
//!   the assigned ones and those threats;
//! - how it moves: as the network chooses, a burn toward or away from a
//!   place, or an arrival at a place (or a range band of it) by a round;
//! - when it ends: a round, a range band reached, being fired on, losses.
//!
//! Each squadron's pass also sees the rest of the side, nearest first, as
//! ships of its own whose orders are dropped, so it fights knowing where its
//! neighbours are. A squadron is at most twenty ships -- one pass -- and
//! ships in none are grouped by name, twenty at a time.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use callisto::entity::{Entity, Vec3, DELTA_TIME_F64, G};
use callisto::ship::{Goal, Range, Ship, MAX_SQUADRON};
use callisto::tactics::{self, Helm, Intent};
use cgmath::InnerSpace;
use serde::{Deserialize, Serialize};

use crate::commander::Commander;
use crate::network::commander::{PassOrders, SideCommander};
use crate::network::side_net::SideNet;
use crate::observation::{Contact, Observation};

/// The most of the network's enemy slots a pass may fill.
const FOE_SLOTS: usize = crate::network::side::FOES;
/// Threats a step shows and allows besides its assigned enemies, unless it
/// says otherwise.
const DEFAULT_THREATS: usize = 8;

/// A side's orders: a script for each squadron.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Directive {
  /// What the commander says about the orders, to the players.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub say: Option<String>,
  #[serde(default)]
  pub squadrons: Vec<Script>,
}

/// One squadron's orders: its steps, in turn. `squadron` `*` is every
/// squadron without a script of its own.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Script {
  pub squadron: String,
  #[serde(default)]
  pub steps: Vec<Step>,
}

/// What a squadron does until `until`; with no `until`, from then on.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Step {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub goal: Option<Goal>,
  /// Where a Hold step holds, and how close counts (km).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub post: Option<Place>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub post_radius_km: Option<f64>,
  /// Who the squadron may engage; `None` for anyone.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub engage: Option<Rule>,
  /// Enemies besides those assigned that it may also engage: the ones that
  /// can hit it, most dangerous first.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub threats: Option<usize>,
  #[serde(default)]
  pub movement: Movement,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub until: Option<Until>,
}

/// Which enemies: any named design or ship, within the tonnage limits.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Rule {
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub designs: Vec<String>,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub ships: Vec<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub max_tons: Option<u32>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub min_tons: Option<u32>,
}

impl Rule {
  /// Whether `contact` is one of these. Designs and ships widen the rule
  /// (either will do; neither given means any); tonnage narrows it.
  #[must_use]
  pub fn matches(&self, contact: &Contact) -> bool {
    self.matches_ship(&contact.name, &contact.design, contact.displacement)
  }

  fn matches_ship(&self, name: &str, design: &str, tons: u32) -> bool {
    let named = (self.designs.is_empty() && self.ships.is_empty())
      || self.designs.iter().any(|wanted| wanted.eq_ignore_ascii_case(design))
      || self.ships.iter().any(|ship| ship == name);
    named && self.max_tons.is_none_or(|max| tons <= max) && self.min_tons.is_none_or(|min| tons >= min)
  }
}

/// How a squadron moves.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Movement {
  /// As the network chooses.
  #[default]
  Network,
  /// Toward a place (or away from it), at up to `thrust` G.
  Burn {
    toward: Place,
    #[serde(default)]
    away: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    thrust: Option<f64>,
  },
  /// To a place -- or to `band` of it, on this side -- by `round`, all the
  /// squadron arriving together.
  Arrive {
    at: Place,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    band: Option<Range>,
    round: u32,
  },
}

/// A place, named as the players know it.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Place {
  Planet(String),
  Ship(String),
  /// The middle of one of our squadrons.
  Squadron(String),
  /// The middle of the enemies a rule picks out.
  Enemies(Rule),
  /// A point, in metres.
  At([f64; 3]),
}

/// When a step ends.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Until {
  /// This round has begun.
  Round(u32),
  /// Any of the squadron is within `band` of the place.
  Within { of: Place, band: Range },
  /// The squadron took damage last round.
  FiredOn,
  /// The squadron has lost this share of its hull since the step began.
  Losses(f32),
  /// Any of these.
  Any(Vec<Until>),
}

/// Where a squadron is in its script.
#[derive(Debug, Clone, Default)]
struct Progress {
  step: usize,
  /// The squadron's hull when the step began, and last round.
  hull_at_step: u32,
  hull_last_round: u32,
}

/// Our squadrons as they stand: by name, each with her ships, in name order.
#[must_use]
pub fn squadrons(own: &[Ship]) -> Vec<(String, Vec<String>)> {
  let mut named: HashMap<String, Vec<String>> = HashMap::new();
  let mut loose: Vec<String> = Vec::new();
  for ship in own {
    match &ship.squadron {
      Some(name) => named.entry(name.clone()).or_default().push(ship.get_name().to_string()),
      None => loose.push(ship.get_name().to_string()),
    }
  }
  loose.sort();
  let mut groups: Vec<(String, Vec<String>)> = named
    .into_iter()
    .map(|(name, mut ships)| {
      ships.sort();
      (name, ships)
    })
    .collect();
  groups.sort();
  let chunks = loose.chunks(MAX_SQUADRON).count();
  groups.extend(loose.chunks(MAX_SQUADRON).enumerate().map(|(i, ships)| {
    let name = if chunks == 1 {
      "(no squadron)".to_string()
    } else {
      format!("(no squadron {})", i + 1)
    };
    (name, ships.to_vec())
  }));
  groups
}

/// The middle of some positions.
fn middle(points: impl Iterator<Item = Vec3>) -> Option<Vec3> {
  let (sum, count) = points.fold((Vec3::new(0.0, 0.0, 0.0), 0.0), |(sum, count), p| (sum + p, count + 1.0));
  (count > 0.0).then(|| sum / count)
}

fn ships_of<'a>(view: &'a Observation, names: &[String]) -> Vec<&'a Ship> {
  view
    .own
    .iter()
    .filter(|ship| names.iter().any(|name| name == ship.get_name()))
    .collect()
}

/// Where `place` is now, if it can be found.
fn locate(place: &Place, view: &Observation, groups: &[(String, Vec<String>)]) -> Option<Vec3> {
  match place {
    Place::Planet(name) => view
      .planets
      .iter()
      .find(|planet| planet.get_name() == name)
      .map(Entity::get_position),
    Place::Ship(name) => view
      .own
      .iter()
      .find(|ship| ship.get_name() == name)
      .map(Entity::get_position)
      .or_else(|| view.contacts.iter().find(|contact| contact.name == *name).map(|c| c.position)),
    Place::Squadron(name) => groups
      .iter()
      .find(|(group, _)| group == name)
      .and_then(|(_, ships)| middle(ships_of(view, ships).into_iter().map(Entity::get_position))),
    Place::Enemies(rule) => middle(view.enemies().into_iter().filter(|c| rule.matches(c)).map(|c| c.position)),
    Place::At([x, y, z]) => Some(Vec3::new(*x, *y, *z)),
  }
}

/// Whether `until` has come, for a squadron of `ships` in `progress`.
fn reached(
  until: &Until, view: &Observation, ships: &[&Ship], groups: &[(String, Vec<String>)], progress: &Progress,
) -> bool {
  let hull: u32 = ships.iter().map(|ship| ship.current_hull).sum();
  match until {
    Until::Round(round) => view.round >= *round,
    Until::Within { of, band } => locate(of, view, groups).is_some_and(|at| {
      ships
        .iter()
        .any(|ship| tactics::band_at((at - ship.get_position()).magnitude()) as usize <= *band as usize)
    }),
    Until::FiredOn => hull < progress.hull_last_round,
    #[allow(clippy::cast_precision_loss)]
    Until::Losses(share) => {
      progress.hull_at_step > 0
        && (progress.hull_at_step - hull.min(progress.hull_at_step)) as f32 >= share * progress.hull_at_step as f32
    }
    Until::Any(any) => any.iter().any(|until| reached(until, view, ships, groups, progress)),
  }
}

/// The enemies a squadron's pass is shown, in order of claim on the slots,
/// and those it may engage (`None` for any).
fn foes_for(view: &Observation, ships: &[&Ship], step: &Step) -> (Vec<String>, Option<Vec<String>>) {
  let centre = middle(ships.iter().map(|ship| ship.get_position())).unwrap_or(Vec3::new(0.0, 0.0, 0.0));
  let mut enemies: Vec<&Contact> = view.enemies();
  enemies.sort_by(|a, b| (a.position - centre).magnitude().total_cmp(&(b.position - centre).magnitude()));
  let Some(rule) = &step.engage else {
    return (enemies.iter().map(|c| c.name.clone()).collect(), None);
  };
  let assigned: Vec<&Contact> = enemies.iter().copied().filter(|c| rule.matches(c)).collect();
  // What each other enemy can do to the squadron from where it is now.
  let danger = |contact: &Contact| -> f64 {
    ships
      .iter()
      .filter_map(|ship| {
        let band = tactics::band_at((contact.position - ship.get_position()).magnitude());
        view
          .engagement(ship.get_name(), &contact.name)
          .and_then(|table| table.they_hit.iter().find(|odds| odds.band == band))
          .map(|odds| odds.expected_damage)
      })
      .sum()
  };
  let mut threats: Vec<(&Contact, f64)> = enemies
    .iter()
    .copied()
    .filter(|c| !rule.matches(c))
    .map(|c| (c, danger(c)))
    .filter(|(_, danger)| *danger > 0.0)
    .collect();
  threats.sort_by(|a, b| b.1.total_cmp(&a.1));
  threats.truncate(step.threats.unwrap_or(DEFAULT_THREATS));
  let allowed: Vec<String> = assigned
    .iter()
    .map(|c| c.name.clone())
    .chain(threats.iter().map(|(c, _)| c.name.clone()))
    .collect();
  let shown: Vec<String> = allowed
    .iter()
    .cloned()
    .chain(enemies.iter().map(|c| c.name.clone()).filter(|name| !allowed.contains(name)))
    .collect();
  (shown, Some(allowed))
}

/// The view one squadron's pass sees: the squadron, then the rest of the
/// side nearest first, twenty in all; and the enemies it is shown.
fn pass_view(view: &Observation, members: &[String], shown: &[String]) -> Observation {
  let mut part = view.clone();
  let ships = ships_of(view, members);
  let centre = middle(ships.iter().map(|ship| ship.get_position())).unwrap_or(Vec3::new(0.0, 0.0, 0.0));
  let mut allies: Vec<Ship> = view
    .own
    .iter()
    .filter(|ship| !members.iter().any(|name| name == ship.get_name()))
    .cloned()
    .collect();
  allies.sort_by(|a, b| {
    (a.get_position() - centre)
      .magnitude()
      .total_cmp(&(b.get_position() - centre).magnitude())
  });
  part.own = ships
    .into_iter()
    .cloned()
    .chain(allies)
    .take(crate::network::side::OWN)
    .collect();
  let room = FOE_SLOTS;
  let kept: Vec<&String> = shown.iter().take(room).collect();
  let team = view.team;
  part
    .contacts
    .retain(|contact| (team.is_some() && contact.team == team) || kept.contains(&&contact.name));
  // Blips fill what is left, nearest first.
  let left = room.saturating_sub(kept.len());
  part.blips.sort_by(|a, b| a.distance.total_cmp(&b.distance));
  part.blips.truncate(left);
  part
}

/// The helm and thrust cap that carry out `movement` for a squadron.
fn steer(
  movement: &Movement, view: &Observation, ships: &[&Ship], groups: &[(String, Vec<String>)],
) -> Option<(Helm, Option<f64>)> {
  let centre = middle(ships.iter().map(|ship| ship.get_position()))?;
  match movement {
    Movement::Network => None,
    Movement::Burn { toward, away, thrust } => {
      let at = locate(toward, view, groups)?;
      let target = if *away {
        // Far enough the other way that she never arrives.
        centre + (centre - at).normalize() * 1.0e12
      } else {
        at
      };
      Some((
        Helm::Station {
          position: [target.x, target.y, target.z],
        },
        *thrust,
      ))
    }
    Movement::Arrive { at, band, round } => {
      let at = locate(at, view, groups)?;
      let target = match band {
        Some(band) => {
          let offset = centre - at;
          let distance = offset.magnitude();
          if distance < 1.0 {
            at
          } else {
            at + offset / distance * tactics::aim_point(*band).min(distance)
          }
        }
        None => at,
      };
      // Accelerate half the way and brake the rest, in the rounds left:
      // d = a t^2 / 4.
      let time = f64::from(round.saturating_sub(view.round).max(1)) * DELTA_TIME_F64;
      let needed = 4.0 * (target - centre).magnitude() / (time * time) / G;
      Some((
        Helm::Station {
          position: [target.x, target.y, target.z],
        },
        Some(needed.max(0.05)),
      ))
    }
  }
}

/// A side played by squadrons under a [`Directive`].
pub struct FleetCommander {
  net: Arc<SideNet>,
  directive: Directive,
  passes: HashMap<String, SideCommander>,
  progress: HashMap<String, Progress>,
}

impl FleetCommander {
  #[must_use]
  pub fn new(net: Arc<SideNet>, directive: Directive) -> Self {
    FleetCommander {
      net,
      directive,
      passes: HashMap::new(),
      progress: HashMap::new(),
    }
  }

  /// Take new orders. Squadrons whose script is unchanged keep their place
  /// in it; the rest start theirs from the top.
  pub fn set_directive(&mut self, directive: Directive) {
    let unchanged: Vec<String> = directive
      .squadrons
      .iter()
      .filter(|script| self.directive.squadrons.contains(script))
      .map(|script| script.squadron.clone())
      .collect();
    self.progress.retain(|name, _| unchanged.contains(name));
    self.directive = directive;
  }

  #[must_use]
  pub fn directive(&self) -> &Directive {
    &self.directive
  }

  fn script(&self, squadron: &str) -> Option<&Script> {
    let scripts = &self.directive.squadrons;
    scripts
      .iter()
      .find(|script| script.squadron == squadron)
      .or_else(|| scripts.iter().find(|script| script.squadron == "*"))
  }
}

#[async_trait]
impl Commander for FleetCommander {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let groups = squadrons(&view.own);
    let mut intents = Vec::new();
    for (name, members) in &groups {
      let ships = ships_of(view, members);
      let hull: u32 = ships.iter().map(|ship| ship.current_hull).sum();
      let steps = self.script(name).map(|script| script.steps.clone()).unwrap_or_default();
      let progress = self.progress.entry(name.clone()).or_insert_with(|| Progress {
        step: 0,
        hull_at_step: hull,
        hull_last_round: hull,
      });
      // Move on past every step whose end has come.
      while let Some(until) = steps.get(progress.step).and_then(|step| step.until.as_ref()) {
        if !reached(until, view, &ships, &groups, progress) || progress.step + 1 >= steps.len() {
          break;
        }
        progress.step += 1;
        progress.hull_at_step = hull;
      }
      progress.hull_last_round = hull;
      let step = steps.get(progress.step).cloned().unwrap_or_default();

      let (shown, allowed) = foes_for(view, &ships, &step);
      let part = pass_view(view, members, &shown);
      let orders = PassOrders {
        goal: step.goal,
        post: step.post.as_ref().and_then(|place| locate(place, view, &groups)).map(|at| {
          (
            at,
            step.post_radius_km.unwrap_or(callisto::ship::DEFAULT_POST_RADIUS_KM) * 1000.0,
          )
        }),
        allowed,
      };
      let net = Arc::clone(&self.net);
      let pass = self.passes.entry(name.clone()).or_insert_with(|| SideCommander::new(net));
      let mut decided: Vec<(String, Intent)> = pass
        .decide_pass(&part, &orders)
        .into_iter()
        .filter(|(ship, _)| members.contains(ship))
        .collect();
      if let Some((helm, cap)) = steer(&step.movement, view, &ships, &groups) {
        for (_, intent) in &mut decided {
          intent.helm = helm.clone();
          intent.max_thrust = cap.or(intent.max_thrust);
        }
      }
      intents.extend(decided);
    }
    intents
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn a_directive_reads_from_json() {
    let directive: Directive = serde_json::from_str(
      r#"{"say": "Fighters on fighters.",
          "squadrons": [
            {"squadron": "Lance", "steps": [
              {"engage": {"designs": ["Light Fighter"]},
               "movement": {"mode": "arrive", "at": {"enemies": {"designs": ["Light Fighter"]}}, "band": "Medium", "round": 4},
               "until": {"any": [{"round": 4}, "fired_on"]}},
              {"engage": {"designs": ["Light Fighter"]}}
            ]},
            {"squadron": "*", "steps": [{"goal": "hold", "post": {"planet": "Jupiter"}}]}
          ]}"#,
    )
    .unwrap();
    assert_eq!(directive.squadrons.len(), 2);
    assert_eq!(
      directive.squadrons[0].steps[0].until,
      Some(Until::Any(vec![Until::Round(4), Until::FiredOn]))
    );
    assert_eq!(directive.squadrons[1].steps[0].goal, Some(Goal::Hold));
  }

  #[test]
  fn a_rule_widens_by_name_and_narrows_by_tonnage() {
    let fighters = Rule {
      designs: vec!["light fighter".to_string()],
      ..Rule::default()
    };
    assert!(fighters.matches_ship("Spear 1", "Light Fighter", 10));
    assert!(!fighters.matches_ship("CS Tarsis", "Colonial Cruiser - Kinunir", 1200));
    let small = Rule {
      max_tons: Some(100),
      ..Rule::default()
    };
    assert!(small.matches_ship("Spear 1", "Light Fighter", 10));
    assert!(!small.matches_ship("CS Tarsis", "Colonial Cruiser - Kinunir", 1200));
  }
}

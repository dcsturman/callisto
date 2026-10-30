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
use callisto::tactics::{self, Flight, Helm, Intent};
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
  /// When to ask the commander again.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub call_me_when: Vec<Trigger>,
}

/// One squadron's orders: its steps, in turn. `squadron` `*` is every
/// squadron without a script of its own. A script naming a `ship` instead
/// takes that ship out of her squadron to fight on her own.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Script {
  #[serde(default)]
  pub squadron: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ship: Option<String>,
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
  /// G of thrust each ship keeps back to dodge, at least; what is left of
  /// her drive is for moving. The crews choose when this is not given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub dodge: Option<u8>,
  /// Flight operations for the squadron's carriers and craft. The crews
  /// run them when this is not given (see [`FlightOrder`]).
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub flight: Option<FlightOrder>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub until: Option<Until>,
}

/// What a step says about flight operations. `launch` has the squadron's
/// carriers put their craft out -- every ready craft, or only `wings`, or
/// until `count` of hers are on the plot -- and keeps the craft already out
/// from going home until they are spent. `recover` brings every craft of
/// the squadron's carriers home, and the squadron's own craft. `hold` keeps
/// craft aboard where they are and craft out where they are. Left out, the
/// crews launch when an enemy is within Very Long and go home when spent or
/// when nothing is left to fight.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq, Eq)]
pub struct FlightOrder {
  #[serde(default)]
  pub op: FlightOp,
  /// `launch` only: launch until this many of each carrier's craft are on
  /// the plot. All of them when left out.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub count: Option<u32>,
  /// `launch` only: only craft of these squadrons.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub wings: Vec<String>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FlightOp {
  #[default]
  Launch,
  Recover,
  Hold,
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

/// How a squadron moves: `network` (as the network chooses), `burn`
/// toward (or `away` from) a place at up to `thrust` G, or `arrive` at a
/// place -- or at `band` of it, on this side -- by `round`, the squadron
/// arriving together.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Movement {
  #[serde(default)]
  pub mode: Mode,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub toward: Option<Place>,
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub away: bool,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub thrust: Option<f64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at: Option<Place>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub band: Option<Range>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub round: Option<u32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
  #[default]
  Network,
  Burn,
  Arrive,
}

/// A place, named as the players know it: the first of these given.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Place {
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub planet: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub ship: Option<String>,
  /// The middle of one of our squadrons.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub squadron: Option<String>,
  /// The middle of the enemies a rule picks out.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub enemies: Option<Rule>,
  /// A point, in km.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub at_km: Option<[f64; 3]>,
}

/// When a step ends: as soon as any of these given is true.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Until {
  /// This round has begun.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub round: Option<u32>,
  /// Any of the squadron is within a band of a place.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub within: Option<Within>,
  /// The squadron took damage last round.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub fired_on: bool,
  /// The squadron has lost this share of its hull since the step began.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub losses: Option<f32>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Within {
  pub of: Place,
  pub band: Range,
}

/// When the commander wants to be asked again: any of these given.
#[derive(Serialize, Deserialize, Debug, Clone, Default, PartialEq)]
pub struct Trigger {
  /// This squadron (any, if absent) has lost this share of its hull since
  /// the orders were given.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub squadron: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub losses: Option<f32>,
  /// Enemies not seen when the orders were given have been found.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub new_enemies: bool,
  /// This round has begun.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub round: Option<u32>,
}

/// Where a squadron is in its script.
#[derive(Debug, Clone, Default)]
struct Progress {
  step: usize,
  /// The squadron's hull when the step began, and last round.
  hull_at_step: u64,
  hull_last_round: u64,
}

/// The group name of a ship with orders of her own.
fn detached_name(ship: &str) -> String {
  format!("ship:{ship}")
}

/// Our squadrons as they stand: by name, each with her ships, in name order.
/// Ships in `detached` have orders of their own and fight alone.
#[must_use]
pub fn squadrons(own: &[Ship], detached: &[String]) -> Vec<(String, Vec<String>)> {
  let mut named: HashMap<String, Vec<String>> = HashMap::new();
  let mut loose: Vec<String> = Vec::new();
  for ship in own {
    if detached.iter().any(|name| name == ship.get_name()) {
      named.insert(detached_name(ship.get_name()), vec![ship.get_name().to_string()]);
      continue;
    }
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
  if let Some(name) = &place.planet {
    return view
      .planets
      .iter()
      .find(|planet| planet.get_name() == name)
      .map(Entity::get_position);
  }
  if let Some(name) = &place.ship {
    return view
      .own
      .iter()
      .find(|ship| ship.get_name() == name)
      .map(Entity::get_position)
      .or_else(|| view.contacts.iter().find(|contact| contact.name == *name).map(|c| c.position));
  }
  if let Some(name) = &place.squadron {
    return groups
      .iter()
      .find(|(group, _)| group == name)
      .and_then(|(_, ships)| middle(ships_of(view, ships).into_iter().map(Entity::get_position)));
  }
  if let Some(rule) = &place.enemies {
    return middle(view.enemies().into_iter().filter(|c| rule.matches(c)).map(|c| c.position));
  }
  place.at_km.map(|[x, y, z]| Vec3::new(x, y, z) * 1000.0)
}

/// Whether `until` has come, for a squadron of `ships` in `progress`.
fn reached(
  until: &Until, view: &Observation, ships: &[&Ship], groups: &[(String, Vec<String>)], progress: &Progress,
) -> bool {
  let hull: u64 = ships.iter().map(|ship| u64::from(ship.current_hull)).sum();
  let round = until.round.is_some_and(|round| view.round >= round);
  let within = until.within.as_ref().is_some_and(|within| {
    locate(&within.of, view, groups).is_some_and(|at| {
      ships
        .iter()
        .any(|ship| tactics::band_at((at - ship.get_position()).magnitude()) as usize <= within.band as usize)
    })
  });
  let fired_on = until.fired_on && hull < progress.hull_last_round;
  #[allow(clippy::cast_precision_loss)]
  let losses = until.losses.is_some_and(|share| {
    progress.hull_at_step > 0
      && (progress.hull_at_step - hull.min(progress.hull_at_step)) as f32 >= share * progress.hull_at_step as f32
  });
  round || within || fired_on || losses
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
  let station = |at: Vec3| Helm::Station {
    position: [at.x, at.y, at.z],
  };
  match movement.mode {
    Mode::Network => None,
    Mode::Burn => {
      let at = locate(movement.toward.as_ref()?, view, groups)?;
      let target = if movement.away {
        // Far enough the other way that she never arrives.
        centre + (centre - at).normalize() * 1.0e12
      } else {
        at
      };
      Some((station(target), movement.thrust))
    }
    Mode::Arrive => {
      let at = locate(movement.at.as_ref()?, view, groups)?;
      let target = match movement.band {
        Some(band) => {
          let offset = centre - at;
          let distance = offset.magnitude();
          if distance < 1.0 {
            at
          } else {
            at + offset / distance * tactics::aim_point(band).min(distance)
          }
        }
        None => at,
      };
      let Some(round) = movement.round else {
        return Some((station(target), movement.thrust));
      };
      // Accelerate half the way and brake the rest, in the rounds left:
      // d = a t^2 / 4.
      let time = f64::from(round.saturating_sub(view.round).max(1)) * DELTA_TIME_F64;
      let needed = 4.0 * (target - centre).magnitude() / (time * time) / G;
      Some((station(target), Some(needed.max(0.05))))
    }
  }
}

/// A step in a few words, for the players: "engage Light Fighter; arrive at
/// Medium of the enemy by round 6; until round 6 or fired on".
#[must_use]
pub fn describe(step: &Step) -> String {
  let place = |place: &Place| {
    place
      .planet
      .clone()
      .or_else(|| place.ship.clone())
      .or_else(|| place.squadron.clone())
      .or_else(|| {
        place
          .enemies
          .as_ref()
          .map(|rule| match (rule.designs.first(), rule.ships.first()) {
            (Some(design), _) => format!("the {design}s"),
            (None, Some(ship)) => ship.clone(),
            (None, None) => "the enemy".to_string(),
          })
      })
      .unwrap_or_else(|| "a point".to_string())
  };
  let mut parts: Vec<String> = Vec::new();
  if let Some(goal) = step.goal {
    parts.push(format!("{goal:?}").to_lowercase());
  }
  if let Some(rule) = &step.engage {
    let mut who: Vec<String> = rule.designs.iter().chain(&rule.ships).cloned().collect();
    if let Some(tons) = rule.max_tons {
      who.push(format!("up to {tons} t"));
    }
    if let Some(tons) = rule.min_tons {
      who.push(format!("{tons} t and up"));
    }
    parts.push(format!(
      "engage {}",
      if who.is_empty() {
        "anyone".to_string()
      } else {
        who.join(", ")
      }
    ));
  }
  if let Some(dodge) = step.dodge {
    parts.push(format!("dodge {dodge} G"));
  }
  if let Some(flight) = &step.flight {
    parts.push(match flight.op {
      FlightOp::Launch => {
        let what = if !flight.wings.is_empty() {
          flight.wings.join(", ")
        } else if let Some(count) = flight.count {
          format!("{count} craft")
        } else {
          "every craft".to_string()
        };
        format!("launch {what}")
      }
      FlightOp::Recover => "recover the craft".to_string(),
      FlightOp::Hold => "hold the craft".to_string(),
    });
  }
  match step.movement.mode {
    Mode::Network => {}
    Mode::Burn => parts.push(format!(
      "burn {} {}",
      if step.movement.away { "away from" } else { "toward" },
      step.movement.toward.as_ref().map_or_else(|| "a point".to_string(), place)
    )),
    Mode::Arrive => parts.push(format!(
      "arrive at {}{}{}",
      step.movement.band.map(|band| format!("{band:?} of ")).unwrap_or_default(),
      step.movement.at.as_ref().map_or_else(|| "a point".to_string(), place),
      step
        .movement
        .round
        .map(|round| format!(" by round {round}"))
        .unwrap_or_default()
    )),
  }
  if let Some(until) = &step.until {
    let mut ends: Vec<String> = Vec::new();
    if let Some(round) = until.round {
      ends.push(format!("round {round}"));
    }
    if let Some(within) = &until.within {
      ends.push(format!("within {:?} of {}", within.band, place(&within.of)));
    }
    if until.fired_on {
      ends.push("fired on".to_string());
    }
    if let Some(share) = until.losses {
      ends.push(format!("{:.0}% lost", share * 100.0));
    }
    parts.push(format!("until {}", ends.join(" or ")));
  }
  if parts.is_empty() {
    "fight as the crews judge".to_string()
  } else {
    parts.join("; ")
  }
}

fn capitalise(text: &str) -> String {
  let mut chars = text.chars();
  chars
    .next()
    .map_or_else(String::new, |first| first.to_uppercase().chain(chars).collect())
}

/// Keep at least `dodge` G back to dodge on every ship, up to her drive, and
/// cap her burn to what is left. Running away would spend the reserve, so a
/// ship told to dodge holds instead.
fn hold_back(decided: &mut [(String, Intent)], ships: &[&Ship], dodge: u8) {
  for (name, intent) in decided.iter_mut() {
    let Some(ship) = ships.iter().find(|ship| ship.get_name() == name) else {
      continue;
    };
    let drive = ship.max_acceleration() + ship.get_dodge_thrust() + u8::from(ship.get_assist_gunners());
    let reserve = intent.evade.unwrap_or(0).max(dodge).min(drive);
    // The report was written for the crews' own reserve: say the real one.
    if let Some(chosen) = intent.evade.filter(|chosen| *chosen != reserve) {
      intent.rationale = intent
        .rationale
        .replace(&format!("{chosen} G kept to dodge"), &format!("{reserve} G kept to dodge"));
    }
    intent.evade = Some(reserve);
    if matches!(intent.helm, Helm::OpenRange { .. }) {
      intent.helm = Helm::Hold;
    }
    let left = f64::from(drive - reserve);
    intent.max_thrust = Some(intent.max_thrust.map_or(left, |cap| cap.min(left)));
  }
}

/// The flight intents a step gives, for the squadron's own `members` and,
/// through its carriers, for their craft wherever they are: `(ship, flight,
/// own)`, where `own` says the ship is a member (a craft's own squadron's
/// step outranks what her carrier's squadron says of her).
fn flight_intents(order: &FlightOrder, members: &[String], view: &Observation) -> Vec<(String, Flight, bool)> {
  let is_member = |name: &str| members.iter().any(|member| member == name);
  let mut intents = Vec::new();
  for ship in view.own.iter().filter(|ship| is_member(ship.get_name())) {
    let name = ship.get_name();
    if ship.is_carrier() {
      let craft_out: Vec<&Ship> = view.own.iter().filter(|craft| craft.carrier.as_deref() == Some(name)).collect();
      match order.op {
        FlightOp::Launch => {
          let ready: Vec<String> = tactics::ready_craft(ship)
            .into_iter()
            .filter(|craft| {
              order.wings.is_empty()
                || ship
                  .craft_aboard(craft)
                  .and_then(|craft| craft.squadron.as_deref())
                  .is_some_and(|wing| order.wings.iter().any(|wanted| wanted.eq_ignore_ascii_case(wing)))
            })
            .collect();
          // Until `count` are out: what is still wanted, less what is. With
          // enough out, or nothing ready, she holds -- rather than being
          // left to the crews, who would launch the rest.
          let wanted = order
            .count
            .map(|count| count.saturating_sub(u32::try_from(craft_out.len()).unwrap_or(u32::MAX)));
          let flight = if wanted == Some(0) || ready.is_empty() {
            Flight::Hold
          } else {
            Flight::Launch {
              craft: ready,
              count: wanted,
            }
          };
          intents.push((name.to_string(), flight, true));
          // Craft already out stay out until spent.
          intents.extend(
            craft_out
              .iter()
              .map(|craft| (craft.get_name().to_string(), Flight::Hold, false)),
          );
        }
        FlightOp::Recover => {
          intents.push((name.to_string(), Flight::Hold, true));
          intents.extend(craft_out.iter().map(|craft| {
            (
              craft.get_name().to_string(),
              Flight::Land {
                carrier: name.to_string(),
              },
              false,
            )
          }));
        }
        FlightOp::Hold => intents.push((name.to_string(), Flight::Hold, true)),
      }
    }
    if let Some(carrier) = &ship.carrier {
      let flight = match order.op {
        FlightOp::Recover => Flight::Land {
          carrier: carrier.clone(),
        },
        FlightOp::Launch | FlightOp::Hold => Flight::Hold,
      };
      intents.push((name.to_string(), flight, true));
    }
  }
  intents
}

/// Put the steps' flight orders on the intents: a ship's own squadron's
/// first, else what her carrier's squadron said of her.
fn apply_flight(intents: &mut [(String, Intent)], orders: &[(String, Flight, bool)]) {
  for (ship, intent) in intents.iter_mut() {
    let chosen = orders
      .iter()
      .filter(|(name, _, _)| name == ship)
      .max_by_key(|(_, _, own)| *own)
      .map(|(_, flight, _)| flight.clone());
    if let Some(flight) = chosen {
      intent.flight = Some(flight);
    }
  }
}

fn progress_step(progress: &HashMap<String, Progress>, group: &str) -> usize {
  progress.get(group).map_or(0, |progress| progress.step)
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
      .map(|script| script.ship.as_deref().map_or_else(|| script.squadron.clone(), detached_name))
      .collect();
    self.progress.retain(|name, _| unchanged.contains(name));
    self.directive = directive;
  }

  #[must_use]
  pub fn directive(&self) -> &Directive {
    &self.directive
  }

  fn script(&self, group: &str) -> Option<&Script> {
    let scripts = &self.directive.squadrons;
    if let Some(ship) = group.strip_prefix("ship:") {
      return scripts.iter().find(|script| script.ship.as_deref() == Some(ship));
    }
    scripts
      .iter()
      .find(|script| script.ship.is_none() && script.squadron == group)
      .or_else(|| scripts.iter().find(|script| script.ship.is_none() && script.squadron == "*"))
  }

  /// Ships with orders of their own.
  fn detached(&self) -> Vec<String> {
    self
      .directive
      .squadrons
      .iter()
      .filter_map(|script| script.ship.clone())
      .collect()
  }

  /// Where a squadron (or a detached ship, `ship:<name>`) is in its script:
  /// the step it is on and how many it has.
  #[must_use]
  pub fn step_of(&self, group: &str) -> Option<(usize, usize)> {
    let steps = self.script(group)?.steps.len();
    Some((self.progress.get(group).map_or(0, |progress| progress.step), steps))
  }
}

#[async_trait]
impl Commander for FleetCommander {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, Intent)> {
    let groups = squadrons(&view.own, &self.detached());
    let mut intents = Vec::new();
    let mut flights: Vec<(String, Flight, bool)> = Vec::new();
    for (name, members) in &groups {
      let ships = ships_of(view, members);
      let hull: u64 = ships.iter().map(|ship| u64::from(ship.current_hull)).sum();
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
      if let Some(dodge) = step.dodge {
        hold_back(&mut decided, &ships, dodge);
      }
      if let Some(order) = &step.flight {
        flights.extend(flight_intents(order, members, view));
      }
      // Each ship's report says what her squadron is doing: "Lance, step 1
      // of 2: ...", or just what it is doing for a ship in no squadron, a
      // ship on her own, or a script of one step.
      if !steps.is_empty() {
        let what = describe(&step);
        let named = !name.starts_with('(') && !name.starts_with("ship:");
        let doing = match (named, steps.len()) {
          (true, 1) => format!("{name}: {what}"),
          (true, n) => format!("{name}, step {} of {n}: {what}", progress_step(&self.progress, name) + 1),
          (false, 1) => capitalise(&what),
          (false, n) => format!("Step {} of {n}: {what}", progress_step(&self.progress, name) + 1),
        };
        for (_, intent) in &mut decided {
          intent.rationale = if intent.rationale.is_empty() {
            doing.clone()
          } else {
            format!("{doing}. {}", intent.rationale)
          };
        }
      }
      intents.extend(decided);
    }
    apply_flight(&mut intents, &flights);
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
               "until": {"round": 4, "fired_on": true}},
              {"engage": {"designs": ["Light Fighter"]}}
            ]},
            {"ship": "CS Hesperus", "steps": [{"movement": {"mode": "burn", "toward": {"planet": "Jupiter"}, "away": true}}]},
            {"squadron": "*", "steps": [{"goal": "hold", "post": {"planet": "Jupiter"}}]}
          ],
          "call_me_when": [{"squadron": "Lance", "losses": 0.5}, {"new_enemies": true}]}"#,
    )
    .unwrap();
    assert_eq!(directive.squadrons.len(), 3);
    let until = directive.squadrons[0].steps[0].until.as_ref().unwrap();
    assert_eq!((until.round, until.fired_on), (Some(4), true));
    assert_eq!(directive.squadrons[1].ship.as_deref(), Some("CS Hesperus"));
    assert_eq!(directive.squadrons[2].steps[0].goal, Some(Goal::Hold));
    assert_eq!(directive.call_me_when.len(), 2);
  }

  /// A step's flight order reads from JSON, writes back the same, and is
  /// described in words.
  #[test]
  fn a_flight_order_round_trips_and_is_described() {
    let cases = [
      (r#"{"flight": {"op": "launch"}}"#, "launch every craft"),
      (r#"{"flight": {"op": "launch", "count": 40}}"#, "launch 40 craft"),
      (
        r#"{"flight": {"op": "launch", "wings": ["Wind Wing 1", "Wind Wing 2"]}}"#,
        "launch Wind Wing 1, Wind Wing 2",
      ),
      (r#"{"flight": {"op": "recover"}}"#, "recover the craft"),
      (r#"{"flight": {"op": "hold"}}"#, "hold the craft"),
    ];
    for (json, words) in cases {
      let step: Step = serde_json::from_str(json).unwrap();
      assert_eq!(describe(&step), words);
      let back: Step = serde_json::from_value(serde_json::to_value(&step).unwrap()).unwrap();
      assert_eq!(back, step);
    }
    let plain: Step = serde_json::from_str("{}").unwrap();
    assert_eq!(plain.flight, None);
    assert_eq!(describe(&plain), "fight as the crews judge");
  }

  fn carrier_and_craft() -> Observation {
    use callisto::ship::{FeatureKind, ShipDesignTemplate, ShipFeature};
    let design = Arc::new(ShipDesignTemplate {
      name: "Carrier".to_string(),
      displacement: 50_000,
      features: vec![
        ShipFeature {
          name: "Docking spaces".to_string(),
          power: 0,
          default_on: true,
          kind: Some(FeatureKind::DockingSpace {
            craft_tons: 50,
            count: 10,
          }),
        },
        ShipFeature {
          name: "Launch tube".to_string(),
          power: 500,
          default_on: true,
          kind: Some(FeatureKind::LaunchTube {
            craft_tons: 50,
            count: 1,
          }),
        },
      ],
      ..Default::default()
    });
    let ship = |name: &str, design: &Arc<ShipDesignTemplate>| {
      Ship::new(
        name.to_string(),
        Vec3::new(0.0, 0.0, 0.0),
        Vec3::new(0.0, 0.0, 0.0),
        design,
        None,
        None,
      )
    };
    let mut carrier = ship("Sirocco", &design);
    carrier.squadron = Some("Battle Line".to_string());
    let fighter = Arc::new(ShipDesignTemplate {
      name: "Fighter".to_string(),
      displacement: 10,
      ..Default::default()
    });
    for n in 1..=4 {
      let mut aboard = ship(&format!("Wing A {n}"), &fighter);
      aboard.squadron = Some("Wing A".to_string());
      carrier.embark(aboard);
    }
    let mut out = ship("Wing B 1", &fighter);
    out.squadron = Some("Wing B".to_string());
    out.carrier = Some("Sirocco".to_string());
    let mut escort = ship("Taipan", &design);
    escort.squadron = Some("Screen".to_string());
    Observation {
      own: vec![carrier, out, escort],
      ..Observation::default()
    }
  }

  /// A carrier's squadron step reaches her craft wherever they are, and a
  /// craft's own squadron step outranks it.
  #[test]
  fn flight_orders_reach_the_carriers_craft_and_her_own_step_wins() {
    let view = carrier_and_craft();
    let line = vec!["Sirocco".to_string()];
    let wing_b = vec!["Wing B 1".to_string()];
    let order = |op: FlightOp| FlightOrder {
      op,
      ..FlightOrder::default()
    };

    // Launch: the carrier launches her ready craft, and the one out stays out.
    let launch = flight_intents(&order(FlightOp::Launch), &line, &view);
    assert_eq!(
      launch,
      vec![
        (
          "Sirocco".to_string(),
          Flight::Launch {
            craft: (1..=4).map(|n| format!("Wing A {n}")).collect(),
            count: None,
          },
          true
        ),
        ("Wing B 1".to_string(), Flight::Hold, false),
      ]
    );
    // Recover: the carrier launches nothing, and her craft come home.
    let recover = flight_intents(&order(FlightOp::Recover), &line, &view);
    assert_eq!(recover[0], ("Sirocco".to_string(), Flight::Hold, true));
    assert_eq!(
      recover[1],
      (
        "Wing B 1".to_string(),
        Flight::Land {
          carrier: "Sirocco".to_string()
        },
        false
      )
    );
    // Count: until that many are out, less the one already out; wings
    // narrow it to the named squadrons.
    let counted = flight_intents(
      &FlightOrder {
        op: FlightOp::Launch,
        count: Some(3),
        wings: vec!["wing a".to_string()],
      },
      &line,
      &view,
    );
    assert!(
      matches!(&counted[0].1, Flight::Launch { craft, count: Some(2) } if craft.len() == 4),
      "{counted:?}"
    );
    let none = flight_intents(
      &FlightOrder {
        op: FlightOp::Launch,
        count: Some(1),
        wings: vec![],
      },
      &line,
      &view,
    );
    assert_eq!(
      none,
      vec![
        ("Sirocco".to_string(), Flight::Hold, true),
        ("Wing B 1".to_string(), Flight::Hold, false)
      ],
      "one is out already: the carrier holds, and so does the craft out"
    );

    // The craft's own squadron's step outranks her carrier's.
    let mut intents: Vec<(String, Intent)> = view
      .own
      .iter()
      .map(|ship| (ship.get_name().to_string(), Intent::default()))
      .collect();
    let mut orders = flight_intents(&order(FlightOp::Launch), &line, &view);
    orders.extend(flight_intents(&order(FlightOp::Recover), &wing_b, &view));
    apply_flight(&mut intents, &orders);
    assert!(matches!(intents[0].1.flight, Some(Flight::Launch { .. })));
    assert_eq!(
      intents[1].1.flight,
      Some(Flight::Land {
        carrier: "Sirocco".to_string()
      })
    );
    assert_eq!(intents[2].1.flight, None, "the escort is left to the crews");
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

//! What a model answers with: the `plan_round` call.
//!
//! The model never writes an [`Intent`] directly. `Intent` is Rust's shape,
//! with externally tagged enums that neither provider's schema language
//! describes well. [`TeamDecision`] is the flat shape the model fills in --
//! strings from fixed lists, every field optional but the ship's name -- and
//! [`to_intents`] turns it into intents, checking every ship and target it
//! names against what the side can actually see. Anything it cannot carry
//! out is dropped and said in a note, so a mistake costs one order, not the
//! round.

use callisto::entity::Entity;
use callisto::ship::{PowerSystem, Range, ShipSystem};
use callisto::software::SoftwareKind;
use callisto::tactics::{self, Boost, Engage, EngineerVerb, Helm, Intent, Posture, SensorVerb};
use cgmath::InnerSpace;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};

use crate::commander::REPAIR_PRIORITY;
use crate::observation::Observation;

/// The name of the one tool the model is given.
pub const TOOL_NAME: &str = "plan_round";

/// One side's answer for a round.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct TeamDecision {
  /// The side's standing plan, carried to the next round's prompt.
  #[serde(default)]
  pub plan: String,
  #[serde(default)]
  pub ships: Vec<ShipOrder>,
  /// Ships flying together this round, each behind a guide.
  #[serde(default)]
  pub squadrons: Vec<SquadronOrder>,
}

/// A squadron for the round: ships that fly together behind a guide. The
/// guide flies the squadron's helm order at its slowest ship's thrust, easing
/// off while one is out of station; the others keep station on
/// her. Their guns, sensors and engineering come from their own orders as
/// usual.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct SquadronOrder {
  pub ships: Vec<String>,
  pub guide: String,
  #[serde(default)]
  pub helm: Option<HelmOrder>,
}

/// One ship's orders, as the model writes them.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct ShipOrder {
  pub ship: String,
  #[serde(default)]
  pub helm: Option<HelmOrder>,
  #[serde(default)]
  pub posture: Option<PostureName>,
  /// Thrust in G held back for dodging.
  #[serde(default, deserialize_with = "lenient_u8")]
  pub evade: Option<u8>,
  #[serde(default)]
  pub fire: Vec<FireOrder>,
  /// Mounts to put on point defence: laser turrets, which then do not fire.
  #[serde(default, deserialize_with = "lenient_indices")]
  pub defend: Vec<usize>,
  #[serde(default)]
  pub sensors: Vec<SensorOrder>,
  #[serde(default)]
  pub engineering: Vec<EngineerOrder>,
  /// Programs to start or stop on the computer.
  #[serde(default)]
  pub software: Vec<SoftwareOrder>,
  #[serde(default)]
  pub assist_gunners: Option<bool>,
  /// Systems for Auto-Repair to attempt.
  #[serde(default)]
  pub computer_repair: Vec<ShipSystem>,
  /// The captain's boosts, most wanted first.
  #[serde(default)]
  pub boosts: Vec<BoostOrder>,
  #[serde(default)]
  pub rationale: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum HelmVerb {
  Hold,
  Intercept,
  HoldRange,
  OpenRange,
  Station,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct HelmOrder {
  pub verb: HelmVerb,
  #[serde(default)]
  pub target: Option<String>,
  #[serde(default)]
  pub band: Option<Range>,
  /// For `station`: where to go and stop, in km.
  #[serde(default)]
  pub position_km: Option<[f64; 3]>,
  /// The most thrust to use, in G, fractions allowed. Omit for the whole
  /// drive.
  #[serde(default, deserialize_with = "lenient_thrust")]
  pub max_thrust: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FireOrder {
  pub target: String,
  /// Mount numbers, as listed for the ship. Empty means everything that
  /// reaches.
  #[serde(default, deserialize_with = "lenient_indices")]
  pub mounts: Vec<usize>,
  #[serde(default)]
  pub called: Option<ShipSystem>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SensorVerbName {
  Lock,
  BreakLock,
  JamMissiles,
  JamComms,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SensorOrder {
  pub verb: SensorVerbName,
  #[serde(default)]
  pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EngineerVerbName {
  Repair,
  OverloadDrive,
  OverloadPlant,
  Jump,
  Power,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct EngineerOrder {
  pub verb: EngineerVerbName,
  /// For `repair`: the system to see to first. Otherwise the worst of what
  /// keeps the ship fighting and moving.
  #[serde(default)]
  pub system: Option<ShipSystem>,
  /// For `power`: what to switch, and for a weapon or feature which one.
  #[serde(default)]
  pub power_system: Option<PowerName>,
  #[serde(default, deserialize_with = "lenient_u8")]
  pub number: Option<u8>,
  #[serde(default)]
  pub online: Option<bool>,
}

/// A posture as the model names it. `Sharing` is the engine's `Passive`:
/// listening and sharing the picture, which transmits. "Passive" read as
/// "quiet", and a stealthed ship fired from it thinking it hidden.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PostureName {
  Silent,
  #[serde(alias = "Passive")]
  Sharing,
  Active,
}

impl From<PostureName> for Posture {
  fn from(name: PostureName) -> Self {
    match name {
      PostureName::Silent => Posture::Silent,
      PostureName::Sharing => Posture::Passive,
      PostureName::Active => Posture::Active,
    }
  }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BoostKind {
  Fire,
  PointDefence,
  Sensor,
  Engineer,
  Dodge,
  AssistGunners,
  Detection,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BoostOrder {
  pub kind: BoostKind,
  /// The mount, operator or engineer number, where the kind names one.
  #[serde(default, deserialize_with = "lenient_u8")]
  pub number: Option<u8>,
  /// For `detection`: the ship to look for.
  #[serde(default)]
  pub target: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum PowerName {
  Sensors,
  Maneuver,
  Jump,
  Weapon,
  Feature,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoftwareOrder {
  pub program: SoftwareKind,
  pub running: bool,
}

/// Models write integers as `2` or `2.0`, and either should read.
fn lenient_u8<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u8>, D::Error> {
  let value = Option::<f64>::deserialize(deserializer)?;
  #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
  Ok(value.map(|value| value.round().clamp(0.0, 255.0) as u8))
}

fn lenient_thrust<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<f64>, D::Error> {
  let value = Option::<f64>::deserialize(deserializer)?;
  Ok(value.filter(|value| value.is_finite()).map(|value| value.max(0.0)))
}

fn lenient_indices<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<usize>, D::Error> {
  let values = Vec::<f64>::deserialize(deserializer)?;
  #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
  Ok(values.iter().map(|value| value.round().max(0.0) as usize).collect())
}

/// The tool's input schema, in the subset of JSON Schema both providers
/// accept: `type`, `description`, `enum`, `items`, `properties`, `required`.
/// No unions, no nullables, no references.
/// One JSON literal, long because the verb set is; splitting it would only
/// scatter the schema.
#[must_use]
#[allow(clippy::too_many_lines)]
pub fn schema() -> Value {
  let name = |what: &str| json!({"type": "string", "description": what});
  let bands = json!({"type": "string", "enum": ["Short", "Medium", "Long", "VeryLong"]});
  let systems = json!({
    "type": "string",
    "enum": ["Maneuver", "Weapon", "Powerplant", "Sensors", "Bridge", "Jump", "Fuel"]
  });
  let mut schema = json!({
    "type": "object",
    "properties": {
      "plan": {"type": "string", "description": "The side's standing plan, two or three sentences; what each ship is for."},
      "ships": {
        "type": "array",
        "description": "One entry per ship you command.",
        "items": {
          "type": "object",
          "properties": {
            "ship": name("The ship's name, exactly as listed."),
            "helm": {
              "type": "object",
              "description": "What the helm does. Omit to keep the current course.",
              "properties": {
                "verb": {"type": "string", "enum": ["hold", "intercept", "hold_range", "open_range", "station"],
                  "description": "hold: keep the current plan. intercept: close with target and arrive inside band, matching her velocity. hold_range: stay inside band of target. open_range: burn straight away from target at the whole drive's thrust, releasing any dodge reserve and gunner assist this round. station: go to position_km and stop."},
                "target": name("The ship the verb is about: a contact, or a friendly ship."),
                "band": bands,
                "position_km": {"type": "array", "items": {"type": "number"}, "description": "x, y, z in km, for station."},
                "max_thrust": {"type": "number", "description": "The most thrust the helm may use, in G; fractions such as 2.5 are allowed. Every G of burn is +1 to anyone looking for you, so creep with 1 or 2 when staying unseen matters. Omit to use the whole drive."}
              },
              "required": ["verb"]
            },
            "posture": {"type": "string", "enum": ["Silent", "Sharing", "Active"],
              "description": "Silent: radiate nothing; the only quiet posture. Sharing: listen and share the picture with the team, which transmits (+6 to be found). Active: sweep and lock. Omit to keep the current posture."},
            "evade": {"type": "integer", "description": "Thrust in G to hold back for dodging. Omit to keep the current reserve."},
            "fire": {
              "type": "array",
              "description": "Fire orders for this round. Only contacts can be shot; only guns that reach will fire.",
              "items": {
                "type": "object",
                "properties": {
                  "target": name("The contact to fire on."),
                  "mounts": {"type": "array", "items": {"type": "integer"}, "description": "Mount numbers to use. Omit for every gun that reaches."},
                  "called": {"type": "string", "enum": ["Maneuver", "Weapon", "Powerplant", "Sensors", "Bridge", "Jump", "Fuel"], "description": "A called shot on this system. Only at Short range; -2 to hit."}
                },
                "required": ["target"]
              }
            },
            "defend": {"type": "array", "items": {"type": "integer"}, "description": "Mount numbers to put on point defence against incoming missiles this round: laser turrets only, and they then do not fire. Size it to the missiles coming; leave empty to attack with everything."},
            "sensors": {
              "type": "array",
              "description": "One per sensor operator aboard.",
              "items": {
                "type": "object",
                "properties": {
                  "verb": {"type": "string", "enum": ["lock", "break_lock", "jam_missiles", "jam_comms"]},
                  "target": name("The contact, for lock, break_lock and jam_comms.")
                },
                "required": ["verb"]
              }
            },
            "engineering": {
              "type": "array",
              "description": "One per engineer aboard.",
              "items": {
                "type": "object",
                "properties": {
                  "verb": {"type": "string", "enum": ["repair", "overload_drive", "overload_plant", "jump", "power"],
                    "description": "repair: try to fix a critical. overload_drive / overload_plant: hard check for +1 G or more power for a few rounds. jump: leave the fight, if the ship is clear of every body's 100-diameter limit with fuel and Jump Control running. power: shut a system down to free its Power (a check), or bring it back online (no check)."},
                  "system": systems,
                  "power_system": {"type": "string", "enum": ["Sensors", "Maneuver", "Jump", "Weapon", "Feature"], "description": "For power: what to switch."},
                  "number": {"type": "integer", "description": "For power on a Weapon or Feature: its number."},
                  "online": {"type": "boolean", "description": "For power: true to bring it back, false to shut it down."}
                },
                "required": ["verb"]
              }
            },
            "software": {
              "type": "array",
              "description": "Programs to start or stop on the ship's computer, within its bandwidth. Stops apply first. Free; takes effect this round.",
              "items": {
                "type": "object",
                "properties": {
                  "program": {"type": "string", "enum": ["JumpControl", "Evade", "FireControl", "AdvancedFireControl", "LaunchSolution", "BroadSpectrumEw", "ElectronicWarfare", "PointDefence", "VirtualGunner", "AutoRepair", "BattleNetwork", "ScreenOptimiser"]},
                  "running": {"type": "boolean"}
                },
                "required": ["program", "running"]
              }
            },
            "assist_gunners": {"type": "boolean", "description": "The pilot spends the round helping the gunners aim instead of only flying: a pilot check whose result is a bonus or penalty on every attack this ship makes. Costs 1 G of thrust. Omit to keep the current setting."},
            "computer_repair": {"type": "array", "items": systems, "description": "Systems for the Auto-Repair program to attempt, one per point of the program. Needs repair drones."},
            "boosts": {
              "type": "array",
              "description": "The captain's leadership check, and what to spend its points on, most wanted first. Each point bought is +1 to one roll; the check decides how many you get. Omit for no check.",
              "items": {
                "type": "object",
                "properties": {
                  "kind": {"type": "string", "enum": ["fire", "point_defence", "sensor", "engineer", "dodge", "assist_gunners", "detection"]},
                  "number": {"type": "integer", "description": "The mount (fire, point_defence), sensor operator or engineer, counting from 0."},
                  "target": name("For detection: the ship to search for.")
                },
                "required": ["kind"]
              }
            },
            "rationale": name("One sentence on why, for the admiral.")
          },
          "required": ["ship", "rationale"]
        }
      }
    },
    "required": ["plan", "ships"]
  });
  // A squadron's helm order is a ship's, given to the guide.
  let helm = schema["properties"]["ships"]["items"]["properties"]["helm"].clone();
  schema["properties"]["squadrons"] = json!({
    "type": "array",
    "description": "Ships flying together this round, each squadron behind a guide. The guide flies the squadron's helm order at its slowest ship's thrust, easing off a quarter G for each band a ship is out of station (up to 1 G) so she can catch up; the others keep station on her and arrive with her. A ship in a squadron takes her course from it; her other orders (fire, posture, sensors, engineering) still come from her own entry. Omit, or leave a ship out, for ships flying on their own.",
    "items": {
      "type": "object",
      "properties": {
        "ships": {"type": "array", "items": {"type": "string"}, "description": "The ships in the squadron, the guide among them."},
        "guide": name("The ship the others form on."),
        "helm": helm
      },
      "required": ["ships", "guide", "helm"]
    }
  });
  schema
}

/// The squadrons, as a line for the plan: shown back to the model next round,
/// so it keeps them unless something has changed, and to the GM in Fleet Comms.
#[must_use]
pub fn squadrons_line(squadrons: &[SquadronOrder]) -> Option<String> {
  let each: Vec<String> = squadrons
    .iter()
    .map(|squadron| {
      let others: Vec<&str> = squadron
        .ships
        .iter()
        .filter(|ship| **ship != squadron.guide)
        .map(String::as_str)
        .collect();
      let order = squadron.helm.as_ref().map_or_else(
        || "holding course".to_string(),
        |helm| {
          let verb = match helm.verb {
            HelmVerb::Hold => "holding course",
            HelmVerb::Intercept => "intercepting",
            HelmVerb::HoldRange => "holding range on",
            HelmVerb::OpenRange => "opening range from",
            HelmVerb::Station => "going to station",
          };
          let target = helm.target.as_deref().map_or_else(String::new, |target| format!(" {target}"));
          let band = helm.band.map_or_else(String::new, |band| format!(" at {band:?}"));
          format!("{verb}{target}{band}")
        },
      );
      format!("{} leading {} ({order})", squadron.guide, others.join(", "))
    })
    .collect();
  (!each.is_empty()).then(|| format!("Squadrons: {}.", each.join("; ")))
}

/// What the tool is for, as the model is told.
pub const TOOL_DESCRIPTION: &str =
  "Give this round's orders: the side's standing plan and an order for each ship you command.";

/// Which ships' orders were kept, which parts were dropped, and why.
#[derive(Debug, Default, PartialEq)]
pub struct Conversion {
  pub intents: Vec<(String, Intent)>,
  /// One line per order or part of one that could not be carried out.
  pub notes: Vec<String>,
}

/// Turn a decision into intents for the ships the side commands, dropping
/// what names a ship or target the side cannot see.
#[must_use]
pub fn to_intents(decision: &TeamDecision, view: &Observation) -> Conversion {
  let mut conversion = Conversion::default();
  for order in &decision.ships {
    if !view.own.iter().any(|ship| ship.get_name() == order.ship) {
      conversion
        .notes
        .push(format!("{}: not a ship under this side's command; ignored.", order.ship));
      continue;
    }
    // A ship in a squadron takes her course from it, so her own helm order is
    // not checked: a model often leaves the target off one it knows the
    // squadron will replace, and saying it was dropped is only noise.
    let in_squadron = decision.squadrons.iter().any(|squadron| {
      squadron.ships.contains(&order.ship)
        && squadron.ships.contains(&squadron.guide)
        && view.own.iter().any(|ship| ship.get_name() == squadron.guide)
    });
    let intent = if in_squadron {
      let mut own = order.clone();
      own.helm = None;
      intent_of(&own, view, &mut conversion.notes)
    } else {
      intent_of(order, view, &mut conversion.notes)
    };
    conversion.intents.push((order.ship.clone(), intent));
  }
  let mut placed = std::collections::HashSet::new();
  for squadron in &decision.squadrons {
    squadron_helms(squadron, view, &mut placed, &mut conversion);
  }
  conversion
}

/// The step, in G, by which a guide eases off for a straggler, up to a whole
/// G.
pub const STRAGGLER_STEP: f64 = 0.25;

/// What a guide gives up, in G, for one ship `apart` metres from her and
/// moving at `drift` m/s relative to her: nothing for a ship settled in
/// station (inside Short and drifting less than a twentieth of it a round,
/// as `tactics` keeps station), otherwise enough for her to rejoin in about
/// [`REJOIN_ROUNDS`] -- shed her relative speed and close the gap -- rounded
/// up to a quarter G, and never more than a whole G.
///
/// Sized by what rejoining takes, not by how far out she is: a ship left
/// 17 km/s behind in one round is only a band away at first, but a quarter G
/// takes twenty rounds to stop her falling further back, and by then she is
/// out of the fight.
#[must_use]
pub fn straggler_margin(apart: f64, drift: f64) -> f64 {
  let per_round = drift * callisto::entity::DELTA_TIME_F64;
  if tactics::band_at(apart) == Range::Short && per_round <= SETTLED_DRIFT {
    return 0.0;
  }
  let time = f64::from(REJOIN_ROUNDS) * callisto::entity::DELTA_TIME_F64;
  // Stop the drift, then cover the gap from rest: accelerate half way, brake
  // the other half.
  let needed = drift / time + 4.0 * apart / (time * time);
  let steps = (needed / callisto::entity::G / STRAGGLER_STEP).ceil().max(1.0);
  (steps * STRAGGLER_STEP).min(1.0)
}

/// The rounds a straggler is given to rejoin her squadron.
pub const REJOIN_ROUNDS: u8 = 5;

/// Drift a round, in metres, below which a ship inside Short is settled in
/// station: a twentieth of Short, as `tactics` keeps it.
const SETTLED_DRIFT: f64 = 1_250_000.0 / 20.0;

/// Give a squadron's ships their courses: the guide the squadron's helm order,
/// at the slowest ship's thrust; the rest `Follow` her.
fn squadron_helms(
  squadron: &SquadronOrder, view: &Observation, placed: &mut std::collections::HashSet<String>,
  conversion: &mut Conversion,
) {
  let notes = &mut conversion.notes;
  let own = |name: &str| view.own.iter().find(|ship| ship.get_name() == name);
  let mut members: Vec<&callisto::ship::Ship> = Vec::new();
  for name in &squadron.ships {
    match own(name) {
      None => notes.push(format!("squadron of {}: {name} is not ours; left out.", squadron.guide)),
      Some(ship) if ship.max_acceleration() == 0 => notes.push(format!(
        "squadron of {}: {name} has no thrust to keep station with; left out.",
        squadron.guide
      )),
      Some(_) if placed.contains(name) => notes.push(format!(
        "squadron of {}: {name} is already in another squadron; left out.",
        squadron.guide
      )),
      Some(ship) => members.push(ship),
    }
  }
  if !members.iter().any(|ship| ship.get_name() == squadron.guide) {
    notes.push(format!(
      "squadron of {}: the guide is not one of its ships that can fly; the squadron is not formed.",
      squadron.guide
    ));
    return;
  }
  if members.len() < 2 {
    notes.push(format!(
      "squadron of {}: no other ship to fly with; she flies on her own orders.",
      squadron.guide
    ));
    return;
  }
  // The guide flies at the slowest ship's thrust, so a squadron in station
  // loses nothing by flying together. While a ship is out of station the
  // guide eases off by how far out she is, so a straggler just off her slot
  // costs the squadron little and one left far behind gets thrust to catch up.
  let slowest = f64::from(members.iter().map(|ship| ship.max_acceleration()).min().unwrap_or(1));
  let guide = members.iter().find(|ship| ship.get_name() == squadron.guide);
  let margin = guide.map_or(0.0, |guide| {
    members
      .iter()
      .map(|ship| {
        straggler_margin(
          (ship.get_position() - guide.get_position()).magnitude(),
          (ship.get_velocity() - guide.get_velocity()).magnitude(),
        )
      })
      .fold(0.0, f64::max)
  });
  let cap = (slowest - margin).max(STRAGGLER_STEP);
  let helm = squadron.helm.as_ref();
  let guide_helm = helm.map_or(Helm::Hold, |helm| helm_of(&squadron.guide, helm, view, notes));
  let guide_cap = Some(helm.and_then(|helm| helm.max_thrust).map_or(cap, |wanted| wanted.min(cap)));
  for ship in members {
    let name = ship.get_name().to_string();
    placed.insert(name.clone());
    let index = conversion
      .intents
      .iter()
      .position(|(ship, _)| *ship == name)
      .unwrap_or_else(|| {
        conversion.intents.push((name.clone(), Intent::default()));
        conversion.intents.len() - 1
      });
    let intent = &mut conversion.intents[index].1;
    if name == squadron.guide {
      intent.helm = guide_helm.clone();
      intent.max_thrust = guide_cap;
    } else {
      intent.helm = Helm::Follow {
        guide: squadron.guide.clone(),
        guide_helm: Box::new(guide_helm.clone()),
        guide_cap,
      };
      intent.max_thrust = None;
    }
  }
}

fn is_contact(view: &Observation, name: &str) -> bool {
  view.contacts.iter().any(|contact| contact.name == name)
}

fn is_blip(view: &Observation, name: &str) -> bool {
  view.blips.iter().any(|blip| blip.name == name)
}

fn is_friend(view: &Observation, name: &str) -> bool {
  view.own.iter().chain(&view.allies).any(|ship| ship.get_name() == name)
}

fn intent_of(order: &ShipOrder, view: &Observation, notes: &mut Vec<String>) -> Intent {
  let ship = &order.ship;
  let helm = order.helm.as_ref().map_or(Helm::Hold, |helm| helm_of(ship, helm, view, notes));
  let fire = order
    .fire
    .iter()
    .filter(|fire| {
      let known = is_contact(view, &fire.target);
      if !known {
        notes.push(format!("{ship}: cannot fire on {}, not a contact.", fire.target));
      }
      known
    })
    .map(|fire| Engage {
      target: fire.target.clone(),
      weapons: fire.mounts.clone(),
      called: fire.called,
    })
    .collect();
  let sensors = order
    .sensors
    .iter()
    .filter_map(|sensor| sensor_of(ship, sensor, view, notes))
    .collect();
  let engineering = order
    .engineering
    .iter()
    .map(|engineer| match engineer.verb {
      EngineerVerbName::Repair => EngineerVerb::Repair {
        priority: engineer.system.map_or_else(|| REPAIR_PRIORITY.to_vec(), |system| vec![system]),
      },
      EngineerVerbName::OverloadDrive => EngineerVerb::OverloadDrive,
      EngineerVerbName::OverloadPlant => EngineerVerb::OverloadPlant,
      EngineerVerbName::Jump => EngineerVerb::Jump,
      EngineerVerbName::Power => {
        let number = usize::from(engineer.number.unwrap_or(0));
        let system = match engineer.power_system {
          Some(PowerName::Sensors) => PowerSystem::Sensors,
          Some(PowerName::Maneuver) => PowerSystem::Maneuver,
          Some(PowerName::Jump) => PowerSystem::Jump,
          Some(PowerName::Feature) => PowerSystem::Feature(number),
          Some(PowerName::Weapon) | None => PowerSystem::Weapon(number),
        };
        EngineerVerb::Power {
          system,
          online: engineer.online.unwrap_or(false),
        }
      }
    })
    .collect();
  let boosts = order
    .boosts
    .iter()
    .filter_map(|boost| {
      let number = usize::from(boost.number.unwrap_or(0));
      Some(match boost.kind {
        BoostKind::Fire => Boost::Fire { mount: number },
        BoostKind::PointDefence => Boost::PointDefence { mount: number },
        BoostKind::Sensor => Boost::Sensor { operator: number },
        BoostKind::Engineer => Boost::Engineer { engineer: number },
        BoostKind::Dodge => Boost::Dodge,
        BoostKind::AssistGunners => Boost::AssistGunners,
        BoostKind::Detection => Boost::Detection {
          target: boost.target.clone()?,
        },
      })
    })
    .collect();
  Intent {
    posture: order.posture.map(Posture::from),
    max_thrust: order.helm.as_ref().and_then(|helm| helm.max_thrust),
    helm,
    evade: order.evade,
    fire,
    defend: order.defend.clone(),
    sensors,
    engineering,
    software: order
      .software
      .iter()
      .map(|software| tactics::SoftwareOrder {
        program: software.program,
        running: software.running,
      })
      .collect(),
    assist_gunners: order.assist_gunners,
    computer_repair: order.computer_repair.clone(),
    boosts,
    rationale: order.rationale.clone(),
  }
}

fn helm_of(ship: &str, helm: &HelmOrder, view: &Observation, notes: &mut Vec<String>) -> Helm {
  let mut target = |what: &str| -> Option<String> {
    match &helm.target {
      Some(target) if is_contact(view, target) || is_blip(view, target) || is_friend(view, target) => {
        Some(target.clone())
      }
      Some(target) => {
        notes.push(format!(
          "{ship}: cannot {what} {target}, not a ship we can see; holding course."
        ));
        None
      }
      None => {
        notes.push(format!("{ship}: {what} names no target; holding course."));
        None
      }
    }
  };
  match helm.verb {
    HelmVerb::Hold => Helm::Hold,
    HelmVerb::Intercept => target("intercept").map_or(Helm::Hold, |target| Helm::Intercept {
      target,
      band: helm.band.unwrap_or(Range::Medium),
    }),
    HelmVerb::HoldRange => target("hold range on").map_or(Helm::Hold, |target| Helm::HoldRange {
      target,
      band: helm.band.unwrap_or(Range::Medium),
    }),
    HelmVerb::OpenRange => target("open range from").map_or(Helm::Hold, |target| Helm::OpenRange { target }),
    HelmVerb::Station => helm.position_km.map_or_else(
      || {
        notes.push(format!("{ship}: station names no position; holding course."));
        Helm::Hold
      },
      |[x, y, z]| Helm::Station {
        position: [x * 1000.0, y * 1000.0, z * 1000.0],
      },
    ),
  }
}

fn sensor_of(ship: &str, sensor: &SensorOrder, view: &Observation, notes: &mut Vec<String>) -> Option<SensorVerb> {
  let mut contact = |what: &str| -> Option<String> {
    match &sensor.target {
      Some(target) if is_contact(view, target) => Some(target.clone()),
      Some(target) => {
        notes.push(format!("{ship}: cannot {what} {target}, not a contact."));
        None
      }
      None => {
        notes.push(format!("{ship}: {what} names no target."));
        None
      }
    }
  };
  match sensor.verb {
    SensorVerbName::Lock => contact("lock").map(|target| SensorVerb::Lock { target }),
    SensorVerbName::BreakLock => contact("break lock on").map(|target| SensorVerb::BreakLock { target }),
    SensorVerbName::JamMissiles => Some(SensorVerb::JamMissiles),
    SensorVerbName::JamComms => contact("jam").map(|target| SensorVerb::JamComms { target }),
  }
}

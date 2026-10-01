#![allow(clippy::elidable_lifetime_names)]

use std::cell::RefCell;
use std::collections::HashMap;
use std::error::Error;
use std::fmt::{Debug, Display, Formatter, Result as FmtResult};
use std::hash::BuildHasher;
use std::sync::{Arc, RwLock};

use cgmath::{InnerSpace, Zero};
use derivative::Derivative;
use once_cell::sync::OnceCell;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, skip_serializing_none};

use crate::software::{Software, SoftwareKind};
use strum_macros::{EnumIter, FromRepr};

use futures::stream::{self, StreamExt};

use crate::computer::MAX_ACCEL_WIGGLE_ROOM;
use crate::crew::Crew;
use crate::entity::{Entity, UpdateAction, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME, DELTA_TIME_F64, G};
use crate::payloads::Vec3asVec;
use crate::{debug, error, warn};
use crate::{list_local_or_cloud_dir, read_local_or_cloud_file, MAX_CONCURRENT_DIR_FILE_READS};

/// Directory holding one ship-design JSON file per design. Mirrors the
/// scenarios layout: each file is a self-contained `ShipDesignTemplate`
/// (object, not array). Adding/changing a file triggers a watcher reload
/// and the new design is merged into the global registry. Removing a file
/// is intentionally harmless — the in-memory copy persists so any in-flight
/// scenario that references the design keeps working.
pub const DEFAULT_SHIP_TEMPLATES_DIR: &str = "./ship_templates/";

pub type ShipTemplateTable = HashMap<String, Arc<ShipDesignTemplate>>;
type SharedShipTemplateTable = Arc<ShipTemplateTable>;

pub static SHIP_TEMPLATES: OnceCell<RwLock<SharedShipTemplateTable>> = OnceCell::new();

std::thread_local! {
  static DESERIALIZING_SHIP_TEMPLATES: RefCell<Option<SharedShipTemplateTable>> = const { RefCell::new(None) };
}

struct ShipTemplateDeserializationGuard(Option<SharedShipTemplateTable>);

impl Drop for ShipTemplateDeserializationGuard {
  fn drop(&mut self) {
    DESERIALIZING_SHIP_TEMPLATES.with(|templates| {
      *templates.borrow_mut() = self.0.take();
    });
  }
}

/// Replace the current global ship-template snapshot.
///
/// # Panics
///
/// Panics if the write lock is poisoned.
pub fn replace_ship_templates<S>(templates: HashMap<String, Arc<ShipDesignTemplate>, S>)
where
  S: BuildHasher,
{
  let templates = Arc::new(templates.into_iter().collect::<ShipTemplateTable>());
  let templates_lock = SHIP_TEMPLATES.get_or_init(|| RwLock::new(templates.clone()));
  *templates_lock
    .write()
    .expect("(replace_ship_templates) Unable to update ship templates") = templates;
}

/// Merge the supplied templates into the current global snapshot. Existing
/// entries with the same name are overwritten by the new ones. Entries that
/// are NOT present in the new map are preserved — this is the intended
/// reload semantics so that a removed-on-disk design doesn't disappear
/// from memory while an active scenario may still be referencing it.
///
/// # Panics
///
/// Panics if the write lock is poisoned.
pub fn merge_ship_templates<S>(new_templates: HashMap<String, Arc<ShipDesignTemplate>, S>)
where
  S: BuildHasher,
{
  let merged: ShipTemplateTable = match SHIP_TEMPLATES.get() {
    Some(lock) => {
      let existing = lock
        .read()
        .expect("(merge_ship_templates) Unable to read existing ship templates")
        .clone();
      let mut merged: ShipTemplateTable = (*existing).clone();
      for (name, template) in new_templates {
        merged.insert(name, template);
      }
      merged
    }
    None => new_templates.into_iter().collect(),
  };
  let merged_arc = Arc::new(merged);
  let lock = SHIP_TEMPLATES.get_or_init(|| RwLock::new(merged_arc.clone()));
  *lock.write().expect("(merge_ship_templates) Unable to update ship templates") = merged_arc;
}

/// Return the current global ship-template snapshot.
///
/// # Panics
///
/// Panics if ship templates have not been initialized yet or if the read lock is poisoned.
#[must_use]
pub fn get_ship_templates_snapshot() -> SharedShipTemplateTable {
  SHIP_TEMPLATES
    .get()
    .expect("(get_ship_templates_snapshot) Ship templates not loaded")
    .read()
    .expect("(get_ship_templates_snapshot) Unable to read ship templates")
    .clone()
}

#[must_use]
pub fn get_ship_template(name: &str) -> Option<Arc<ShipDesignTemplate>> {
  get_ship_templates_snapshot().get(name).cloned()
}

pub(crate) fn with_ship_templates_for_deserialization<T, F>(
  ship_templates: Arc<HashMap<String, Arc<ShipDesignTemplate>>>, callback: F,
) -> T
where
  F: FnOnce() -> T,
{
  let previous_templates = DESERIALIZING_SHIP_TEMPLATES.with(|templates| templates.replace(Some(ship_templates)));
  let _reset_guard = ShipTemplateDeserializationGuard(previous_templates);
  callback()
}

fn get_ship_template_for_deserialization(name: &str) -> Option<Arc<ShipDesignTemplate>> {
  DESERIALIZING_SHIP_TEMPLATES
    .with(|templates| templates.borrow().as_ref().and_then(|snapshot| snapshot.get(name).cloned()))
    .or_else(|| get_ship_template(name))
}

#[skip_serializing_none]
#[serde_as]
#[derive(Derivative)]
#[derivative(PartialEq, Debug)]
#[derive(Serialize, Deserialize, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct Ship {
  name: String,
  #[serde_as(as = "Vec3asVec")]
  position: Vec3,
  #[serde_as(as = "Vec3asVec")]
  velocity: Vec3,
  pub plan: FlightPlan,

  #[serde_as(as = "TemplateNameOnly")]
  #[derivative(PartialEq = "ignore")]
  #[derivative(Debug(format_with = "format_ship_template_name_only"))]
  pub design: Arc<ShipDesignTemplate>,

  /// Per-ship armament.  `None` means "use the design's weapons", which is what
  /// every pre-existing scenario deserializes to and what we store whenever a
  /// client omits the field.  Read it through [`Ship::weapons`], never directly.
  #[serde(default)]
  weapons: Option<Vec<Weapon>>,

  #[serde(default)]
  pub current_hull: u32,
  #[serde(default)]
  pub current_armor: u32,
  #[serde(default)]
  pub current_power: u32,
  #[serde(default)]
  pub current_maneuver: u8,
  #[serde(default)]
  pub current_jump: u8,
  #[serde(default)]
  pub current_fuel: u32,
  #[serde(default)]
  pub current_crew: u32,
  #[serde(default)]
  pub current_sensors: Sensors,

  /// The ship's computer Bandwidth, as reduced by bridge crits.
  ///
  /// TODO: implement computer software. Nothing runs on the computer yet, so
  /// this is only ever spent by a sensor hand-off (one point at each end) and
  /// there is no notion of *available* Bandwidth distinct from the rating.
  /// Once software exists, both the maximum and what is left should reach the
  /// client and be shown on the ship's display; until then they are always the
  /// same number and showing it would tell a player nothing.
  #[serde(default)]
  pub current_computer: u32,
  #[serde(default)]
  pub active_weapons: Vec<bool>,

  #[derivative(PartialEq = "ignore")]
  #[serde(default)]
  pub sensor_locks: Vec<String>,

  /// The ships this one currently detects.
  ///
  /// Directional: A having a contact on B says nothing about whether B has one
  /// on A, which is what lets stealth work at all. Sticky once established -
  /// High Guard p. 77, "after initial contact, sensor detection is maintained
  /// under most circumstances" - so it is dropped only by losing a stealthed
  /// target across a range band or by the range opening past Distant.
  ///
  /// A sensor lock requires a contact, and more broadly nothing can be done to
  /// a ship that is not detected. Omitted from the wire when empty.
  #[derivative(PartialEq = "ignore")]
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub contacts: Vec<String>,

  /// Whether the ship is running active radar/lidar.
  ///
  /// Active sensors are what let a sensop pinpoint another ship at all: High
  /// Guard p. 77, "attempting to locate a ship with this level of accuracy
  /// requires the use of active sensors". They also announce the ship, handing
  /// anyone looking for it DM+2 on the Initial Detection table.
  ///
  /// Running dark keeps the contacts already held - detection "is maintained
  /// under most circumstances" - but acquires nothing new and cannot lock.
  #[serde(default = "default_true", skip_serializing_if = "is_true")]
  pub active_sensors: bool,

  /// Whether the ship is radiating on RF: transponder, radio comms, or both.
  ///
  /// High Guard's row is "transponder **or** radio comms" at +6 — the single
  /// largest modifier on the detection table — so the two are one flag. A
  /// merchant squawking its transponder because it believes all is well, and a
  /// stealth ship breaking silence to warn a team-mate, are the same emission
  /// as far as anyone hunting them is concerned.
  ///
  /// Defaults to **off**. RAW expects transponders on in civilised space, but
  /// this is the largest row on the detection table by some margin, and a ship
  /// left transmitting by accident is simply found — which would quietly undo
  /// stealth for any scenario whose author did not think about it. Defaulting
  /// off means a scenario opts into the noise deliberately, which is the safer
  /// direction for a switch this loud. Scenario builders can turn it on per
  /// ship when adding one.
  ///
  /// Receiving a transmission does not set this. Listening is passive; only
  /// sending gives you away.
  #[serde(default, skip_serializing_if = "is_false")]
  pub transmitting: bool,

  /// Whether this ship is sharing its sensor picture with its team.
  ///
  /// A hand-off is automatic in RAW — it needs no check and no action, only a
  /// point of computer Bandwidth at each end — so this is a standing setting
  /// rather than something the sensop does each round.
  ///
  /// Sharing means transmitting, so turning this on forces `transmitting` on
  /// and holds it there: a ship cannot pass its contacts to anyone while
  /// running silent. Turning it off releases the flag but does not switch it
  /// back off, since the crew may want to stay lit for other reasons.
  #[serde(default, skip_serializing_if = "is_false")]
  pub handoff_sensors: bool,

  /// Whether this ship's comms are being jammed this round.
  ///
  /// Transient: set by a successful `JamComms` and cleared at the end of the
  /// round, so it never reaches the wire or a saved scenario. While it is set
  /// the ship can neither send nor receive a sensor hand-off — jamming stops
  /// communication, and a hand-off is communication.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip)]
  pub comms_jammed: bool,

  /// Which side this ship is on, if any.
  ///
  /// Unaligned by default, and omitted from the wire when unset, so nothing
  /// built before teams existed changes. Nothing enforces it yet — it colours
  /// the display and will be what sensor hand-offs are shared along.
  #[derivative(PartialEq = "ignore")]
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub team: Option<Team>,

  /// The crew aboard, or `None` when the scenario did not say.
  ///
  /// `None` means "take the design's", resolved by `fixup_current_values` on
  /// load -- which is why this is private and read through `get_crew`. Without
  /// the Option an omitted crew and a deliberately green one are the same JSON,
  /// and a design-level crew could never be overridden back down to zero.
  ///
  /// Always `Some` by the time it reaches a client.
  #[derivative(PartialEq = "ignore")]
  #[serde(default)]
  crew: Option<Crew>,

  /// The thrust the pilot has set aside for dodging, as an order that stands
  /// until they change it.
  #[derivative(PartialEq = "ignore")]
  #[serde(default)]
  dodge_thrust: u8,

  /// How much of that order has been used this round. Kept apart from the
  /// order itself: spending it used to eat the order, so a pilot who dodged
  /// two attacks stopped dodging -- that round and every round after -- until
  /// they noticed and typed the number in again. Per-round scratch, so it is
  /// neither saved nor sent.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip)]
  dodge_spent: u8,

  #[derivative(PartialEq = "ignore")]
  #[serde(default)]
  assist_gunners: bool,

  #[derivative(PartialEq = "ignore")]
  #[serde(default)]
  can_jump: bool,

  // Engineer action fields
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_zero_u8")]
  temporary_maneuver: u8,

  /// Rounds the overload has left to run, including this one. House rule: an
  /// overload lasts the Effect of the check in rounds, and an Effect of 0
  /// still buys one. Per-round scratch, so it is not saved.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_zero_u8")]
  temporary_maneuver_rounds: u8,

  #[derivative(PartialEq = "ignore")]
  #[serde(
    skip_deserializing,
    default = "default_power_multiplier",
    skip_serializing_if = "is_default_power_multiplier"
  )]
  temporary_power_multiplier: f32,

  /// Rounds the plant's overload has left, on the same house rule.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_zero_u8")]
  temporary_power_rounds: u8,

  #[derivative(PartialEq = "ignore")]
  last_repair_component: Option<ShipSystem>,

  #[derivative(PartialEq = "ignore")]
  #[serde(default, skip_serializing_if = "is_zero_u8")]
  repair_bonus: u8,

  // Tracks whether engineer has taken an action this turn
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_false")]
  engineer_action_taken: bool,

  // Tracks whether the captain's Evade boost has already been consumed against
  // the FIRST attack on this ship this turn. Cleared by
  // `reset_temporary_bonuses`.
  //
  // Note the asymmetry: there is no `assist_gunner_boost_used` field. The
  // assist-gunner first-only check is local to `do_fire_actions` (one
  // attacker per call, multiple weapons in the same closure), so a
  // ship-level flag is unnecessary there.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_false")]
  evade_boost_used: bool,

  // Leadership points the captain rolled this turn. Set by the server when
  // the captain hits "Captain Action"; consumed at end-of-turn Phase 0 to
  // truncate the captain's queued boost list. Reset to 0 by
  // `reset_temporary_bonuses` so each turn requires a fresh roll.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_zero_i16")]
  leadership_points: i16,

  // Whether the captain has rolled this turn. Lets the FE distinguish
  // "rolled and got 0" from "haven't rolled yet" without an extra option type.
  // Reset by `reset_temporary_bonuses`.
  #[derivative(PartialEq = "ignore")]
  #[serde(skip_deserializing, default, skip_serializing_if = "is_false")]
  leadership_rolled: bool,

  // Index by turning ShipSystem enum into usize.
  // Skip deserializing as we don't expect them when loading from a file
  // and don't intend to receive them from the client.
  // But we do serialize them to send to the client.
  #[serde(skip_deserializing)]
  pub crit_level: [u8; 11],
  /// The state of each bridge station, indexed by `BridgeStation`. Omitted from
  /// the wire while every station works.
  #[serde(default, skip_serializing_if = "all_stations_working")]
  pub bridge_stations: [StationStatus; BridgeStation::COUNT],
  /// Systems the engineer has powered down, by [`PowerSystem`]. Freeing their
  /// draw is the point: a ship short of Power can shut a weapon off to keep
  /// flying.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub offline: Vec<PowerSystem>,
  /// Software aboard. The design's, plus anything a scenario added.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub software: Vec<Software>,
  /// Which of it is running. Bandwidth limits this, not what is installed:
  /// a ship can own more software than its computer can run at once, and
  /// choosing is the point.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub software_running: Vec<Software>,

  /// Basic ship systems running at half power, which High Guard p. 17 allows
  /// in an emergency. Uncomfortable, and the only way to find Power on a ship
  /// with nothing else to shut off.
  #[serde(default, skip_serializing_if = "is_false")]
  pub basic_power_halved: bool,

  /// How many times the engineer has overloaded the drive and the plant this
  /// fight. Each attempt after the first takes a cumulative DM-2 (Core
  /// Rulebook p. 171), cleared only by maintenance out of combat.
  #[serde(default, skip_serializing_if = "is_zero_u8")]
  pub overload_drive_attempts: u8,
  #[serde(default, skip_serializing_if = "is_zero_u8")]
  pub overload_plant_attempts: u8,

  /// What each critical hit took away, in the order it happened, with the
  /// system and the severity that did it. Repairing a system undoes its own
  /// damage from the end.
  ///
  /// Hull, armour, fuel, cargo and crew are not on here. None of them is
  /// repaired in flight: a hole is a hole, spent fuel is spent, and a repair
  /// does not heal anyone.
  ///
  /// Server-side bookkeeping: the client is told each system's severity and
  /// what the ship can currently do, which is all it can show. Kept off the
  /// wire and out of saved scenarios for that reason.
  #[serde(skip)]
  pub damage_log: Vec<(ShipSystem, u8, SystemDamage)>,
  #[serde(skip)]
  pub attack_dm: i32,
  #[serde(skip)]
  pub point_defense_list: Vec<(usize, u16)>,
  /// Everything this ship can shoot down this round, in pool points.
  ///
  /// Batteries contribute their Intercept (High Guard p. 40) and each queued
  /// gunner contributes the Effect of one check (Core Rulebook p. 171); the
  /// book totals them into a single pool, so this does too.  A missile costs
  /// one point and a torpedo two.
  ///
  /// Per-round scratch like `point_defense_list`, so it is not persisted.
  #[serde(skip)]
  pub point_defense_pool: u32,
  /// Damage each of this ship's screens will absorb this round, index-aligned
  /// with the design's `screens`.  Rolled once at the start of resolution and
  /// spent as attacks arrive; per-round scratch, so not persisted.
  #[serde(skip)]
  pub screen_pool: Vec<u32>,
  /// Power currently suppressed by ion hits.
  ///
  /// Ion weapons deal no lasting harm -- the Power comes back when the effect
  /// lapses -- so this is tracked apart from `current_power` rather than
  /// subtracted from it.  Keeping them separate means a repair cannot
  /// accidentally "fix" an ion hit, and an ion hit cannot mask real damage.
  ///
  /// Omitted from the wire when zero, so a ship nobody has shot with an ion
  /// cannon serializes exactly as it did before ion existed.
  #[serde(default, skip_serializing_if = "is_zero_u32")]
  pub ion_power_loss: u32,
  /// Rounds of ion suppression still to run.  Zero means none.
  #[serde(default, skip_serializing_if = "is_zero_u8")]
  pub ion_rounds: u8,
}

fn default_power_multiplier() -> f32 {
  1.0
}

/// A helper function to avoid serializing when zero.  It makes
/// the use of a reference a bit funny, but necessary.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_default_power_multiplier(value: &f32) -> bool {
  (*value - 1.0).abs() < f32::EPSILON
}

/// A helper function to avoid serializing when zero.  It makes
/// the use of a reference a bit funny, but necessary.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u8(value: &u8) -> bool {
  *value == 0
}

/// Active sensors default to on: a ship runs them unless the crew decides to
/// go quiet.
fn default_true() -> bool {
  true
}

/// Paired with `default_true` so a ship running normally adds nothing to the
/// wire or to a saved scenario.
///
/// Takes a reference because that is what `skip_serializing_if` hands it, the
/// same wrinkle as the other helpers here.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_true(value: &bool) -> bool {
  *value
}

/// A helper function to avoid serializing when zero.  It makes
/// the use of a reference a bit funny, but necessary.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_u32(value: &u32) -> bool {
  *value == 0
}

/// A helper function to avoid serializing when zero.  It makes
/// the use of a reference a bit funny, but necessary.
#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_false(value: &bool) -> bool {
  !value
}

#[allow(clippy::trivially_copy_pass_by_ref)]
fn is_zero_i16(value: &i16) -> bool {
  *value == 0
}

fn format_ship_template_name_only(value: &Arc<ShipDesignTemplate>, f: &mut Formatter<'_>) -> FmtResult {
  write!(f, "\"{}\"", value.name)
}

#[skip_serializing_none]
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ShipDesignTemplate {
  pub name: String,
  pub displacement: u32,
  pub hull: u32,
  pub armor: u32,
  pub maneuver: u8,
  pub jump: u8,
  pub power: u32,
  pub fuel: u32,
  pub crew: u32,
  pub sensors: Sensors,
  pub stealth: Option<Stealth>,
  pub countermeasures: Option<CounterMeasures>,
  pub computer: u32,
  /// The crew this ship flies with, for a design that is one particular ship
  /// rather than a class.
  ///
  /// HMS Executor is a single hull with a single crew; restating their skills
  /// in every scenario is how they drift, and they had -- three scenarios gave
  /// her three different crews. Gunnery makes the case on its own: it is
  /// index-aligned with `weapons` below, so a design whose armament changes
  /// silently misaligns every scenario's array onto the wrong mounts.
  ///
  /// A scenario that states its own `crew` still wins, so a wounded or
  /// replacement crew stays expressible. Left unset on class designs, where
  /// there is no such thing as "the" crew.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub crew_skills: Option<Crew>,
  pub weapons: Vec<Weapon>,
  /// Directed defensive systems (High Guard pp. 40-41).  Omitted from the wire
  /// when empty, so every design written before screens existed is unchanged.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub screens: Vec<ScreenType>,
  /// Powered systems that are not drives, sensors or guns: a Harrier's
  /// holographic hull, say. Omitted from the wire when empty.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub auxiliary: Vec<AuxiliarySystem>,
  /// The software the design is sold with. A scenario can add to it.
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub software: Vec<Software>,
  /// Jump Control Specialisation: Processing is +5 for Jump Control software
  /// only (Core Rulebook p. 180). The Type-S scout's Computer/5bis is how it
  /// runs Jump Control/2 on a Processing 5 machine.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub computer_bis: bool,
  /// Hardened against electromagnetic attack: immune to ion weapons.
  #[serde(default, skip_serializing_if = "std::ops::Not::not")]
  pub computer_fib: bool,
  pub tl: u8,
  /// Broad role used to group designs in the ship-design picker, e.g. "Trader",
  /// "Escort", "Small Craft".  Purely presentational; absent on older designs.
  pub role: Option<String>,
  /// Where the design came from, e.g. "High Guard", "Ships of the Reach".
  /// Used as a secondary grouping and shown in the design tooltip.
  pub source: Option<String>,
}

/// A High Guard weapon Advantage or Disadvantage (pp. 70-71).
///
/// These attach to the **weapon**, not the mount: the book fits a triple turret
/// with "long range, high yield pulse lasers x2, sandcaster", where only the
/// lasers are modified.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum WeaponModifier {
  /// DM+1 to all attack rolls.
  Accurate,
  /// DM-1 to all attack rolls.
  Inaccurate,
  /// Rolling damage, every `1` counts as `2`.  Not applicable to missiles or
  /// torpedoes.
  HighYield,
  /// Rolling damage, every `1` and `2` counts as `3`.  Not applicable to
  /// missiles or torpedoes.
  VeryHighYield,
  /// AP+2.  Lasers and particle weapons only.
  IntenseFocus,
  /// Range increased by one band, to a maximum of Very Long.
  LongRange,
  /// Critical hits on this weapon are one Severity lower.
  Resilient,
  /// Consumes 25% less Power.  Recorded but inert: Callisto does not model a
  /// weapon's power draw in play.
  EnergyEfficient,
  /// Consumes 30% more Power.  Recorded but inert, as above.
  EnergyInefficient,
  /// 10% less tonnage.  Recorded but inert: tonnage is not validated.
  SizeReduction,
  /// 25% more tonnage.  Recorded but inert, as above.
  IncreasedSize,
  /// DM+1 to repair attempts.  Recorded but inert.
  EasyToRepair,
}

impl From<WeaponModifier> for String {
  fn from(m: WeaponModifier) -> Self {
    match m {
      WeaponModifier::Accurate => "accurate".to_string(),
      WeaponModifier::Inaccurate => "inaccurate".to_string(),
      WeaponModifier::HighYield => "high yield".to_string(),
      WeaponModifier::VeryHighYield => "very high yield".to_string(),
      WeaponModifier::IntenseFocus => "intense focus".to_string(),
      WeaponModifier::LongRange => "long range".to_string(),
      WeaponModifier::Resilient => "resilient".to_string(),
      WeaponModifier::EnergyEfficient => "energy efficient".to_string(),
      WeaponModifier::EnergyInefficient => "energy inefficient".to_string(),
      WeaponModifier::SizeReduction => "size reduction".to_string(),
      WeaponModifier::IncreasedSize => "increased size".to_string(),
      WeaponModifier::EasyToRepair => "easy to repair".to_string(),
    }
  }
}

/// One gun inside a mount.
///
/// Modifiers live here rather than on the mount because the book fits a triple
/// turret with "long range, high yield pulse lasers x2, sandcaster" -- only the
/// lasers are modified.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Gun {
  pub kind: WeaponType,
  #[serde(default, skip_serializing_if = "Vec::is_empty")]
  pub modifiers: Vec<WeaponModifier>,
}

impl Gun {
  #[must_use]
  pub fn new(kind: WeaponType) -> Self {
    Gun {
      kind,
      modifiers: vec![],
    }
  }

  #[must_use]
  pub fn with_modifiers(kind: WeaponType, modifiers: Vec<WeaponModifier>) -> Self {
    Gun { kind, modifiers }
  }
}

/// A mount resolved down to the one weapon type it is firing.
///
/// A mixed turret may only use one type per round (Core Rulebook p. 166), so
/// everything downstream of that choice works on this rather than on the mount.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Firing<'a> {
  pub kind: WeaponType,
  pub mount: &'a WeaponMount,
  pub modifiers: &'a [WeaponModifier],
  /// Guns of this type in the mount, for the same-type damage bonus.
  pub count: u8,
  /// A gunner's cap on the salvo, when they chose to throw fewer than the
  /// mount holds. `None` is the whole salvo. Means nothing to direct fire.
  pub salvo_limit: Option<u16>,
}

/// One weapon mount and everything bolted into it.
///
/// This is the unit `weapon_id` addresses and the unit a gunner is assigned to,
/// which is why a mixed turret is one `Weapon` with several `Gun`s rather than
/// several `Weapon`s.  A turret holds one to three guns; every other mount holds
/// exactly one.
///
/// See `docs/mixed_turrets_design.md`.  The wire format still writes the older
/// `{kind, mount, modifiers}` shape whenever every gun matches, so existing
/// designs are unchanged -- see the `Serialize`/`Deserialize` impls below.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Weapon {
  pub mount: WeaponMount,
  pub guns: Vec<Gun>,
}

impl Weapon {
  /// A mount holding `count` identical guns.
  #[must_use]
  pub fn uniform(kind: WeaponType, mount: WeaponMount, count: u8) -> Self {
    Weapon {
      mount,
      guns: (0..count.max(1)).map(|_| Gun::new(kind)).collect(),
    }
  }

  /// A mount holding a single gun, for barbettes, bays and fixed mounts.
  #[must_use]
  pub fn single(kind: WeaponType, mount: WeaponMount) -> Self {
    Weapon {
      mount,
      guns: vec![Gun::new(kind)],
    }
  }

  /// True when every gun in this mount is the same type.
  ///
  /// A uniform mount fires all its guns together; a mixed one must choose a
  /// type each round (Core Rulebook p. 166).
  #[must_use]
  pub fn is_uniform(&self) -> bool {
    self.guns.windows(2).all(|pair| pair[0].kind == pair[1].kind)
  }

  /// The distinct weapon types in this mount, in first-appearance order.
  #[must_use]
  pub fn kinds(&self) -> Vec<WeaponType> {
    let mut seen: Vec<WeaponType> = Vec::new();
    for gun in &self.guns {
      if !seen.contains(&gun.kind) {
        seen.push(gun.kind);
      }
    }
    seen
  }

  /// How many guns of `kind` this mount holds.
  ///
  /// This is the count the same-type damage bonus and the sandcaster and
  /// point-defence tallies all want -- not the size of the turret, which in a
  /// mixed mount overstates every one of them.
  #[must_use]
  pub fn count_of(&self, kind: WeaponType) -> u8 {
    u8::try_from(self.guns.iter().filter(|gun| gun.kind == kind).count()).unwrap_or(u8::MAX)
  }

  /// Whether this mount carries any gun of `kind`.
  #[must_use]
  pub fn has_kind(&self, kind: WeaponType) -> bool {
    self.guns.iter().any(|gun| gun.kind == kind)
  }

  /// Resolve this mount for firing a particular weapon type.
  ///
  /// `None` when the mount carries no gun of that type.  The `count` is the
  /// number of guns of that type -- not the size of the turret -- which is what
  /// the same-type damage bonus wants, and the distinction only matters once a
  /// turret can hold different weapons.
  #[must_use]
  pub fn firing(&self, kind: WeaponType) -> Option<Firing<'_>> {
    let count = self.count_of(kind);
    if count == 0 {
      return None;
    }
    let modifiers = self
      .guns
      .iter()
      .find(|gun| gun.kind == kind)
      .map_or(&[] as &[WeaponModifier], |gun| gun.modifiers.as_slice());
    Some(Firing {
      kind,
      mount: &self.mount,
      modifiers,
      count,
      salvo_limit: None,
    })
  }

  /// Resolve a uniform mount, or the first gun of a mixed one.
  #[must_use]
  pub fn firing_default(&self) -> Option<Firing<'_>> {
    self.guns.first().and_then(|gun| self.firing(gun.kind))
  }

  /// The type a uniform mount fires, or the first gun's type otherwise.
  ///
  /// Callers that must handle a mixed mount correctly should use [`kinds`] or
  /// [`count_of`]; this exists for logging and display.
  #[must_use]
  pub fn primary_kind(&self) -> WeaponType {
    self.guns.first().map_or(WeaponType::Beam, |gun| gun.kind)
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum WeaponMount {
  /// A turret holds one to three guns; the count lives in `Weapon::guns`
  /// rather than here, so the two cannot disagree.
  Turret,
  Barbette,
  Bay(BaySize),
  /// A single weapon bolted to the hull.  Unlike a turret it cannot traverse,
  /// so it fires along the thrust vector only and cannot serve as point defense.
  FixedMount,
  /// A 20-ton point-defence battery consuming one Hardpoint.  The `u8` is the
  /// book's Type -- 1, 2 or 3 (High Guard p. 40) -- which sets its Intercept.
  ///
  /// The grade lives here rather than on [`WeaponType`] so that a second family
  /// of batteries (the book also sells gauss ones) is a single new `WeaponType`
  /// reusing these same mounts.
  Battery(u8),
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum BaySize {
  Small,
  Medium,
  Large,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter)]
pub enum WeaponType {
  Beam = 0,
  Pulse,
  Missile,
  Sand,
  Particle,
  // Everything below was added after the fact.  Variants are appended rather
  // than sorted into place because scenario and design JSON round-trips these
  // by name, and because the older entries above are load-bearing in saved
  // games.  Order carries no meaning.
  Torpedo,
  Fusion,
  Plasma,
  Railgun,
  Meson,
  MassDriver,
  Repulsor,
  /// An ion cannon.  Instead of damaging the hull it temporarily drains the
  /// target's Power, disabling rather than destroying (High Guard p. 30).
  Ion,
  /// A point-defence laser battery.  Never fires offensively and never takes an
  /// attack roll: it is a passive sink that deletes incoming missiles.  Its
  /// Intercept grade lives on [`WeaponMount::Battery`].
  PointDefense,
}

/// A weapon mount with the turret count erased.
///
/// Turret size scales the *number of guns*, never the damage multiple, so every
/// `Turret(n)` shares one profile.  `FixedMount` is our own concept rather than
/// the book's — High Guard treats a fixed mount as a turret that cannot
/// traverse — so it resolves to the same profiles a turret gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, EnumIter)]
pub enum MountClass {
  Turret,
  Fixed,
  Barbette,
  SmallBay,
  MediumBay,
  LargeBay,
  /// Point-defence batteries.  Unlike every other class this is not a size --
  /// all batteries are 20 tons -- but it has to be distinct so the profile
  /// table can refuse to put a gun in one.
  Battery,
}

impl From<&WeaponMount> for MountClass {
  fn from(mount: &WeaponMount) -> Self {
    match mount {
      WeaponMount::Turret => MountClass::Turret,
      WeaponMount::FixedMount => MountClass::Fixed,
      WeaponMount::Barbette => MountClass::Barbette,
      WeaponMount::Bay(BaySize::Small) => MountClass::SmallBay,
      WeaponMount::Bay(BaySize::Medium) => MountClass::MediumBay,
      WeaponMount::Bay(BaySize::Large) => MountClass::LargeBay,
      WeaponMount::Battery(_) => MountClass::Battery,
    }
  }
}

/// How many objects a launcher throws per attack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Salvo {
  /// One per gun in the mount, i.e. `Turret(n)` launches `n`.
  PerGun,
  /// A large missile bay throws 120, so this does not fit in a `u8`.
  Fixed(u16),
}

/// Everything about a weapon that depends on how it is mounted.
///
/// A weapon scales its output in exactly one of two ways, never both: direct-fire
/// weapons multiply damage by the mount's Damage Multiple (High Guard p. 29),
/// while launchers throw a bigger salvo and take no multiple at all. That
/// invariant is why `use_multiple` and `salvo` are always opposites in the
/// table below.
// The bools are the book's weapon traits, which are genuinely independent of
// one another -- a weapon can be any combination of them.  Bundling them into
// a flags type would obscure the mapping to the printed tables without making
// any illegal state unrepresentable.
#[allow(clippy::struct_excessive_bools)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeaponProfile {
  /// Tech level of the weapon itself, which drives the Smart DM.
  pub tl: u8,
  pub damage_dice: u8,
  pub hit_mod: i32,
  /// The longest band this reaches; `None` is the book's "Special", meaning
  /// range never rules the shot out.
  pub max_range: Option<Range>,
  /// Armour ignored before damage is reduced. [`AP_INFINITE`] ignores all of it.
  pub ap: u8,
  pub radiation: bool,
  /// Smart rounds add their TL minus the target's, clamped to +1..=+6
  /// (Core Rulebook p. 79).
  pub smart: bool,
  pub use_multiple: bool,
  /// `None` for direct-fire weapons.
  pub salvo: Option<Salvo>,
  /// Damage suppresses the target's Power instead of harming its hull
  /// (High Guard p. 30).  Nothing is permanently destroyed.
  pub ion: bool,
}

// --- Weapon wire format ---------------------------------------------------
//
// A `Weapon` used to be one gun that knew its mount: `{kind, mount: {Turret: 3},
// modifiers}` meant a triple turret of three identical guns.  It is now a mount
// holding a list of guns, so that a turret can hold different ones.
//
// Both spellings are read.  The old one is still *written* whenever every gun in
// a mount matches, which is every design in the library, so this change leaves
// those files byte-identical and only a genuinely mixed turret gets new syntax.

/// The mount as it appears on the wire, where a turret still carries its size.
#[derive(Serialize, Deserialize)]
enum MountWire {
  Turret(u8),
  Barbette,
  Bay(BaySize),
  FixedMount,
  Battery(u8),
}

impl MountWire {
  fn to_mount(&self) -> WeaponMount {
    match self {
      MountWire::Turret(_) => WeaponMount::Turret,
      MountWire::Barbette => WeaponMount::Barbette,
      MountWire::Bay(size) => WeaponMount::Bay(*size),
      MountWire::FixedMount => WeaponMount::FixedMount,
      MountWire::Battery(grade) => WeaponMount::Battery(*grade),
    }
  }

  fn from_mount(mount: &WeaponMount, guns: usize) -> Self {
    match mount {
      WeaponMount::Turret => MountWire::Turret(u8::try_from(guns).unwrap_or(1)),
      WeaponMount::Barbette => MountWire::Barbette,
      WeaponMount::Bay(size) => MountWire::Bay(*size),
      WeaponMount::FixedMount => MountWire::FixedMount,
      WeaponMount::Battery(grade) => MountWire::Battery(*grade),
    }
  }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum WeaponWire {
  /// The shape every existing design file uses: one kind, and a turret size.
  Uniform {
    kind: WeaponType,
    mount: MountWire,
    #[serde(default)]
    modifiers: Vec<WeaponModifier>,
  },
  /// A mount listing its guns, needed only when they differ.
  Guns { mount: MountWire, guns: Vec<Gun> },
}

impl<'de> Deserialize<'de> for Weapon {
  fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
    Ok(match WeaponWire::deserialize(deserializer)? {
      WeaponWire::Uniform { kind, mount, modifiers } => {
        // A turret's size becomes that many identical guns; everything else
        // holds exactly one.
        let count = match mount {
          MountWire::Turret(size) => size.max(1),
          _ => 1,
        };
        Weapon {
          mount: mount.to_mount(),
          guns: (0..count)
            .map(|_| Gun {
              kind,
              modifiers: modifiers.clone(),
            })
            .collect(),
        }
      }
      WeaponWire::Guns { mount, guns } => Weapon {
        mount: mount.to_mount(),
        guns,
      },
    })
  }
}

impl Serialize for Weapon {
  fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
    let mount = MountWire::from_mount(&self.mount, self.guns.len());
    if self.is_uniform() {
      // Write the older shape so existing designs round-trip unchanged.
      let first = self.guns.first();
      WeaponUniformOut {
        kind: first.map_or(WeaponType::Beam, |gun| gun.kind),
        mount,
        modifiers: first.map(|gun| gun.modifiers.clone()).unwrap_or_default(),
      }
      .serialize(serializer)
    } else {
      WeaponGunsOut {
        mount,
        guns: &self.guns,
      }
      .serialize(serializer)
    }
  }
}

#[derive(Serialize)]
struct WeaponUniformOut {
  kind: WeaponType,
  mount: MountWire,
  #[serde(skip_serializing_if = "Vec::is_empty")]
  modifiers: Vec<WeaponModifier>,
}

#[derive(Serialize)]
struct WeaponGunsOut<'a> {
  mount: MountWire,
  guns: &'a Vec<Gun>,
}

/// A directed defensive system that reduces the damage of a specific kind of
/// attack (High Guard pp. 40-41).
///
/// Screens are not weapons: they have no mount, consume no Hardpoint, never
/// fire, and cannot be aimed.  They live in their own list rather than in
/// `Ship::weapons()` for exactly that reason.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum ScreenType {
  /// Reduces meson weapon damage by 2D x 10.
  Meson,
  /// Reduces fusion and nuclear-warhead damage by 2D.
  NuclearDamper,
}

impl ScreenType {
  /// Damage dice this screen rolls, and the factor its roll is multiplied by.
  ///
  /// The meson screen's x10 is part of its stated reduction, not a separate
  /// step: "a successful use of a meson screen reduces the damage of a meson
  /// weapon by 2D x 10" (High Guard p. 41).
  #[must_use]
  pub const fn reduction_dice(self) -> (u8, u32) {
    match self {
      ScreenType::Meson => (2, 10),
      ScreenType::NuclearDamper => (2, 1),
    }
  }

  /// Whether this screen defends against a given weapon.
  ///
  /// Screens are strictly type-specific: a meson screen does nothing against a
  /// fusion gun and a nuclear damper does nothing against a meson gun.
  #[must_use]
  pub fn defends_against(self, kind: WeaponType) -> bool {
    match self {
      ScreenType::Meson => kind == WeaponType::Meson,
      // The book also covers nuclear warheads, which Callisto does not model.
      ScreenType::NuclearDamper => kind == WeaponType::Fusion,
    }
  }
}

impl From<ScreenType> for String {
  fn from(s: ScreenType) -> Self {
    match s {
      ScreenType::Meson => "meson screen".to_string(),
      ScreenType::NuclearDamper => "nuclear damper".to_string(),
    }
  }
}

/// Meson guns ignore armour entirely (the book writes this as "AP ∞").
pub const AP_INFINITE: u8 = u8::MAX;

impl WeaponProfile {
  /// A direct-fire weapon: takes the mount's Damage Multiple, throws nothing.
  #[must_use]
  pub const fn gun(tl: u8, damage_dice: u8, max_range: Range) -> Self {
    Self {
      tl,
      damage_dice,
      hit_mod: 0,
      max_range: Some(max_range),
      ap: 0,
      radiation: false,
      smart: false,
      use_multiple: true,
      salvo: None,
      ion: false,
    }
  }

  /// A launcher: throws `salvo` objects that resolve on impact.  Range is
  /// "Special" and the Damage Multiple never applies — salvo size is the
  /// scaling instead.
  #[must_use]
  pub const fn launcher(tl: u8, damage_dice: u8, salvo: Salvo) -> Self {
    Self {
      tl,
      damage_dice,
      hit_mod: 0,
      max_range: None,
      ap: 0,
      radiation: false,
      smart: true,
      use_multiple: false,
      salvo: Some(salvo),
      ion: false,
    }
  }

  /// A weapon whose damage the rules leave as "Special", so it rolls nothing.
  #[must_use]
  pub const fn special(tl: u8, max_range: Range) -> Self {
    Self {
      damage_dice: 0,
      use_multiple: false,
      ..Self::gun(tl, 0, max_range)
    }
  }

  #[must_use]
  pub const fn hit(mut self, hit_mod: i32) -> Self {
    self.hit_mod = hit_mod;
    self
  }

  #[must_use]
  pub const fn ap(mut self, ap: u8) -> Self {
    self.ap = ap;
    self
  }

  #[must_use]
  pub const fn rad(mut self) -> Self {
    self.radiation = true;
    self
  }

  /// Mark this as an ion weapon: it drains Power rather than damaging the hull,
  /// and ignores armour entirely while doing so.
  #[must_use]
  pub const fn ion(mut self) -> Self {
    self.ion = true;
    self.ap = AP_INFINITE;
    self
  }

  /// Apply a weapon's Advantages and Disadvantages to its profile.
  ///
  /// Only the ones that change how a weapon *fires* are handled here; the rest
  /// are recorded on the weapon but inert (power draw and tonnage are not
  /// modelled).  Modifiers the rules forbid for this weapon are ignored rather
  /// than rejected, so a hand-edited design still loads.
  #[must_use]
  pub fn with_modifiers(mut self, kind: WeaponType, modifiers: &[WeaponModifier]) -> Self {
    for modifier in modifiers {
      match modifier {
        WeaponModifier::Accurate => self.hit_mod += 1,
        WeaponModifier::Inaccurate => self.hit_mod -= 1,
        // "Intense Focus can only be applied to lasers and particle weapons."
        WeaponModifier::IntenseFocus if kind.is_laser() || kind == WeaponType::Particle => {
          self.ap = self.ap.saturating_add(2);
        }
        // "The range for the weapon is increased by one band, to a maximum of
        // Very Long."  A launcher has no range band to raise.
        WeaponModifier::LongRange => {
          self.max_range = self.max_range.map(Range::one_band_further);
        }
        // Yield changes the dice themselves; see `min_die`.
        _ => {}
      }
    }
    self
  }

  /// The lowest value any damage die may show, after High Yield.
  ///
  /// "When rolling damage for a High Yield weapon ... any '1's rolled are
  /// counted as '2's", and Very High Yield counts '1's and '2's as '3's
  /// (High Guard p. 71).  Neither applies to missiles or torpedoes.
  #[must_use]
  pub fn min_die(kind: WeaponType, modifiers: &[WeaponModifier]) -> u8 {
    if matches!(kind, WeaponType::Missile | WeaponType::Torpedo) {
      return 1;
    }
    modifiers
      .iter()
      .map(|modifier| match modifier {
        WeaponModifier::VeryHighYield => 3,
        WeaponModifier::HighYield => 2,
        _ => 1,
      })
      .max()
      .unwrap_or(1)
  }

  /// Whether this weapon may be fired at `range`.
  #[must_use]
  pub fn reaches(&self, range: Range) -> bool {
    self.max_range.is_none_or(|max| range <= max)
  }
}

#[derive(Serialize, Deserialize, Debug, Default, Clone, Copy, PartialEq, PartialOrd, FromRepr)]
pub enum Sensors {
  Basic = 0,
  #[default]
  Civilian,
  Military,
  Improved,
  Advanced,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, FromRepr)]
pub enum Range {
  Short = 0,
  Medium,
  Long,
  VeryLong,
  Distant,
}

impl Range {
  /// The next range band out, saturating at Very Long.
  ///
  /// Distant is deliberately not reachable: "the range for the weapon is
  /// increased by one band, to a maximum of Very Long" (High Guard p. 71).
  #[must_use]
  pub const fn one_band_further(self) -> Self {
    match self {
      Range::Short => Range::Medium,
      Range::Medium => Range::Long,
      Range::Long | Range::VeryLong | Range::Distant => Range::VeryLong,
    }
  }
}

impl Display for Range {
  /// Written the way the book writes them. Was the derived `Debug`, which
  /// reached players as the identifier `VeryLong` wherever a band is named.
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let name = match self {
      Range::Short => "Short",
      Range::Medium => "Medium",
      Range::Long => "Long",
      Range::VeryLong => "Very Long",
      Range::Distant => "Distant",
    };
    write!(f, "{name}")
  }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum Stealth {
  Basic,
  Improved,
  Enhanced,
  Advanced,
}

/// Which side a ship is on.
///
/// Capped at four, and named for colours rather than numbers because the whole
/// point is that the display codes them: a referee reading "Red" on a dropdown
/// and seeing a red ship in the view needs no translation step. `None` means
/// unaligned, which is how every ship built before teams existed arrives.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Team {
  Red,
  Blue,
  Green,
  Gold,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum CounterMeasures {
  Standard,
  Military,
}

#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, FromRepr, Deserialize, Serialize)]
pub enum ShipSystem {
  Sensors = 0,
  Powerplant,
  Fuel,
  Weapon,
  Armor,
  Hull,
  Maneuver,
  Cargo,
  Jump,
  Crew,
  Bridge,
}

/// A station on the bridge that a bridge critical hit can knock out.
///
/// The Core Rulebook (p. 170) says "random bridge station" and never lists them,
/// so this is our list. Numbered from zero here, one to six on the die. Gunners
/// are not on it: High Guard p. 91 has gunnery control dispersed through a ship,
/// and a ship "with its bridge destroyed can still be lethal as long as its
/// guns keep firing" -- but the fire control that directs them is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromRepr, EnumIter, Deserialize, Serialize)]
pub enum BridgeStation {
  /// Transmitting and sensor hand-off.
  Comms = 0,
  /// The sensop's actions: locks, breaking locks and jamming.
  Sensors,
  /// Acceleration, jump and hand-off, which all need the computer.
  Computer,
  /// Jump.
  Astrogation,
  /// Every weapon, point defence included.
  FireControl,
  /// Acceleration, evasion and assisting the gunners.
  Pilot,
}

impl BridgeStation {
  pub const COUNT: usize = 6;

  /// Roll 1D for the station a hit lands on.
  #[must_use]
  pub fn random(rng: &mut dyn RngCore) -> BridgeStation {
    BridgeStation::from_repr(usize::from(crate::combat::roll(rng) - 1)).unwrap_or(BridgeStation::Comms)
  }

  /// What losing this station stops, for the crit message.
  #[must_use]
  pub fn loses(self) -> &'static str {
    match self {
      BridgeStation::Comms => "no transmitting or sensor hand-off",
      BridgeStation::Sensors => "no sensor locks or jamming",
      BridgeStation::Computer => "no acceleration, jump or sensor hand-off",
      BridgeStation::Astrogation => "no jump",
      BridgeStation::FireControl => "no weapons fire or point defence",
      BridgeStation::Pilot => "no acceleration, evasion or assisting gunners",
    }
  }
}

impl Display for BridgeStation {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.write_str(match self {
      BridgeStation::Comms => "comms",
      BridgeStation::Sensors => "sensors",
      BridgeStation::Computer => "computer",
      BridgeStation::Astrogation => "astrogation",
      BridgeStation::FireControl => "fire control",
      BridgeStation::Pilot => "pilot",
    })
  }
}

/// A call on the ship's power plant.
///
/// The engineer can shut most of these down to free Power for something else
/// (Core Rulebook p. 171, Offline System). Basic ship systems cannot be shut
/// off, but High Guard p. 17 allows them to run at half in an emergency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
pub enum PowerSystem {
  /// Life support, gravity, heat and light: 20% of the hull's tonnage.
  Basic,
  Sensors,
  /// The manoeuvre drive, at the Thrust the drive is rated for.
  Maneuver,
  /// The jump drive, which only draws when the ship actually jumps.
  Jump,
  /// One weapon mount, by its index in the ship's armament.
  Weapon(usize),
  /// Anything else the hull carries that draws Power and can be switched: a
  /// Harrier's holographic hull, a cargo lifter, a research suite. Indexed
  /// into the design's `auxiliary` list.
  ///
  /// The ship's computer will join this table once computers do anything;
  /// it is a draw like any other and belongs in the budget beside these.
  Auxiliary(usize),
}

/// A powered system a design carries that is not a drive, a sensor suite or a
/// gun.
///
/// The Harrier class projects a holographic hull -- a false image of another
/// ship -- which costs 100 Power while it is running. It is off at the dock
/// and the engineer brings it up, so these default to off and a scenario can
/// say otherwise when it adds the ship.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct AuxiliarySystem {
  pub name: String,
  pub power: u32,
  /// Whether the ship starts with it running.
  #[serde(default)]
  pub default_on: bool,
}

/// One line of a ship's power budget.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PowerLine {
  pub system: PowerSystem,
  /// What to call it on a console.
  pub label: String,
  /// What it needs to run.
  pub draw: u32,
  /// What the plant can actually give it. Equal to `draw` on a healthy ship;
  /// less when the plant is damaged and the systems ahead of this one in the
  /// priority have taken what there is.
  pub received: u32,
  /// Whether the engineer has left it switched on.
  pub online: bool,
  /// Whether it only draws at the moment it is used, as the jump drive does.
  pub on_demand: bool,
}

impl PowerLine {
  /// Whether this system is actually running: switched on and fed.
  ///
  /// The drive is the exception and runs on whatever it is given, at reduced
  /// Thrust. Everything else needs its full draw or does nothing -- a sensor
  /// suite at half power is not half a sensor suite.
  #[must_use]
  pub fn powered(&self) -> bool {
    if !self.online {
      return false;
    }
    match self.system {
      PowerSystem::Maneuver => self.received > 0,
      _ => self.received >= self.draw,
    }
  }
}

/// Whether a bridge station can be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
pub enum StationStatus {
  #[default]
  Working,
  /// Out for the rest of the round it was hit in and all of the next. The
  /// count is end-of-round ticks left, so it starts at two.
  Disabled(u8),
  /// Out until an engineer repairs it.
  Destroyed,
}

/// One undoable piece of damage, recorded against the severity that did it so
/// a repair can take the most recent back off first.
///
/// A crit's effect is not a function of its severity alone -- a power plant
/// loses a tenth of its rating each time, a sensor suite drops a grade -- so
/// what was lost has to be written down when it is taken, or a repair has
/// nothing to give back.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub enum SystemDamage {
  /// Thrust lost.
  Thrust(u8),
  /// Power lost.
  Power(u32),
  /// Jump rating lost.
  Jump(u8),
  /// The sensor grade held before the hit knocked it down one.
  SensorGrade(Sensors),
  /// Points taken off the ship's attack DM.
  AttackDm(i32),
  /// Weapon mounts switched off, by index.
  WeaponsOff(Vec<usize>),
  /// A bridge station knocked out for a round or two.
  Disabled(BridgeStation),
  /// A bridge station destroyed, and what it was before, so undoing it cannot
  /// bring back a station an earlier hit had already destroyed.
  Destroyed { station: BridgeStation, was: StationStatus },
  /// Computer Bandwidth lost.
  Bandwidth(u32),
}

fn all_stations_working(stations: &[StationStatus; BridgeStation::COUNT]) -> bool {
  stations.iter().all(|status| *status == StationStatus::Working)
}

impl Ship {
  #[must_use]
  pub fn new(
    name: String, position: Vec3, velocity: Vec3, design: &Arc<ShipDesignTemplate>, crew: Option<Crew>,
    weapons: Option<Vec<Weapon>>,
  ) -> Self {
    let num_weapons = weapons.as_ref().map_or(design.weapons.len(), Vec::len);
    Ship {
      name,
      position,
      velocity,
      plan: FlightPlan::default(),
      design: design.clone(),
      weapons,
      current_hull: design.hull,
      current_armor: design.armor,
      current_power: design.power,
      current_maneuver: design.maneuver,
      current_jump: design.jump,
      current_fuel: design.fuel,
      current_crew: design.crew,
      current_sensors: design.sensors,
      current_computer: design.computer,
      active_weapons: vec![true; num_weapons],
      sensor_locks: vec![],
      contacts: vec![],
      active_sensors: true,
      transmitting: false,
      handoff_sensors: false,
      comms_jammed: false,
      team: None,
      crit_level: [0; 11],
      bridge_stations: [StationStatus::Working; BridgeStation::COUNT],
      damage_log: vec![],
      software: design.software.clone(),
      // Everything the ship owns is running when it arrives, as far as the
      // computer can manage: a crew does not undock with the fire control
      // switched off. What will not fit is left for the engineer to sort out.
      software_running: initial_running(design),
      // A hologram projector is not running when the ship undocks. Whatever
      // the design says starts off, starts off.
      offline: design
        .auxiliary
        .iter()
        .enumerate()
        .filter(|(_, aux)| !aux.default_on)
        .map(|(index, _)| PowerSystem::Auxiliary(index))
        .collect(),
      basic_power_halved: false,
      overload_drive_attempts: 0,
      overload_plant_attempts: 0,
      attack_dm: 0,
      crew: Some(crew.or_else(|| design.crew_skills.clone()).unwrap_or_default()),
      dodge_thrust: 0,
      dodge_spent: 0,
      assist_gunners: false,
      can_jump: false,
      temporary_maneuver: 0,
      temporary_maneuver_rounds: 0,
      temporary_power_multiplier: 1.0,
      temporary_power_rounds: 0,
      last_repair_component: None,
      repair_bonus: 0,
      engineer_action_taken: false,
      evade_boost_used: false,
      leadership_points: 0,
      leadership_rolled: false,
      point_defense_list: vec![],
      point_defense_pool: 0,
      screen_pool: vec![],
      ion_power_loss: 0,
      ion_rounds: 0,
    }
  }

  /// Reset every current value to what the design says, discarding damage.
  ///
  /// `fixup_current_values` raises a current value to the design's but never
  /// lowers it, so that loading an undamaged ship fills the blanks in. That is
  /// wrong when the *design itself* changes: re-pointing a ship at a smaller
  /// hull left it with the larger one's hull, thrust and sensors, and the ship
  /// went on flying at a rating its new design cannot reach.
  pub fn reset_current_values_to_design(&mut self) {
    self.current_hull = self.design.hull;
    self.current_armor = self.design.armor;
    self.current_power = self.design.power;
    self.current_maneuver = self.design.maneuver;
    self.current_jump = self.design.jump;
    self.current_fuel = self.design.fuel;
    self.current_crew = self.design.crew;
    self.current_sensors = self.design.sensors;
    self.current_computer = self.design.computer;
    self.resolve_crew();
    self.active_weapons = vec![true; self.weapons().len()];
    self.crit_level = [0; 11];
    self.bridge_stations = [StationStatus::Working; BridgeStation::COUNT];
    self.damage_log.clear();
    self.attack_dm = 0;
    self.dodge_thrust = 0;
    self.dodge_spent = 0;
  }

  pub fn fixup_current_values(&mut self) {
    self.current_hull = u32::max(self.current_hull, self.design.hull);
    self.current_armor = u32::max(self.current_armor, self.design.armor);
    self.current_power = u32::max(self.current_power, self.design.power);
    self.current_maneuver = u8::max(self.current_maneuver, self.design.maneuver);
    self.current_jump = u8::max(self.current_jump, self.design.jump);
    self.current_fuel = u32::max(self.current_fuel, self.design.fuel);
    self.current_crew = u32::max(self.current_crew, self.design.crew);
    self.current_sensors = Sensors::max(self.current_sensors, self.design.sensors);
    self.current_computer = u32::max(self.current_computer, self.design.computer);
    self.resolve_crew();
    self.active_weapons = vec![true; self.weapons().len()];
    self.crit_level = [0; 11];
    self.bridge_stations = [StationStatus::Working; BridgeStation::COUNT];
    self.damage_log.clear();
    self.attack_dm = 0;
    self.dodge_thrust = 0;
  }

  /// Set the flight plan for this ship.
  ///
  /// # Errors
  ///
  /// Returns 'Err' if the flight plan has an acceleration greater than the ship's capabilities at this point in time.
  pub fn set_flight_plan(&mut self, new_plan: &FlightPlan) -> Result<(), String> {
    // First validate the plan to make sure its legal.
    // Its legal as long as the magnitudes in the flight plan are less than the max of the maneuverability rating
    // and the powerplant rating.
    // We use the current maneuverability rating in case the ship took damage
    let max_accel = f64::from(self.max_acceleration()) * G;
    debug!(
      "(Ship.set_flight_plan) ship: {}, max_accel: {} new_plan: {:?} with magnitude on first accel of {}",
      self.name,
      max_accel,
      new_plan,
      new_plan.0 .0.magnitude()
    );
    if new_plan.0.in_limits(max_accel) {
      if let Some(second) = &new_plan.1 {
        if second.in_limits(max_accel) {
          self.plan = new_plan.clone();
          Ok(())
        } else {
          Err("Flight plan has second acceleration that exceeds max acceleration".to_string())
        }
      } else {
        self.plan = new_plan.clone();
        Ok(())
      }
    } else {
      Err("Flight plan has first acceleration that exceeds max acceleration".to_string())
    }
  }

  /// Helper function when you just want the current acceleration. Avoids having to take apart the flight plan
  /// outside this impl.
  #[must_use]
  pub fn get_acceleration(&self) -> Vec3 {
    self.plan.0 .0
  }

  #[must_use]
  pub fn max_acceleration(&self) -> u8 {
    // Weapons, sensors and basic systems are all drawing on the same plant,
    // so what is left after them is what the drive has to work with. An
    // engineer short of Power can shut something down to get thrust back.
    let power_limit = self.design.thrust_from_power(self.power_for_drive());
    let maneuver_limit = self.current_maneuver;

    // TODO: Remove this once using a match doesn't trigger the warning about attributes on expressions being experimental.
    #[allow(clippy::comparison_chain)]
    if power_limit == maneuver_limit {
      debug!(
        "(Ship.max_acceleration) Ship {} limited in max acceleration by both power and maneuver at {}.",
        self.name, power_limit
      );
    } else if power_limit > maneuver_limit {
      debug!(
        "(Ship.max_acceleration) Ship {} limited in max acceleration by maneuver {}.",
        self.name, maneuver_limit
      );
    } else {
      debug!(
        "(Ship.max_acceleration) Ship {} limited in max acceleration by power {}.",
        self.name, power_limit
      );
    }

    // Making a decision here that temporary_maneuver also gives you extra power somehow.  Otherwise its
    // not very useful.  Might want to undo that later.
    u8::min(power_limit, maneuver_limit).saturating_sub(self.dodge_thrust + u8::from(self.assist_gunners))
      + self.temporary_maneuver
  }

  #[must_use]
  pub fn get_current_hull_points(&self) -> u32 {
    self.current_hull
  }

  #[must_use]
  pub fn get_max_hull_points(&self) -> u32 {
    self.design.hull
  }

  pub fn set_hull_points(&mut self, new_hull: u32) {
    self.current_hull = new_hull;
  }

  #[must_use]
  pub fn get_current_armor(&self) -> u32 {
    self.current_armor
  }

  /// This ship's armament: its own if it was given one, otherwise its design's.
  #[must_use]
  pub fn weapons(&self) -> &[Weapon] {
    self.weapons.as_deref().unwrap_or(&self.design.weapons)
  }

  /// Replace this ship's armament.  `None` reverts it to the design's weapons.
  /// Callers must follow this with [`Ship::fixup_current_values`] so
  /// `active_weapons` matches the new list.
  pub fn set_weapons(&mut self, weapons: Option<Vec<Weapon>>) {
    self.weapons = weapons;
  }

  #[must_use]
  pub fn get_weapon(&self, weapon_id: usize) -> &Weapon {
    &self.weapons()[weapon_id]
  }

  #[must_use]
  /// The crew aboard. An unresolved crew reads as untrained rather than
  /// panicking; `fixup_current_values` resolves it on every load path.
  pub fn get_crew(&self) -> &Crew {
    static UNTRAINED: std::sync::LazyLock<Crew> = std::sync::LazyLock::new(Crew::new);
    self.crew.as_ref().unwrap_or(&UNTRAINED)
  }

  pub fn get_crew_mut(&mut self) -> &mut Crew {
    self.crew.get_or_insert_with(Crew::new)
  }

  pub fn set_crew(&mut self, crew: Crew) {
    self.crew = Some(crew);
  }

  /// Fill in the crew from the design when the scenario did not name one.
  ///
  /// Idempotent, and never overwrites: a scenario that states its own crew
  /// keeps it. Falls back to untrained so `crew` is `Some` from here on.
  fn resolve_crew(&mut self) {
    if self.crew.is_none() {
      self.crew = Some(self.design.crew_skills.clone().unwrap_or_default());
    }
  }

  pub fn enable_jump(&mut self) {
    self.can_jump = true;
  }

  /// Clear of gravity wells, with astrogation and the computer to plot it.
  #[must_use]
  pub fn can_jump(&self) -> bool {
    self.can_jump
      && self.station_working(BridgeStation::Astrogation)
      && self.station_working(BridgeStation::Computer)
      && self.jump_powered()
  }

  #[must_use]
  pub fn station_status(&self, station: BridgeStation) -> StationStatus {
    self.bridge_stations[station as usize]
  }

  #[must_use]
  pub fn station_working(&self, station: BridgeStation) -> bool {
    self.station_status(station) == StationStatus::Working
  }

  /// Knock a station out for the rest of this round and all of the next. A
  /// destroyed station stays destroyed.
  pub fn disable_station(&mut self, station: BridgeStation) {
    if self.station_status(station) != StationStatus::Destroyed {
      self.bridge_stations[station as usize] = StationStatus::Disabled(2);
    }
  }

  pub fn destroy_station(&mut self, station: BridgeStation) {
    self.bridge_stations[station as usize] = StationStatus::Destroyed;
  }

  /// Count disabled stations down at the end of a round.
  pub fn tick_bridge_stations(&mut self) {
    for status in &mut self.bridge_stations {
      if let StationStatus::Disabled(rounds) = status {
        *status = if *rounds <= 1 {
          StationStatus::Working
        } else {
          StationStatus::Disabled(*rounds - 1)
        };
      }
    }
  }

  /// Note what a hit at `level` took from `system`, so a repair can give it
  /// back.
  pub fn record_damage(&mut self, system: ShipSystem, level: u8, damage: SystemDamage) {
    self.damage_log.push((system, level, damage));
  }

  /// A bridge hit at `level` disables `station`, recorded for repair.
  pub fn bridge_hit_disable(&mut self, level: u8, station: BridgeStation) {
    self.record_damage(ShipSystem::Bridge, level, SystemDamage::Disabled(station));
    self.disable_station(station);
  }

  /// A bridge hit at `level` destroys `station`, recorded for repair.
  pub fn bridge_hit_destroy(&mut self, level: u8, station: BridgeStation) {
    let was = self.station_status(station);
    self.record_damage(ShipSystem::Bridge, level, SystemDamage::Destroyed { station, was });
    self.destroy_station(station);
  }

  /// A bridge hit at `level` cuts Bandwidth to `bandwidth`, recorded for repair.
  pub fn bridge_hit_bandwidth(&mut self, level: u8, bandwidth: u32) {
    let lost = self.current_computer.saturating_sub(bandwidth);
    self.record_damage(ShipSystem::Bridge, level, SystemDamage::Bandwidth(lost));
    self.current_computer = bandwidth;
  }

  /// Undo the damage a system took above severity `level`, most recent first,
  /// for a repair that has just brought it down to that level. Returns what
  /// came back, to tell the engineer.
  ///
  /// Nothing is capped above the design: a repair restores what a hit took,
  /// and cannot build a better ship than the yard did.
  pub fn undo_damage(&mut self, system: ShipSystem, level: u8) -> Vec<String> {
    // Newest first, and only this system's: another system's damage may have
    // landed in between, and is not this engineer's to undo.
    let mut undoing = Vec::new();
    let mut index = self.damage_log.len();
    while index > 0 {
      index -= 1;
      if self.damage_log[index].0 == system && self.damage_log[index].1 > level {
        undoing.push(self.damage_log.remove(index).2);
      }
    }

    let mut restored = Vec::new();
    for damage in undoing {
      match damage {
        SystemDamage::Thrust(lost) => {
          self.current_maneuver = (self.current_maneuver + lost).min(self.design.maneuver);
          restored.push(format!("thrust back to {}", self.current_maneuver));
        }
        SystemDamage::Power(lost) => {
          self.current_power = (self.current_power + lost).min(self.design.power);
          restored.push(format!("power back to {}", self.current_power));
        }
        SystemDamage::Jump(lost) => {
          self.current_jump = (self.current_jump + lost).min(self.design.jump);
          restored.push(format!("jump back to {}", self.current_jump));
        }
        SystemDamage::SensorGrade(was) => {
          self.current_sensors = Sensors::max(self.current_sensors, was);
          restored.push(format!("sensors back to {}", String::from(self.current_sensors)));
        }
        SystemDamage::AttackDm(lost) => {
          self.attack_dm += lost;
          restored.push("attack DM restored".to_string());
        }
        SystemDamage::WeaponsOff(indices) => {
          for index in indices {
            if let Some(active) = self.active_weapons.get_mut(index) {
              *active = true;
            }
          }
          restored.push("weapons back online".to_string());
        }
        SystemDamage::Disabled(station) => {
          if matches!(self.station_status(station), StationStatus::Disabled(_)) {
            self.bridge_stations[station as usize] = StationStatus::Working;
            restored.push(format!("{station} station back up"));
          }
        }
        // Back to working unless an earlier hit had already destroyed it. A
        // disable from before is not put back: it would have run out by now.
        SystemDamage::Destroyed { station, was } => {
          if was != StationStatus::Destroyed {
            self.bridge_stations[station as usize] = StationStatus::Working;
            restored.push(format!("{station} station working again"));
          }
        }
        SystemDamage::Bandwidth(lost) => {
          self.current_computer = (self.current_computer + lost).min(self.design.computer);
          restored.push(format!("computer Bandwidth back to {}", self.current_computer));
        }
      }
    }
    restored
  }

  /// Whether the flight plan can be flown: it takes both the pilot and the
  /// computer.
  #[must_use]
  pub fn can_accelerate(&self) -> bool {
    self.station_working(BridgeStation::Pilot) && self.station_working(BridgeStation::Computer)
  }

  /// The first station a sensor hand-off needs that is out, at either end:
  /// comms to pass the picture, and the computer to handle it.
  #[must_use]
  pub fn handoff_station_down(&self) -> Option<BridgeStation> {
    [BridgeStation::Comms, BridgeStation::Computer]
      .into_iter()
      .find(|station| !self.station_working(*station))
  }

  /// Whether the ship is actually radiating. The crew can want to, but not with
  /// the comms station out.
  #[must_use]
  pub fn is_transmitting(&self) -> bool {
    self.transmitting && self.station_working(BridgeStation::Comms)
  }

  /// Power the drive is actually getting.
  #[must_use]
  pub fn power_for_drive(&self) -> u32 {
    self
      .power_line(PowerSystem::Maneuver)
      .map_or(0, |line| if line.online { line.received } else { 0 })
  }

  /// The thrust this ship is applying, in whole G, for the detection tables.
  ///
  /// "Target is operating manoeuvre drive: +1 per Thrust". A flight plan may
  /// carry two accelerations with separate durations; the louder of the two is
  /// what a sensop notices, so the maximum magnitude is used rather than an
  /// average. Rounded down, so a ship drifting under 1G contributes nothing.
  #[must_use]
  pub fn thrust_in_g(&self) -> u8 {
    let first = self.plan.0 .0.magnitude();
    let second = self.plan.1.as_ref().map_or(0.0, |accel| accel.0.magnitude());
    let loudest = first.max(second);

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let g = (loudest / crate::entity::G).floor().clamp(0.0, f64::from(u8::MAX)) as u8;
    g
  }

  /// Total severity of the critical hits this ship has taken.
  ///
  /// "Stealthed target has been damaged and emits heat: +1 per Severity".
  /// Summed across every system, and cleared with the rest of `crit_level` on
  /// repair or reset.
  #[must_use]
  pub fn total_crit_severity(&self) -> u16 {
    self.crit_level.iter().map(|level| u16::from(*level)).sum()
  }

  /// Turn sensor hand-off on or off.
  ///
  /// Switching it on also switches transmitting on, because sharing contacts
  /// means broadcasting them. Switching it off leaves transmitting where it is:
  /// the crew may have wanted to be lit up anyway, and silently going quiet
  /// would be a surprise.
  pub fn set_handoff_sensors(&mut self, handoff: bool) {
    self.handoff_sensors = handoff;
    if handoff {
      self.transmitting = true;
    }
  }

  /// Whether this ship knows where `target` is.
  ///
  /// Ships on the same side always do. A squadron shares a plot as a matter of
  /// course — they launched together, they are in comms, and they are not
  /// hunting each other — so making a sensop roll to find your own wingman
  /// would be strange. This is not a sensor hand-off: hand-offs share contacts
  /// on *third parties*, and cost Bandwidth and an emission to do it.
  ///
  /// Otherwise it is a question of what the sensors have found.
  #[must_use]
  pub fn detects(&self, target: &Ship) -> bool {
    if let (Some(mine), Some(theirs)) = (self.team, target.team) {
      if mine == theirs {
        return true;
      }
    }
    self.contacts.iter().any(|name| name == target.get_name())
  }

  /// Set the ship's emissions, returning whether shutting down active sensors
  /// dropped any locks.
  ///
  /// `None` leaves a setting alone, so a caller can change one without knowing
  /// the other.
  ///
  /// Shutting them down drops every sensor lock this ship holds. A lock is
  /// deliberate, continuous illumination of a target - the Stealthed Ships
  /// table charges DM+2 for "sensor locks, electronic warfare or other
  /// deliberate use of active sensors" - so it cannot survive going quiet.
  /// Contacts are kept: High Guard p. 77 has detection "maintained under most
  /// circumstances" once established, and it is that asymmetry that makes going
  /// dark a real choice rather than a free one. House rule; RAW does not say.
  pub fn set_emissions(&mut self, active_sensors: Option<bool>, transmitting: Option<bool>) -> bool {
    if let Some(transmitting) = transmitting {
      // Sharing a sensor picture means transmitting one. The client greys the
      // control out while hand-off is on, but the rule is enforced here so a
      // hand-crafted request cannot produce a ship sharing contacts in silence.
      self.transmitting = transmitting || self.handoff_sensors;
    }

    let Some(active_sensors) = active_sensors else {
      return false;
    };

    let going_dark = self.active_sensors && !active_sensors;
    self.active_sensors = active_sensors;

    if going_dark && !self.sensor_locks.is_empty() {
      self.sensor_locks.clear();
      return true;
    }
    false
  }

  /// Set possible pilot actions for the next round. These include allocating thrust to dodging as
  /// well as allocating a single point of thrust to assist gunners.
  ///
  /// # Errors
  /// Returns 'Err' if there isn't enough thrust to perform these actions.
  pub fn set_pilot_actions(&mut self, thrust: Option<u8>, assist: Option<bool>) -> Result<(), InvalidThrustError> {
    let old_agility = self.dodge_thrust;
    let old_assist = self.assist_gunners;

    self.dodge_thrust = 0;
    self.assist_gunners = false;

    // First see if we can set the dodge thrust
    match thrust {
      Some(thrust) if thrust > self.max_acceleration() => {
        let old_max_acceleration = self.max_acceleration();
        warn!(
          "(Ship.set_agility_thrust) thrust {} exceeds max acceleration {}",
          thrust, old_max_acceleration
        );
        self.dodge_thrust = old_agility;
        self.assist_gunners = old_assist;
        Err(InvalidThrustError(format!(
          "Thrust {thrust} exceeds max acceleration {old_max_acceleration}."
        )))
      }
      _ => {
        if let Some(thrust) = thrust {
          self.dodge_thrust = thrust;
        }

        // Second see if we can accommodate assist gunner
        if let Some(assist) = assist {
          if assist && self.max_acceleration() < 1 {
            warn!("(Ship.set_agility_thrust) No thrust available to reserve for assisting gunners.");
            self.dodge_thrust = old_agility;
            Err(InvalidThrustError(
              "No thrust available to reserve for assisting gunners".to_string(),
            ))
          } else {
            self.assist_gunners = assist;
            Ok(())
          }
        } else {
          self.assist_gunners = false;
          Ok(())
        }
      }
    }
  }

  /// Spend one point of the pilot's dodge on an attack.
  pub fn decrement_dodge_thrust(&mut self) {
    if self.get_dodge_thrust() == 0 {
      warn!("(Ship.decrement_dodge_thrust) Attempting to decrement a 0 dodge thrust; should never happen.");
    }
    self.dodge_spent = self.dodge_spent.saturating_add(1);
  }

  /// Assisting the gunners takes a pilot at their station.
  #[must_use]
  pub fn get_assist_gunners(&self) -> bool {
    self.assist_gunners && self.station_working(BridgeStation::Pilot)
  }
  pub fn reset_pilot_actions(&mut self) {
    self.dodge_thrust = 0;
    self.dodge_spent = 0;
    self.assist_gunners = false;
  }

  /// Every call on the power plant, in the order a console should show them,
  /// with what each one needs and what it is actually getting.
  ///
  /// High Guard pp. 16-17 and the weapon tables: basic systems are 20% of the
  /// hull, the drives 10% of the hull per point of Thrust or jump number,
  /// sensors by grade, and each weapon mount what its guns draw.
  ///
  /// A plant that cannot meet all of it feeds systems in priority order. Life
  /// support comes first and the drive last, because the drive is the one
  /// thing that does something useful with a partial share: everything else
  /// either runs or does not.
  #[must_use]
  pub fn power_lines(&self) -> Vec<PowerLine> {
    let hull = self.design.displacement;
    let mut lines = vec![
      PowerLine {
        system: PowerSystem::Basic,
        label: if self.basic_power_halved {
          "Basic systems (half)".to_string()
        } else {
          "Basic systems".to_string()
        },
        draw: if self.basic_power_halved { hull / 10 } else { hull / 5 },
        received: 0,
        // The one thing that cannot be switched off, only turned down.
        online: true,
        on_demand: false,
      },
      PowerLine {
        system: PowerSystem::Sensors,
        label: format!("Sensors ({})", String::from(self.current_sensors)),
        draw: sensor_power(self.current_sensors),
        received: 0,
        online: self.is_online(PowerSystem::Sensors),
        on_demand: false,
      },
    ];
    for (index, weapon) in self.weapons().iter().enumerate() {
      let draw = weapon_mount_power(weapon);
      if draw == 0 {
        continue;
      }
      lines.push(PowerLine {
        system: PowerSystem::Weapon(index),
        label: String::from(weapon),
        draw,
        received: 0,
        online: self.is_online(PowerSystem::Weapon(index)) && self.active_weapons[index],
        on_demand: false,
      });
    }
    lines.push(PowerLine {
      system: PowerSystem::Maneuver,
      label: format!("M-drive (thrust {})", self.design.maneuver),
      draw: hull / 10 * u32::from(self.design.maneuver),
      received: 0,
      online: self.is_online(PowerSystem::Maneuver),
      on_demand: false,
    });
    for (index, aux) in self.design.auxiliary.iter().enumerate() {
      lines.push(PowerLine {
        system: PowerSystem::Auxiliary(index),
        label: aux.name.clone(),
        draw: aux.power,
        received: 0,
        online: self.is_online(PowerSystem::Auxiliary(index)),
        on_demand: false,
      });
    }
    if self.design.jump > 0 {
      lines.push(PowerLine {
        system: PowerSystem::Jump,
        label: format!("J-drive (jump {})", self.design.jump),
        draw: hull / 10 * u32::from(self.design.jump),
        received: 0,
        online: self.is_online(PowerSystem::Jump),
        // "This Power requirement is only needed when the ship actually
        // initiates a jump" (High Guard p. 16).
        on_demand: true,
      });
    }

    // Hand out what the plant makes, in order. A system that cannot have its
    // full draw gets nothing and the next one is still tried: a plant with 20
    // to spare can run the sensors even when it cannot run the drive.
    let mut remaining = self.available_power();
    for line in &mut lines {
      if !line.online || line.on_demand {
        continue;
      }
      if line.system == PowerSystem::Maneuver {
        // The drive takes what is left and flies at whatever that buys.
        line.received = remaining.min(line.draw);
      } else if remaining >= line.draw {
        line.received = line.draw;
      }
      remaining -= line.received;
    }
    lines
  }

  /// One system's line, for asking whether it is running.
  #[must_use]
  pub fn power_line(&self, system: PowerSystem) -> Option<PowerLine> {
    self.power_lines().into_iter().find(|line| line.system == system)
  }

  /// Whether a system is switched on and fed.
  ///
  /// A system with no line draws nothing -- a missile rack is a rack, and a
  /// Basic sensor suite is a pair of eyes -- so there is nothing to feed and
  /// nothing that can starve it.
  #[must_use]
  pub fn is_powered(&self, system: PowerSystem) -> bool {
    self.power_line(system).is_none_or(|line| line.powered())
  }

  /// Whether the engineer has left this system running.
  #[must_use]
  pub fn is_online(&self, system: PowerSystem) -> bool {
    !self.offline.contains(&system)
  }

  /// Power everything running asks for, leaving out the jump drive, which
  /// only draws as the ship jumps.
  #[must_use]
  pub fn power_demand(&self) -> u32 {
    self
      .power_lines()
      .iter()
      .filter(|line| line.online && !line.on_demand)
      .map(|line| line.draw)
      .sum()
  }

  /// The computer's Processing score, which is what Bandwidth is measured
  /// against.
  ///
  /// Takes the ship's current figure, so a computer hit knocks capacity out
  /// and the software it was running has to be shed.
  #[must_use]
  pub fn processing(&self) -> u32 {
    self.current_computer
  }

  /// Processing available to one package.
  ///
  /// A /bis computer is worth +5 for Jump Control alone (Core Rulebook
  /// p. 180): the Type-S scout's Computer/5bis runs Jump Control/2 at
  /// Bandwidth 10 and nothing else of that size.
  #[must_use]
  pub fn processing_for(&self, kind: SoftwareKind) -> u32 {
    if kind == SoftwareKind::JumpControl && self.design.computer_bis {
      self.processing() + 5
    } else {
      self.processing()
    }
  }

  /// Bandwidth the running software is using.
  #[must_use]
  pub fn bandwidth_used(&self) -> u32 {
    self.software_running.iter().map(Software::bandwidth).sum()
  }

  /// Whether a package is installed aboard.
  #[must_use]
  pub fn has_software(&self, kind: SoftwareKind) -> bool {
    self.software.iter().any(|package| package.kind == kind)
  }

  /// The level of a package that is currently running, if one is.
  ///
  /// This is the question every effect asks: not "does the ship own Evade"
  /// but "is Evade running right now, and at what level".
  #[must_use]
  pub fn running_level(&self, kind: SoftwareKind) -> Option<u8> {
    self
      .software_running
      .iter()
      .filter(|package| package.kind == kind)
      .map(|package| package.level)
      .max()
  }

  /// Whether this package could run on top of what is already running.
  #[must_use]
  pub fn can_run(&self, package: Software) -> bool {
    if self.software_running.contains(&package) {
      return true;
    }
    self.bandwidth_used() + package.bandwidth() <= self.processing_for(package.kind)
  }

  /// Start or stop a package. Returns whether the ship obeyed.
  ///
  /// Free software is always running and cannot be stopped; anything that
  /// would overrun the computer is refused.
  pub fn set_software_running(&mut self, package: Software, running: bool) -> bool {
    if !self.has_software(package.kind) {
      return false;
    }
    if package.always_running() {
      // Nothing to free, and nothing sensible to do with the request.
      return running;
    }
    if running {
      if !self.can_run(package) {
        return false;
      }
      if !self.software_running.contains(&package) {
        // One level of a package at a time: starting Evade/2 replaces Evade/1.
        self.software_running.retain(|running| running.kind != package.kind);
        self.software_running.push(package);
      }
    } else {
      self.software_running.retain(|running| *running != package);
    }
    true
  }

  /// Fuel one jump number costs: a tenth of the ship's tonnage, so a jump-2
  /// costs a fifth of the hull (High Guard p. 11).
  ///
  /// Tonnage, not hull points. A Scout/Courier is 100 tons with 40 hull
  /// points, and its jump-2 costs 20 tons of fuel, not 4.
  #[must_use]
  pub fn fuel_per_jump_number(&self) -> u32 {
    self.design.displacement / 10
  }

  /// What a full jump costs this ship at its current drive rating.
  #[must_use]
  pub fn fuel_for_full_jump(&self) -> u32 {
    self.fuel_per_jump_number() * u32::from(self.current_jump)
  }

  /// The furthest this ship can actually jump: what the drive is rated for,
  /// or what is in the tanks, whichever runs out first.
  #[must_use]
  pub fn jump_range_available(&self) -> u8 {
    let per = self.fuel_per_jump_number();
    if per == 0 {
      return 0;
    }
    let affordable = u8::try_from(self.current_fuel / per).unwrap_or(u8::MAX);
    affordable.min(self.current_jump)
  }

  /// Whether the sensors are running. A suite with no power finds nothing and
  /// locks onto nothing.
  #[must_use]
  pub fn sensors_powered(&self) -> bool {
    self.is_powered(PowerSystem::Sensors)
  }

  /// Whether this mount has the power to fire.
  #[must_use]
  pub fn weapon_powered(&self, index: usize) -> bool {
    self.is_powered(PowerSystem::Weapon(index))
  }

  /// Whether the jump drive is switched on and the plant could find its draw
  /// on top of everything else running.
  #[must_use]
  pub fn jump_powered(&self) -> bool {
    let Some(jump) = self.power_line(PowerSystem::Jump) else {
      return false;
    };
    let running: u32 = self
      .power_lines()
      .iter()
      .filter(|line| !line.on_demand)
      .map(|line| line.received)
      .sum();
    jump.online && self.available_power().saturating_sub(running) >= jump.draw
  }

  /// Power left over, or `None` when the ship is drawing more than it makes.
  #[must_use]
  pub fn power_spare(&self) -> Option<u32> {
    self.available_power().checked_sub(self.power_demand())
  }

  /// Switch a system off, or back on. Basic systems cannot be switched off --
  /// use [`Ship::set_basic_power_halved`] to turn them down instead.
  pub fn set_online(&mut self, system: PowerSystem, online: bool) {
    if system == PowerSystem::Basic {
      return;
    }
    self.offline.retain(|off| *off != system);
    if !online {
      self.offline.push(system);
    }
  }

  /// Run basic ship systems at half power, or back at full.
  pub fn set_basic_power_halved(&mut self, halved: bool) {
    self.basic_power_halved = halved;
  }

  /// Thrust left for evasion this round: what the pilot set aside, less what
  /// has already been dodged. None without a pilot at their station.
  #[must_use]
  pub fn get_dodge_thrust(&self) -> u8 {
    if self.station_working(BridgeStation::Pilot) {
      self.dodge_thrust.saturating_sub(self.dodge_spent)
    } else {
      0
    }
  }

  pub fn set_point_defense_list(&mut self, list: Vec<(usize, u16)>) {
    self.point_defense_list = list;
  }

  pub fn add_point_defense_pool(&mut self, pool: u32) {
    self.point_defense_pool += pool;
  }

  pub fn set_point_defense_pool(&mut self, pool: u32) {
    self.point_defense_pool = pool;
  }

  /// Spend `cost` pool points to stop one incoming object, if enough remain.
  ///
  /// A partial pool stops nothing: one point left will not half-destroy a
  /// torpedo, and that point stays available for a missile.
  pub fn take_interception(&mut self, cost: u32) -> bool {
    if self.point_defense_pool < cost {
      return false;
    }
    self.point_defense_pool -= cost;
    true
  }

  pub fn clear_point_defense(&mut self) {
    self.point_defense_list.clear();
    self.point_defense_pool = 0;
    self.screen_pool.clear();
  }

  pub fn set_screen_pool(&mut self, pool: Vec<u32>) {
    self.screen_pool = pool;
  }

  /// Spend screens against `damage` from a weapon of `kind`, returning what is
  /// left of it.
  ///
  /// Screens are spent whole and greedily: each one that defends against this
  /// weapon is applied in turn until the damage reaches zero, and whatever it
  /// does not need is lost with it.  The next attack starts from the next
  /// unspent screen.  The book instead lets a gunner pick their moment and
  /// concentrate every screen on one attack; we resolve attacks in sequence
  /// with nobody to ask, so this is the closest approximation available.
  pub fn apply_screens(&mut self, kind: WeaponType, damage: u32) -> u32 {
    if damage == 0 || self.screen_pool.is_empty() {
      return damage;
    }

    let screens = self.design.screens.clone();
    let mut remaining = damage;
    for (index, screen) in screens.iter().enumerate() {
      if remaining == 0 {
        break;
      }
      if !screen.defends_against(kind) {
        continue;
      }
      let Some(absorbed) = self.screen_pool.get_mut(index) else {
        continue;
      };
      if *absorbed == 0 {
        continue;
      }
      remaining = remaining.saturating_sub(*absorbed);
      // Spent whole: any excess beyond what this attack needed is wasted, as it
      // would be in the book where a screen is used against one attack.
      *absorbed = 0;
    }
    remaining
  }

  // Engineer action getters and setters
  #[must_use]
  pub fn get_temporary_maneuver(&self) -> u8 {
    self.temporary_maneuver
  }

  /// Grant the drive's overload for `rounds` rounds, starting with the next.
  pub fn set_temporary_maneuver(&mut self, value: u8, rounds: u8) {
    self.temporary_maneuver = value;
    self.temporary_maneuver_rounds = rounds;
  }

  /// Rounds of drive overload left, including the one being played.
  #[must_use]
  pub fn temporary_maneuver_rounds(&self) -> u8 {
    self.temporary_maneuver_rounds
  }

  /// Rounds of plant overload left.
  #[must_use]
  pub fn temporary_power_rounds(&self) -> u8 {
    self.temporary_power_rounds
  }

  #[must_use]
  pub fn get_temporary_power_multiplier(&self) -> f32 {
    self.temporary_power_multiplier
  }

  /// Grant the plant's overload for `rounds` rounds, starting with the next.
  pub fn set_temporary_power_multiplier(&mut self, value: f32, rounds: u8) {
    self.temporary_power_multiplier = value;
    self.temporary_power_rounds = rounds;
  }

  #[must_use]
  pub fn get_last_repair_component(&self) -> Option<ShipSystem> {
    self.last_repair_component
  }

  pub fn set_last_repair_component(&mut self, value: Option<ShipSystem>) {
    self.last_repair_component = value;
  }

  #[must_use]
  pub fn get_repair_bonus(&self) -> u8 {
    self.repair_bonus
  }

  pub fn set_repair_bonus(&mut self, value: u8) {
    self.repair_bonus = value;
  }

  /// Resets temporary bonuses from engineer overload actions and action tracking.
  pub fn reset_temporary_bonuses(&mut self) {
    // A new round brings the pilot's spare thrust back: "each point of unspent
    // Thrust will allow the spacecraft to attempt to dodge one attack" (CRB
    // p. 171), which is a fresh allowance every round. The order itself stands
    // until the pilot changes it.
    self.dodge_spent = 0;
    // An overload runs for as many rounds as the check's Effect bought, so
    // the round ending spends one of them rather than ending it outright.
    self.temporary_maneuver_rounds = self.temporary_maneuver_rounds.saturating_sub(1);
    if self.temporary_maneuver_rounds == 0 {
      self.temporary_maneuver = 0;
    }
    self.temporary_power_rounds = self.temporary_power_rounds.saturating_sub(1);
    if self.temporary_power_rounds == 0 {
      self.temporary_power_multiplier = 1.0;
    }
    self.engineer_action_taken = false;
    self.evade_boost_used = false;
    self.leadership_points = 0;
    self.leadership_rolled = false;
  }

  #[must_use]
  pub fn get_leadership_points(&self) -> i16 {
    self.leadership_points
  }

  pub fn set_leadership_points(&mut self, value: i16) {
    self.leadership_points = value;
    self.leadership_rolled = true;
  }

  #[must_use]
  pub fn has_leadership_rolled(&self) -> bool {
    self.leadership_rolled
  }

  /// Resets repair bonus tracking when engineer switches to a different component.
  pub fn reset_repair_bonus(&mut self) {
    self.repair_bonus = 0;
    self.last_repair_component = None;
  }

  #[must_use]
  pub fn has_engineer_action_taken(&self) -> bool {
    self.engineer_action_taken
  }

  pub fn set_engineer_action_taken(&mut self, value: bool) {
    self.engineer_action_taken = value;
  }

  #[must_use]
  pub fn has_evade_boost_used(&self) -> bool {
    self.evade_boost_used
  }

  pub fn set_evade_boost_used(&mut self, value: bool) {
    self.evade_boost_used = value;
  }

  /// Power actually available to run the ship, after any ion suppression.
  ///
  /// Everything that asks what the ship can currently do should read this
  /// rather than `current_power`, which is the undamaged-by-ion figure.
  #[must_use]
  pub fn available_power(&self) -> u32 {
    self.current_power.saturating_sub(self.ion_power_loss)
  }

  /// Suppress `amount` Power for `rounds` rounds.
  ///
  /// Hits stack: a ship caught by two ion cannons loses both, and the longer
  /// duration wins so the second hit cannot cut the first one short.
  pub fn apply_ion_damage(&mut self, amount: u32, rounds: u8) {
    self.ion_power_loss = self.ion_power_loss.saturating_add(amount);
    self.ion_rounds = self.ion_rounds.max(rounds);
  }

  /// Run the ion suppression down by one round, restoring the Power when it
  /// lapses.  Called once per round after actions resolve.
  pub fn tick_ion_recovery(&mut self) {
    if self.ion_rounds == 0 {
      self.ion_power_loss = 0;
      return;
    }
    self.ion_rounds -= 1;
    if self.ion_rounds == 0 {
      self.ion_power_loss = 0;
    }
  }

  /// Returns the effective power including temporary multiplier.
  #[must_use]
  #[allow(
    clippy::cast_sign_loss,
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss
  )]
  pub fn get_effective_power(&self) -> u32 {
    (self.available_power() as f32 * self.temporary_power_multiplier) as u32
  }
}

impl PartialOrd for Ship {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    self.name.partial_cmp(&other.name)
  }
}

impl Default for Ship {
  fn default() -> Self {
    let mut ship = Ship::new(
      "Default".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    ship.fixup_current_values();
    ship
  }
}

impl Entity for Ship {
  fn get_name(&self) -> &str {
    &self.name
  }

  fn set_name(&mut self, name: String) {
    self.name = name;
  }

  fn get_position(&self) -> Vec3 {
    self.position
  }

  fn set_position(&mut self, position: Vec3) {
    self.position = position;
  }

  fn get_velocity(&self) -> Vec3 {
    self.velocity
  }

  fn set_velocity(&mut self, velocity: Vec3) {
    self.velocity = velocity;
  }

  fn update(&mut self) -> Option<UpdateAction> {
    debug!("(Ship.update) Updating ship {:?}", self.name);

    // If our ship is blown up, just return that effect (no need to do anything else)
    if self.current_hull == 0 {
      debug!("(Ship.update) Ship {} is destroyed.", self.name);
      return Some(UpdateAction::ShipDestroyed);
    }

    if self.plan.empty() || !self.can_accelerate() {
      // Just move at current velocity. A ship that cannot fly its plan this
      // round keeps it, and picks it up again once the station is back.
      self.position += self.velocity * DELTA_TIME_F64;
      debug!(
        "(Ship.update) No acceleration for {}: move at velocity {:0.0?} for time {}, position now {:0.0?}",
        self.name, self.velocity, DELTA_TIME, self.position
      );
    } else {
      // Adjust time in case max acceleration has changed due to combat damage.  Note this might be simplistic and require a new plan but that is up
      // to the user to notice and fix.
      let max_thrust = f64::from(self.max_acceleration());
      self.plan.ensure_thrust_limit(max_thrust * G);
      let moves = self.plan.advance_time(DELTA_TIME);

      // left_over will be any time left after the last acceleration.  We apply last velocity only after the
      // last acceleration as otherwise the accelerations are applied back to back. i.e. when you take
      // your foot off the accelerator cruise at last velocity.
      let mut left_over = DELTA_TIME_F64;
      for ap in moves.iter() {
        let old_velocity: Vec3 = self.velocity;
        let (accel, duration) = ap.into();
        #[allow(clippy::cast_precision_loss)]
        let duration: f64 = duration as f64;
        self.velocity += accel * duration;
        self.position += (old_velocity + self.velocity) / 2.0 * duration;
        left_over = (left_over - duration).max(0.);

        debug!(
          "(Ship.update) Accelerate {} at {:0.3?} m/s^2 for time {}",
          self.name, accel, duration
        );
        debug!(
          "(Ship.update) For ship {}: New velocity: {:0.0?} New position: {:0.0?}",
          self.name, self.velocity, self.position
        );
      }

      // Don't compare just to zero as there will be round-off error.
      if left_over > 0.01 {
        self.position += self.velocity * left_over;

        debug!(
          "(Ship.update) {} cruises at {:0.3?} m/s for time {} to position {:0.0?}",
          self.name, self.velocity, left_over, self.position
        );
      }
    }

    None
  }
}

/*
impl PartialEq for Ship {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.position == other.position
            && self.velocity == other.velocity
            && self.plan == other.plan
    }
}
 */

serde_with::serde_conv!(
    pub TemplateNameOnly,
    Arc<ShipDesignTemplate>,
    |t: &Arc<ShipDesignTemplate>| t.name.clone(),
    |value: String| -> Result<_, String> {
        get_ship_template_for_deserialization(&value).map_or_else(
          || { error!("(Deserializing Ship) Could not find design {value}"); Err("Could not find design".to_string()) },
          |t| Ok(t.clone()) )
    }
);

enum ShipTemplateFileOutcome {
  // Boxed: a design is far larger than an error pair, and an enum is as big as
  // its largest variant however rare that variant's size is.
  Loaded(Box<ShipDesignTemplate>),
  ParseError(String, String),
  ReadError(String, String),
}

/// Load every ship design from a directory of per-design JSON files.
///
/// Each file in the directory must contain a single `ShipDesignTemplate`
/// JSON object (not an array).
///
/// Per-file failures are split:
/// * **Read errors** (network/auth/transient) cause the WHOLE load to fail,
///   which leaves the watcher's directory fingerprint un-advanced so the
///   next poll retries. This is the self-healing path for a cold-start
///   GCS metadata-server flake — without it, a transient auth blip during
///   startup can permanently wedge the registry until restart.
/// * **Parse errors** (permanent — malformed JSON, wrong shape) are logged
///   and skipped. Otherwise one corrupt file would block every reload
///   forever. Once the operator fixes/removes the file the directory
///   fingerprint changes and we naturally re-attempt.
///
/// # Errors
///
/// Returns `Err` if listing the directory fails OR if any per-file read
/// fails. Per-file parse errors are not propagated.
pub async fn load_ship_templates_from_dir(
  dir: &str,
) -> Result<HashMap<String, Arc<ShipDesignTemplate>>, Box<dyn std::error::Error>> {
  let entries = list_local_or_cloud_dir(dir).await?;
  let dir_normalized = dir.trim_end_matches('/').to_string();

  // Bounded fan-out: see `MAX_CONCURRENT_DIR_FILE_READS`. Results arrive out of
  // order, which is fine — they go straight into a `HashMap` keyed by name.
  let results = stream::iter(entries)
    .map(|entry| {
      let path = format!("{dir_normalized}/{entry}");
      async move {
        match read_local_or_cloud_file(&path).await {
          Ok(body) => match serde_json::from_slice::<ShipDesignTemplate>(&body) {
            Ok(template) => ShipTemplateFileOutcome::Loaded(Box::new(template)),
            Err(e) => ShipTemplateFileOutcome::ParseError(path, format!("parse error: {e}")),
          },
          Err(e) => ShipTemplateFileOutcome::ReadError(path, format!("read error: {e}")),
        }
      }
    })
    .buffer_unordered(MAX_CONCURRENT_DIR_FILE_READS)
    .collect::<Vec<_>>()
    .await;

  let mut table = HashMap::new();
  let mut read_errors: Vec<String> = Vec::new();
  for r in results {
    match r {
      ShipTemplateFileOutcome::Loaded(template) => {
        table.insert(template.name.clone(), Arc::new(*template));
      }
      ShipTemplateFileOutcome::ParseError(path, msg) => {
        warn!("(load_ship_templates_from_dir) Skipping {path}: {msg}");
      }
      ShipTemplateFileOutcome::ReadError(path, msg) => {
        read_errors.push(format!("{path}: {msg}"));
      }
    }
  }
  if !read_errors.is_empty() {
    return Err(
      format!(
        "Failed to read {} of {} ship-design file(s): {}",
        read_errors.len(),
        read_errors.len() + table.len(),
        read_errors.join("; ")
      )
      .into(),
    );
  }
  Ok(table)
}

/// Test support: serializes access to the process-wide `SHIP_TEMPLATES`
/// registry.
///
/// `config_test_ship_templates` replaces that registry wholesale, so a test
/// that installs a registry of its own and then asserts on it can have its
/// entries wiped by any other test re-seeding the registry concurrently. Such
/// a test holds this lock for its whole body (acquiring it with
/// `lock_ship_templates_for_test` and re-seeding via
/// `config_test_ship_templates_locked`); the ordinary re-seeding path only
/// takes it around its own write, which is enough to keep it out of the
/// exclusive window.
///
/// This is a `tokio::sync::Mutex` rather than a `std::sync::Mutex` both
/// because holders await while holding it and because it has no poisoning: a
/// test that panics releases the lock cleanly instead of cascading "poisoned
/// lock" failures across the rest of the suite.
static SHIP_TEMPLATE_TEST_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Take exclusive ownership of the global ship-template registry for the
/// duration of a test. Hold the returned guard for as long as the test depends
/// on the contents it installs. See [`SHIP_TEMPLATE_TEST_LOCK`].
pub async fn lock_ship_templates_for_test() -> tokio::sync::MutexGuard<'static, ()> {
  SHIP_TEMPLATE_TEST_LOCK.lock().await
}

/// Helper method that loads ship templates from the default templates
/// directory. Used by tests to seed the global registry from a known
/// good fixture.
///
/// # Panics
///
/// If the directory cannot be listed.
pub async fn config_test_ship_templates() {
  let templates = load_test_ship_templates().await;
  let _lock = SHIP_TEMPLATE_TEST_LOCK.lock().await;
  replace_ship_templates(templates);
}

/// Same as [`config_test_ship_templates`], for callers that already hold the
/// guard from [`lock_ship_templates_for_test`]. The lock is not reentrant, so
/// those callers must not take it a second time.
///
/// # Panics
///
/// If the directory cannot be listed.
pub async fn config_test_ship_templates_locked(_lock: &tokio::sync::MutexGuard<'static, ()>) {
  replace_ship_templates(load_test_ship_templates().await);
}

async fn load_test_ship_templates() -> ShipTemplateTable {
  load_ship_templates_from_dir(DEFAULT_SHIP_TEMPLATES_DIR)
    .await
    .expect("Unable to load ship templates directory.")
}

/// What a new ship has running: everything it owns that the computer can
/// manage, free software first, then the rest in the order the design lists.
fn initial_running(design: &ShipDesignTemplate) -> Vec<Software> {
  let mut running: Vec<Software> = design.software.iter().copied().filter(Software::always_running).collect();
  let mut used = 0;
  for package in design.software.iter().filter(|p| !p.always_running()) {
    let capacity = if package.kind == SoftwareKind::JumpControl && design.computer_bis {
      design.computer + 5
    } else {
      design.computer
    };
    if used + package.bandwidth() <= capacity {
      used += package.bandwidth();
      running.push(*package);
    }
  }
  running
}

/// What a sensor suite draws (High Guard p. 23).
#[must_use]
pub fn sensor_power(sensors: Sensors) -> u32 {
  match sensors {
    Sensors::Basic => 0,
    Sensors::Civilian => 1,
    Sensors::Military => 2,
    Sensors::Improved => 4,
    Sensors::Advanced => 6,
  }
}

/// What one mount draws with everything in it running: the mount itself, plus
/// each gun bolted into it.
#[must_use]
pub fn weapon_mount_power(weapon: &Weapon) -> u32 {
  let class = MountClass::from(&weapon.mount);
  let guns: u32 = weapon
    .guns
    .iter()
    .map(|gun| crate::rules_tables::weapon_power(gun.kind, class).unwrap_or(0))
    .sum();
  guns + crate::rules_tables::mount_power(class)
}

impl ShipDesignTemplate {
  /// The Thrust a given amount of Power will drive, capped by what the drive
  /// is rated for: 10% of the hull's tonnage per point of Thrust (High Guard
  /// p. 16).
  #[must_use]
  pub fn thrust_from_power(&self, power_for_drive: u32) -> u8 {
    if self.displacement == 0 {
      return self.maneuver;
    }
    (power_for_drive * 10 / self.displacement)
      .try_into()
      .unwrap_or(u8::MAX)
      .min(self.maneuver)
  }

  // Making this overly simplistic for now.  Assume for power usage that
  // basic systems and sensors are prioritized, and we ignore weapons.
  #[must_use]
  pub fn best_thrust(&self, current_power: u32) -> u8 {
    // First take out basic ship systems.
    let power = current_power.saturating_sub(self.displacement / 5);
    // Now adjust for sensors.  If we subtract all the power then we have no power left.
    let power = power.saturating_sub(match self.sensors {
      Sensors::Basic => 0,
      Sensors::Civilian => 1,
      Sensors::Military => 2,
      Sensors::Improved => 4,
      Sensors::Advanced => 6,
    });

    if power == 0 {
      return 0;
    }

    // Power left for thrust is one thrust per 10% of ship displacement in power units.
    // Displacement cannot be over 1M tons ever.
    // If somehow power is enough we are above u8::MAX then just use that as really thrust cannot be that high.
    (power * 10 / self.displacement)
      .try_into()
      .unwrap_or(u8::MAX)
      .min(self.maneuver)
  }
}

impl PartialOrd for Weapon {
  fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
    Some(self.cmp(other))
  }
}

impl Ord for Weapon {
  fn cmp(&self, other: &Self) -> std::cmp::Ordering {
    // Could write this more efficiently as many identical outcomes
    // but seems more readable when expanded.
    #[allow(clippy::match_same_arms)]
    match (&self.mount, &other.mount) {
      (WeaponMount::Bay(BaySize::Large), WeaponMount::Bay(BaySize::Large)) => self.kinds().cmp(&other.kinds()),
      (WeaponMount::Bay(BaySize::Large), _) => std::cmp::Ordering::Less,
      (WeaponMount::Bay(BaySize::Medium), WeaponMount::Bay(BaySize::Large)) => std::cmp::Ordering::Greater,
      (WeaponMount::Bay(BaySize::Medium), WeaponMount::Bay(BaySize::Medium)) => self.kinds().cmp(&other.kinds()),
      (WeaponMount::Bay(BaySize::Medium), _) => std::cmp::Ordering::Less,
      (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Large)) => std::cmp::Ordering::Greater,
      (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Medium)) => std::cmp::Ordering::Greater,
      (WeaponMount::Bay(BaySize::Small), WeaponMount::Bay(BaySize::Small)) => self.kinds().cmp(&other.kinds()),
      (WeaponMount::Bay(BaySize::Small), _) => std::cmp::Ordering::Less,
      (WeaponMount::Barbette, _) => std::cmp::Ordering::Less,
      (WeaponMount::Turret, WeaponMount::Bay(_)) => std::cmp::Ordering::Greater,
      (WeaponMount::Turret, WeaponMount::Barbette) => std::cmp::Ordering::Greater,
      (WeaponMount::Turret, WeaponMount::Turret) => self.kinds().cmp(&other.kinds()),
      // A fixed mount is the least capable mount, so it sorts after everything else.
      (WeaponMount::Turret, WeaponMount::FixedMount) => std::cmp::Ordering::Less,
      // A battery is real hardware but not a gun, so it sits between the
      // turrets and the fixed mounts.
      (WeaponMount::Turret, WeaponMount::Battery(_)) => std::cmp::Ordering::Less,
      (WeaponMount::Battery(_), WeaponMount::Battery(_)) => self.kinds().cmp(&other.kinds()),
      (WeaponMount::Battery(_), WeaponMount::FixedMount) => std::cmp::Ordering::Less,
      (WeaponMount::Battery(_), _) => std::cmp::Ordering::Greater,
      (WeaponMount::FixedMount, WeaponMount::FixedMount) => self.kinds().cmp(&other.kinds()),
      (WeaponMount::FixedMount, _) => std::cmp::Ordering::Greater,
    }
  }
}

impl Sensors {
  #[must_use]
  pub fn max(lhs: Sensors, rhs: Sensors) -> Sensors {
    if lhs > rhs {
      lhs
    } else {
      rhs
    }
  }
}
impl From<Sensors> for i32 {
  fn from(s: Sensors) -> Self {
    match s {
      Sensors::Basic => -4,
      Sensors::Civilian => -2,
      Sensors::Military => 0,
      Sensors::Improved => 1,
      Sensors::Advanced => 2,
    }
  }
}

impl From<Sensors> for String {
  fn from(s: Sensors) -> Self {
    match s {
      Sensors::Basic => "Basic".to_string(),
      Sensors::Civilian => "Civilian".to_string(),
      Sensors::Military => "Military".to_string(),
      Sensors::Improved => "Improved".to_string(),
      Sensors::Advanced => "Advanced".to_string(),
    }
  }
}

impl std::ops::Sub<i32> for Sensors {
  type Output = Sensors;

  fn sub(self, rhs: i32) -> Self::Output {
    // Know that this enum isn't so big this can overflow
    #[allow(clippy::cast_possible_wrap)]
    let int_rep = self as u32 as i32;
    if int_rep - rhs <= 0 {
      Sensors::Basic
    } else {
      // Because of the check above this can never go below 0 so is safe
      #[allow(clippy::cast_sign_loss)]
      Sensors::from_repr((int_rep - rhs) as usize).unwrap()
    }
  }
}

impl From<Stealth> for i32 {
  fn from(s: Stealth) -> Self {
    match s {
      Stealth::Basic | Stealth::Improved => -2,
      Stealth::Enhanced => -4,
      Stealth::Advanced => -6,
    }
  }
}

impl From<Stealth> for String {
  fn from(s: Stealth) -> Self {
    match s {
      Stealth::Basic => "Basic".to_string(),
      Stealth::Improved => "Improved".to_string(),
      Stealth::Enhanced => "Enhanced".to_string(),
      Stealth::Advanced => "Advanced".to_string(),
    }
  }
}

impl From<WeaponType> for String {
  fn from(w: WeaponType) -> Self {
    String::from(&w)
  }
}

impl From<&WeaponType> for String {
  fn from(w: &WeaponType) -> Self {
    match w {
      WeaponType::Beam => "beam laser".to_string(),
      WeaponType::Pulse => "pulse laser".to_string(),
      WeaponType::Missile => "missile".to_string(),
      WeaponType::Sand => "sand".to_string(),
      WeaponType::Particle => "particle beam".to_string(),
      WeaponType::Torpedo => "torpedo".to_string(),
      WeaponType::Fusion => "fusion gun".to_string(),
      WeaponType::Plasma => "plasma gun".to_string(),
      WeaponType::Railgun => "railgun".to_string(),
      WeaponType::Meson => "meson gun".to_string(),
      WeaponType::MassDriver => "mass driver".to_string(),
      WeaponType::Repulsor => "repulsor".to_string(),
      WeaponType::Ion => "ion cannon".to_string(),
      WeaponType::PointDefense => "point defence battery".to_string(),
    }
  }
}

impl From<&Weapon> for String {
  fn from(w: &Weapon) -> Self {
    // A mixed turret is named by its contents rather than by a single kind:
    // "pulse laser x2, sandcaster triple turret".
    let contents = if w.is_uniform() {
      String::from(&w.primary_kind())
    } else {
      w.kinds()
        .iter()
        .map(|kind| {
          let count = w.count_of(*kind);
          if count > 1 {
            format!("{} x{count}", String::from(kind))
          } else {
            String::from(kind)
          }
        })
        .collect::<Vec<_>>()
        .join(", ")
    };

    match &w.mount {
      WeaponMount::Turret => match w.guns.len() {
        1 => format!("{contents} single turret"),
        2 => format!("{contents} double turret"),
        3 => format!("{contents} triple turret"),
        size => format!("{contents} turret of {size}"),
      },
      WeaponMount::FixedMount => format!("{contents} fixed mount"),
      WeaponMount::Barbette => format!("{contents} barbette"),
      WeaponMount::Bay(BaySize::Small) => format!("{contents} small bay"),
      WeaponMount::Bay(BaySize::Medium) => format!("{contents} medium bay"),
      WeaponMount::Bay(BaySize::Large) => format!("{contents} large bay"),
      // The grade is the whole identity of a battery, so name it rather than
      // falling back on the weapon kind.
      WeaponMount::Battery(grade) => {
        let numeral = match grade {
          1 => "I",
          2 => "II",
          3 => "III",
          _ => "?",
        };
        format!("point defence battery (Type {numeral})")
      }
    }
  }
}

impl WeaponType {
  #[must_use]
  pub fn is_laser(&self) -> bool {
    matches!(self, WeaponType::Beam | WeaponType::Pulse)
  }

  // Range used to be a property of the weapon alone, but it is not: a railgun
  // reaches Short from a turret and Medium from a barbette, and a particle beam
  // only reaches Distant out of a large bay.  Ask the profile instead —
  // `WeaponProfile::reaches`.
}

impl From<ShipSystem> for String {
  fn from(s: ShipSystem) -> Self {
    match s {
      ShipSystem::Hull => "hull".to_string(),
      ShipSystem::Armor => "armor".to_string(),
      ShipSystem::Jump => "jump drive".to_string(),
      ShipSystem::Maneuver => "maneuver drive".to_string(),
      ShipSystem::Powerplant => "power plant".to_string(),
      ShipSystem::Crew => "crew".to_string(),
      ShipSystem::Weapon => "a weapon".to_string(),
      ShipSystem::Sensors => "sensors".to_string(),
      ShipSystem::Fuel => "fuel".to_string(),
      ShipSystem::Bridge => "bridge".to_string(),
      ShipSystem::Cargo => "cargo".to_string(),
    }
  }
}

#[derive(Debug)]
pub struct InvalidThrustError(String);

impl InvalidThrustError {
  #[must_use]
  pub fn get_msg(&self) -> String {
    self.0.clone()
  }
}

impl Error for InvalidThrustError {
  fn source(&self) -> Option<&(dyn Error + 'static)> {
    None
  }
}

impl std::fmt::Display for InvalidThrustError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "Invalid thrust attempted: {}", self.0)
  }
}

#[serde_as]
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AccelPair(#[serde_as(as = "Vec3asVec")] pub Vec3, pub u64);

impl From<(Vec3, u64)> for AccelPair {
  fn from(tuple: (Vec3, u64)) -> Self {
    AccelPair(tuple.0, tuple.1)
  }
}

impl From<AccelPair> for (Vec3, u64) {
  fn from(val: AccelPair) -> Self {
    (val.0, val.1)
  }
}

impl AccelPair {
  #[must_use]
  pub fn in_limits(&self, limit: f64) -> bool {
    self.0.magnitude() <= limit + MAX_ACCEL_WIGGLE_ROOM
      || approx::relative_eq!(&self.0.magnitude(), &limit, max_relative = 1e-3)
  }
}
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct FlightPlan(
  pub AccelPair,
  #[serde(
    default,
    skip_serializing_if = "Option::is_none",
    with = "::serde_with::rust::unwrap_or_skip"
  )]
  pub Option<AccelPair>,
);

impl From<Vec<(Vec3, u64)>> for FlightPlan {
  fn from(vec: Vec<(Vec3, u64)>) -> Self {
    match vec.len() {
      0 => FlightPlan::default(),
      1 => FlightPlan(vec[0].into(), None),
      2 => FlightPlan(vec[0].into(), Some(vec[1].into())),
      _ => panic!("(From<Vec<(Vec3, u64)>> for FlightPlan) illegal length of vector"),
    }
  }
}

#[must_use]
fn renormalize(orig: Vec3, limit: f64) -> Vec3 {
  orig / orig.magnitude() * limit
}
impl Default for FlightPlan {
  fn default() -> Self {
    FlightPlan(AccelPair(Vec3::zero(), DEFAULT_ACCEL_DURATION), None)
  }
}

impl FlightPlan {
  #[must_use]
  pub fn new(first: AccelPair, second: Option<AccelPair>) -> Self {
    FlightPlan(first, second)
  }

  // Constructor that creates a flight plan that just has a single acceleration.
  // We use i64::MAX to represent infinite time.
  #[must_use]
  pub fn acceleration(accel: Vec3) -> Self {
    FlightPlan((accel, DEFAULT_ACCEL_DURATION).into(), None)
  }

  // When the first element is set we clear the second element.
  pub fn set_first(&mut self, accel: Vec3, time: u64) {
    self.0 = (accel, time).into();
    self.1 = None;
  }

  pub fn set_second(&mut self, accel: Vec3, time: u64) {
    self.1 = Some((accel, time).into());
  }

  #[must_use]
  pub fn has_second(&self) -> bool {
    self.1.is_some()
  }

  #[must_use]
  pub fn duration(&self) -> u64 {
    self.0 .1 + self.1.as_ref().map_or(0, |a| a.1)
  }

  #[must_use]
  pub fn empty(&self) -> bool {
    self.0 .1 == 0 || self.0 .0 == Vec3::zero()
  }

  // Ensure the thrust limit on a flight plan. Limit is in m/s^2 (not G's)
  pub fn ensure_thrust_limit(&mut self, limit: f64) {
    if self.0 .0.magnitude() > limit + MAX_ACCEL_WIGGLE_ROOM {
      self.0 .0 = renormalize(self.0 .0, limit);
    }

    if let Some(second) = &self.1 {
      if second.0.magnitude() > limit + MAX_ACCEL_WIGGLE_ROOM {
        self.set_second(renormalize(second.0, limit), second.1);
      }
    }
  }

  /// Modify this plan by advancing time and adjusting it based on that time.
  /// i.e. the flight plan advances.
  /// Returns the portion of the plan that was advanced.
  ///
  /// # Arguments
  ///
  /// * `time` - The time to advance the plan.
  #[must_use]
  pub fn advance_time(&mut self, time: u64) -> Self {
    if time < self.0 .1 {
      // If time is less than the first duration:
      // This plan: first acceleration reduced by the time
      // Return: the first acceleration for time
      self.0 .1 -= time;
      FlightPlan::new((self.0 .0, time).into(), None)
    } else {
      match &self.1.clone() {
        Some(second) if time < self.0 .1 + second.1 => {
          // If time is between the first duration plus the second duration:
          // This plan: The second acceleration for the remaining time (duration of the entire plan less the time)
          // Return: The first acceleration for its full time, and the portion of the second acceleration up to time.
          let new_first = self.0.clone();
          let first_time = self.0 .1;
          self.0 = (second.0, second.1 - (time - self.0 .1)).into();
          self.1 = None;
          debug!(
            "(FlightPlan.advance_time) self: {:?} new_first: {:?} second: {:?} time: {} first_time: {}",
            self, new_first, second, time, first_time
          );
          FlightPlan::new(
            new_first,
            if time <= first_time {
              None
            } else {
              Some((second.0, time - first_time).into())
            },
          )
        }
        _ => {
          // If time is more than first and second durations:
          // This plan: becomes a zero acceleration plan.
          // Return: the entire plan.
          let result = self.clone();
          self.0 = (Vec3::zero(), 0).into();
          self.1 = None;
          result
        }
      }
    }
  }

  pub fn iter(&self) -> impl Iterator<Item = AccelPair> + '_ {
    if let Some(second) = &self.1 {
      vec![self.0.clone(), second.clone()].into_iter()
    } else {
      vec![self.0.clone()].into_iter()
    }
  }
}

impl Default for ShipDesignTemplate {
  fn default() -> Self {
    ShipDesignTemplate {
      crew_skills: None,
      name: "Buccaneer".to_string(),
      displacement: 400,
      hull: 160,
      armor: 5,
      maneuver: 3,
      jump: 2,
      power: 300,
      fuel: 81,
      crew: 11,
      sensors: Sensors::Improved,
      stealth: None,
      countermeasures: None,
      computer: 5,
      weapons: vec![
        Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 2),
        Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 2),
        Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 2),
        Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 2),
      ],
      screens: vec![],
      auxiliary: vec![],
      software: vec![],
      computer_bis: false,
      computer_fib: false,
      tl: 15,
      role: None,
      source: None,
    }
  }
}

#[allow(dead_code)]
fn digit_to_int(code: char) -> u8 {
  match code {
    '0' => 0,
    '1' => 1,
    '2' => 2,
    '3' => 3,
    '4' => 4,
    '5' => 5,
    '6' => 6,
    '7' => 7,
    '8' => 8,
    '9' => 9,
    'A' => 10,
    'B' => 11,
    'C' => 12,
    'D' => 13,
    'E' => 14,
    'F' => 15,
    'G' => 16,
    'H' => 17,
    'J' => 18,
    'K' => 19,
    'L' => 20,
    'M' => 21,
    'N' => 22,
    'P' => 23,
    'Q' => 24,
    'R' => 25,
    'S' => 26,
    'T' => 27,
    'U' => 28,
    'V' => 29,
    'W' => 30,
    'X' => 31,
    'Y' => 32,
    'Z' => 33,
    _ => panic!("(ship.digitToInt) Unknown code: {code}"),
  }
}

#[allow(dead_code)]
fn int_to_digit(code: u8) -> char {
  match code {
    x if x <= 9 => (x + b'0') as char,
    x if x <= 17 => (x - 10 + b'A') as char,
    x if x <= 22 => (x - 18 + b'J') as char,
    x if x <= 33 => (x - 23 + b'P') as char,
    _ => panic!("(ship.intToDigit) Unknown code: {code}"),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// The power budget, against the figures in High Guard: basic systems are
  /// 20% of the hull, the drives 10% per point, sensors by grade, and each
  /// mount what its guns draw.
  #[test]
  fn a_ships_power_budget_adds_up() {
    // Executor's shape: 200 tons, Thrust 6, Advanced sensors, a particle
    // barbette and a mixed missile/sand turret.
    let design = Arc::new(ShipDesignTemplate {
      name: "HMS Executor".to_string(),
      displacement: 200,
      power: 260,
      maneuver: 6,
      jump: 2,
      sensors: Sensors::Advanced,
      weapons: vec![
        Weapon::single(WeaponType::Particle, WeaponMount::Barbette),
        Weapon {
          mount: WeaponMount::Turret,
          guns: vec![
            Gun::new(WeaponType::Missile),
            Gun::new(WeaponType::Missile),
            Gun::new(WeaponType::Sand),
          ],
        },
      ],
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Executor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    let draw = |ship: &Ship, system: PowerSystem| {
      ship
        .power_lines()
        .into_iter()
        .find(|line| line.system == system)
        .map(|line| line.draw)
    };
    assert_eq!(draw(&ship, PowerSystem::Basic), Some(40), "20% of 200 tons");
    assert_eq!(draw(&ship, PowerSystem::Sensors), Some(6), "advanced sensors");
    assert_eq!(draw(&ship, PowerSystem::Maneuver), Some(120), "10% per Thrust, six of them");
    assert_eq!(draw(&ship, PowerSystem::Jump), Some(40), "and 10% per jump number");
    assert_eq!(draw(&ship, PowerSystem::Weapon(0)), Some(15), "a particle barbette");
    assert_eq!(
      draw(&ship, PowerSystem::Weapon(1)),
      Some(1),
      "racks draw nothing; the turret draws 1"
    );

    // The jump drive only draws as the ship jumps, so it is not in the
    // running total.
    assert_eq!(ship.power_demand(), 40 + 6 + 120 + 15 + 1);
    assert_eq!(ship.power_spare(), Some(260 - 182));
    assert_eq!(ship.max_acceleration(), 6, "and there is power enough to fly");

    // Lose most of the plant and the drive is what suffers.
    ship.current_power = 150;
    assert!(ship.power_spare().is_none(), "150 cannot run all of it");
    assert_eq!(ship.max_acceleration(), 4, "the drive gets what is left: 88 of 200");

    // The engineer shuts the barbette down and gets some of it back.
    ship.set_online(PowerSystem::Weapon(0), false);
    assert_eq!(ship.max_acceleration(), 5);

    // Basic systems cannot be switched off, only turned down.
    ship.set_online(PowerSystem::Basic, false);
    assert_eq!(draw(&ship, PowerSystem::Basic), Some(40));
    ship.set_basic_power_halved(true);
    assert_eq!(draw(&ship, PowerSystem::Basic), Some(20), "half, in an emergency");
    assert_eq!(ship.max_acceleration(), 6);
  }

  /// A hologram projector is a luxury: it starts off, and when the plant is
  /// short it loses its share before the ship loses Thrust.
  #[test]
  fn an_auxiliary_system_starts_off_and_gives_way_to_the_drive() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Harrier".to_string(),
      displacement: 200,
      power: 260,
      maneuver: 6,
      jump: 2,
      sensors: Sensors::Advanced,
      weapons: vec![],
      auxiliary: vec![AuxiliarySystem {
        name: "Holographic hull".to_string(),
        power: 100,
        default_on: false,
      }],
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Harrier".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    assert!(!ship.is_online(PowerSystem::Auxiliary(0)), "the projector is down at the dock");
    assert_eq!(ship.max_acceleration(), 6);

    // Bring it up: 40 for life support, 6 for the sensors and 120 for the
    // drive leave 94 of 260, which is not the 100 the projector wants.
    ship.set_online(PowerSystem::Auxiliary(0), true);
    assert!(!ship.is_powered(PowerSystem::Auxiliary(0)), "not enough left to light it");
    assert_eq!(ship.max_acceleration(), 6, "and the drive keeps its share");

    // Drop the drive and there is room for it.
    ship.set_online(PowerSystem::Maneuver, false);
    assert!(ship.is_powered(PowerSystem::Auxiliary(0)));
    assert_eq!(ship.max_acceleration(), 0);
  }

  /// Bandwidth, not Power, is what limits a computer -- and a ship can own
  /// more software than it can run at once. HMS Executor is the case in
  /// point: Evade/1, Fire Control/2 and Jump Control/2 is 30 Bandwidth on a
  /// Computer/20, so she fights or she jumps.
  #[test]
  fn software_runs_within_bandwidth_not_beyond_it() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Harrier".to_string(),
      displacement: 200,
      computer: 20,
      software: vec![
        Software::new(SoftwareKind::Manoeuvre, 0),
        Software::new(SoftwareKind::Library, 0),
        Software::new(SoftwareKind::Evade, 1),
        Software::new(SoftwareKind::FireControl, 2),
        Software::new(SoftwareKind::JumpControl, 2),
      ],
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Executor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    // She undocks running what fits, in the order the design lists it: Evade
    // and Fire Control fill the computer, and Jump Control is left out.
    assert_eq!(ship.bandwidth_used(), 20);
    assert_eq!(ship.running_level(SoftwareKind::Evade), Some(1));
    assert_eq!(ship.running_level(SoftwareKind::FireControl), Some(2));
    assert_eq!(ship.running_level(SoftwareKind::JumpControl), None);

    // Free software runs whatever else is on: there is no Bandwidth to free
    // by stopping it.
    assert!(ship.software_running.contains(&Software::new(SoftwareKind::Manoeuvre, 0)));

    // No room for the jump drive's software until something gives.
    assert!(!ship.set_software_running(Software::new(SoftwareKind::JumpControl, 2), true));
    assert!(ship.set_software_running(Software::new(SoftwareKind::FireControl, 2), false));
    assert!(ship.set_software_running(Software::new(SoftwareKind::JumpControl, 2), true));
    assert_eq!(ship.running_level(SoftwareKind::JumpControl), Some(2));

    // And nothing can run software the ship does not have aboard.
    assert!(!ship.set_software_running(Software::new(SoftwareKind::AutoRepair, 1), true));
  }

  /// A /bis computer is worth +5 for Jump Control alone, which is how the
  /// Type-S scout runs Jump Control/2 on a Processing 5 machine.
  #[test]
  fn a_bis_computer_counts_for_jump_control_only() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Scout/Courier".to_string(),
      displacement: 100,
      computer: 5,
      computer_bis: true,
      software: vec![
        Software::new(SoftwareKind::Library, 0),
        Software::new(SoftwareKind::JumpControl, 2),
      ],
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    assert_eq!(ship.processing(), 5);
    assert_eq!(ship.processing_for(SoftwareKind::JumpControl), 10);
    assert_eq!(
      ship.running_level(SoftwareKind::JumpControl),
      Some(2),
      "10 Bandwidth on a /bis 5"
    );

    // The same 10 Bandwidth spent on anything else does not fit.
    ship.software.push(Software::new(SoftwareKind::Evade, 1));
    assert!(!ship.set_software_running(Software::new(SoftwareKind::Evade, 1), true));
  }

  /// Jump fuel is a tenth of the ship's *tonnage* per jump number. A
  /// Scout/Courier is 100 tons with 40 hull points, so its jump-2 costs 20
  /// tons of fuel -- not the 4 that reading hull points would give.
  #[test]
  fn jump_fuel_is_a_tenth_of_the_tonnage_per_jump_number() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Scout/Courier".to_string(),
      displacement: 100,
      hull: 40,
      jump: 2,
      fuel: 23,
      power: 60,
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Dragon".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    assert_eq!(ship.fuel_per_jump_number(), 10);
    assert_eq!(ship.fuel_for_full_jump(), 20);
    assert_eq!(ship.jump_range_available(), 2, "23 tons is enough for a jump-2");

    // Enough for one jump number but not two: the ship still jumps, less far.
    ship.current_fuel = 19;
    assert_eq!(ship.jump_range_available(), 1);

    ship.current_fuel = 9;
    assert_eq!(ship.jump_range_available(), 0, "not enough for a jump-1");

    // A damaged drive caps the range whatever the tanks hold.
    ship.current_fuel = 23;
    ship.current_jump = 1;
    assert_eq!(ship.jump_range_available(), 1);
  }

  /// A plant that cannot feed everything feeds what it can, in order, and the
  /// drive takes what is left -- which is the one system that does something
  /// useful with a partial share.
  #[test]
  fn a_damaged_plant_browns_out_what_it_cannot_feed() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Executor".to_string(),
      displacement: 200,
      power: 260,
      maneuver: 6,
      jump: 2,
      sensors: Sensors::Advanced,
      weapons: vec![Weapon::single(WeaponType::Particle, WeaponMount::Barbette)],
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Executor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    // Healthy: everything runs, and there is room for the jump drive too.
    assert!(ship.sensors_powered());
    assert!(ship.weapon_powered(0));
    assert_eq!(ship.max_acceleration(), 6);
    assert!(ship.jump_powered());

    // A plant at 100 runs basic systems (40), sensors (6) and the barbette
    // (15), leaving 39 for a drive that wants 120: a tenth of the hull buys
    // one Thrust, so 39 buys one.
    ship.current_power = 100;
    assert!(ship.sensors_powered());
    assert!(ship.weapon_powered(0));
    assert_eq!(ship.max_acceleration(), 1);
    assert!(!ship.jump_powered(), "nothing like enough left to jump");

    // Switching the barbette off hands its share to the drive.
    ship.set_online(PowerSystem::Weapon(0), false);
    assert!(!ship.weapon_powered(0), "and it cannot fire while it is off");
    assert_eq!(ship.max_acceleration(), 2);

    // At 40 there is nothing for anything but life support.
    ship.current_power = 40;
    assert!(!ship.sensors_powered());
    assert_eq!(ship.max_acceleration(), 0);

    // Half power on basic systems frees twenty, which the sensors take first.
    ship.set_basic_power_halved(true);
    assert!(ship.sensors_powered());
    assert_eq!(ship.max_acceleration(), 0, "but not enough for a tenth of the hull");
  }

  /// The pilot's Evade order stands between rounds; only the allowance is
  /// spent. It used to be the same number, so a pilot who dodged two attacks
  /// silently stopped dodging from then on.
  #[test]
  fn a_dodge_order_survives_the_round_it_is_spent_in() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Executor".to_string(),
      maneuver: 6,
      power: 300,
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Executor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    ship.set_pilot_actions(Some(2), None).expect("a 6G hull can spare 2G");

    ship.decrement_dodge_thrust();
    assert_eq!(ship.get_dodge_thrust(), 1, "one attack dodged, one left");
    ship.decrement_dodge_thrust();
    assert_eq!(ship.get_dodge_thrust(), 0, "the round's allowance is spent");

    ship.reset_temporary_bonuses();
    assert_eq!(ship.get_dodge_thrust(), 2, "and it comes back next round");
  }
  use crate::crew::Skills;
  use cgmath::assert_ulps_eq;

  struct ShipTemplateRestoreGuard(ShipTemplateTable);

  impl Drop for ShipTemplateRestoreGuard {
    fn drop(&mut self) {
      replace_ship_templates(self.0.clone());
    }
  }

  /// Scratch directory for the design-loader tests. The caller removes it.
  fn make_design_scratch_dir(tag: &str) -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
      .duration_since(std::time::UNIX_EPOCH)
      .expect("clock before epoch")
      .as_nanos();
    let dir = std::env::temp_dir().join(format!("callisto_designs_{tag}_{nanos}"));
    std::fs::create_dir_all(&dir).expect("unable to create scratch dir");
    dir
  }

  fn minimal_design_json(name: &str) -> String {
    format!(
      r#"{{"name":"{name}","displacement":100,"hull":40,"armor":0,"maneuver":1,"jump":1,
          "power":50,"fuel":10,"crew":4,"sensors":"Civilian","computer":5,
          "weapons":[{{"kind":"Beam","mount":{{"Turret":1}}}}],"tl":12}}"#
    )
  }

  /// The loader reads files with bounded concurrency
  /// ([`MAX_CONCURRENT_DIR_FILE_READS`]) rather than firing them all at once.
  /// Bounding must not lose files: with more designs than the limit, every one
  /// still has to come back. Results also arrive out of completion order, so
  /// this pins that the table is keyed by design name, not by position.
  #[test_log::test(tokio::test)]
  async fn test_load_ship_templates_reads_more_files_than_the_concurrency_limit() {
    let dir = make_design_scratch_dir("bounded");
    let count = MAX_CONCURRENT_DIR_FILE_READS * 3 + 1;
    for i in 0..count {
      std::fs::write(
        dir.join(format!("design_{i}.json")),
        minimal_design_json(&format!("Design {i}")),
      )
      .expect("unable to write design file");
    }

    let table = load_ship_templates_from_dir(dir.to_str().expect("non-utf8 scratch path"))
      .await
      .expect("loading a directory of valid designs must succeed");

    assert_eq!(table.len(), count, "Every design must survive the bounded fan-out");
    for i in 0..count {
      assert!(table.contains_key(&format!("Design {i}")), "Missing design {i}");
    }

    std::fs::remove_dir_all(&dir).ok();
  }

  /// Bounding the fan-out must not change the error policy: a single malformed
  /// file is logged and skipped, and the rest of the directory still loads.
  #[test_log::test(tokio::test)]
  async fn test_load_ship_templates_skips_unparseable_files() {
    let dir = make_design_scratch_dir("parse_error");
    for i in 0..MAX_CONCURRENT_DIR_FILE_READS + 2 {
      std::fs::write(
        dir.join(format!("design_{i}.json")),
        minimal_design_json(&format!("Design {i}")),
      )
      .expect("unable to write design file");
    }
    std::fs::write(dir.join("broken.json"), b"{ not json").expect("unable to write broken file");

    let table = load_ship_templates_from_dir(dir.to_str().expect("non-utf8 scratch path"))
      .await
      .expect("a single malformed file must not fail the whole load");

    assert_eq!(
      table.len(),
      MAX_CONCURRENT_DIR_FILE_READS + 2,
      "Only the broken file may be dropped"
    );

    std::fs::remove_dir_all(&dir).ok();
  }

  #[test_log::test]
  fn test_digit_to_int() {
    // Test digits 0-9
    assert_eq!(digit_to_int('0'), 0);
    assert_eq!(digit_to_int('1'), 1);
    assert_eq!(digit_to_int('2'), 2);
    assert_eq!(digit_to_int('3'), 3);
    assert_eq!(digit_to_int('4'), 4);
    assert_eq!(digit_to_int('5'), 5);
    assert_eq!(digit_to_int('6'), 6);
    assert_eq!(digit_to_int('7'), 7);
    assert_eq!(digit_to_int('8'), 8);
    assert_eq!(digit_to_int('9'), 9);

    // Test all valid letters A-Z (excluding I, O)
    assert_eq!(digit_to_int('A'), 10);
    assert_eq!(digit_to_int('B'), 11);
    assert_eq!(digit_to_int('C'), 12);
    assert_eq!(digit_to_int('D'), 13);
    assert_eq!(digit_to_int('E'), 14);
    assert_eq!(digit_to_int('F'), 15);
    assert_eq!(digit_to_int('G'), 16);
    assert_eq!(digit_to_int('H'), 17);
    assert_eq!(digit_to_int('J'), 18);
    assert_eq!(digit_to_int('K'), 19);
    assert_eq!(digit_to_int('L'), 20);
    assert_eq!(digit_to_int('M'), 21);
    assert_eq!(digit_to_int('N'), 22);
    assert_eq!(digit_to_int('P'), 23);
    assert_eq!(digit_to_int('Q'), 24);
    assert_eq!(digit_to_int('R'), 25);
    assert_eq!(digit_to_int('S'), 26);
    assert_eq!(digit_to_int('T'), 27);
    assert_eq!(digit_to_int('U'), 28);
    assert_eq!(digit_to_int('V'), 29);
    assert_eq!(digit_to_int('W'), 30);
    assert_eq!(digit_to_int('X'), 31);
    assert_eq!(digit_to_int('Y'), 32);
    assert_eq!(digit_to_int('Z'), 33);
  }

  #[test_log::test]
  fn test_int_to_digit() {
    // Test integers 0-9
    assert_eq!(int_to_digit(0), '0');
    assert_eq!(int_to_digit(1), '1');
    assert_eq!(int_to_digit(2), '2');
    assert_eq!(int_to_digit(3), '3');
    assert_eq!(int_to_digit(4), '4');
    assert_eq!(int_to_digit(5), '5');
    assert_eq!(int_to_digit(6), '6');
    assert_eq!(int_to_digit(7), '7');
    assert_eq!(int_to_digit(8), '8');
    assert_eq!(int_to_digit(9), '9');

    // Test all valid integers 10-33 (corresponding to A-Z, excluding I, O)
    assert_eq!(int_to_digit(10), 'A');
    assert_eq!(int_to_digit(11), 'B');
    assert_eq!(int_to_digit(12), 'C');
    assert_eq!(int_to_digit(13), 'D');
    assert_eq!(int_to_digit(14), 'E');
    assert_eq!(int_to_digit(15), 'F');
    assert_eq!(int_to_digit(16), 'G');
    assert_eq!(int_to_digit(17), 'H');
    assert_eq!(int_to_digit(18), 'J');
    assert_eq!(int_to_digit(19), 'K');
    assert_eq!(int_to_digit(20), 'L');
    assert_eq!(int_to_digit(21), 'M');
    assert_eq!(int_to_digit(22), 'N');
    assert_eq!(int_to_digit(23), 'P');
    assert_eq!(int_to_digit(24), 'Q');
    assert_eq!(int_to_digit(25), 'R');
    assert_eq!(int_to_digit(26), 'S');
    assert_eq!(int_to_digit(27), 'T');
    assert_eq!(int_to_digit(28), 'U');
    assert_eq!(int_to_digit(29), 'V');
    assert_eq!(int_to_digit(30), 'W');
    assert_eq!(int_to_digit(31), 'X');
    assert_eq!(int_to_digit(32), 'Y');
    assert_eq!(int_to_digit(33), 'Z');
  }

  #[test_log::test(tokio::test)]
  async fn test_replacing_global_templates_does_not_mutate_existing_ship_designs() {
    // Held for the whole test: it installs its own global registry and asserts
    // on it further down, so no other test may re-seed SHIP_TEMPLATES in the
    // meantime. Declared before the restore guard so the guard runs first.
    let templates_lock = lock_ship_templates_for_test().await;
    config_test_ship_templates_locked(&templates_lock).await;

    let previous_templates = get_ship_templates_snapshot();
    let _restore_guard = ShipTemplateRestoreGuard(previous_templates.as_ref().clone());

    let original_template = Arc::new(ShipDesignTemplate {
      name: "Test Design".to_string(),
      displacement: 100,
      hull: 10,
      armor: 2,
      maneuver: 1,
      jump: 1,
      power: 25,
      fuel: 10,
      crew: 4,
      sensors: Sensors::Basic,
      stealth: None,
      countermeasures: None,
      computer: 1,
      crew_skills: None,
      weapons: vec![],
      screens: vec![],
      auxiliary: vec![],
      software: vec![],
      computer_bis: false,
      computer_fib: false,
      tl: 10,
      role: None,
      source: None,
    });
    let mut templates = previous_templates.as_ref().clone();
    templates.insert("Test Design".to_string(), original_template.clone());
    replace_ship_templates(templates);

    let ship = Ship::new(
      "Snapshot Test Ship".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      &get_ship_template("Test Design").unwrap(),
      None,
      None,
    );

    let mut updated_template = (*original_template).clone();
    updated_template.power = 99;
    let mut updated_templates = previous_templates.as_ref().clone();
    updated_templates.insert("Test Design".to_string(), Arc::new(updated_template));
    replace_ship_templates(updated_templates);

    assert_eq!(ship.design.power, 25);
    assert_eq!(get_ship_template("Test Design").unwrap().power, 99);
  }

  #[test_log::test]
  fn test_digit_to_int_invalid_cases() {
    let invalid_chars = ['I', 'O', 'a', 'i', 'o', 'z', '#', ' ', '-'];
    for &c in &invalid_chars {
      let result = std::panic::catch_unwind(|| digit_to_int(c));
      assert!(result.is_err(), "Expected panic for character: {c}");
    }
  }

  #[test_log::test]
  fn test_int_to_digit_invalid_cases() {
    let invalid_ints = [34, 35, 99, 255];
    for &i in &invalid_ints {
      let result = std::panic::catch_unwind(|| int_to_digit(i));
      assert!(result.is_err(), "Expected panic for integer: {i}");
    }
  }

  #[test_log::test]
  fn test_digit_conversion_roundtrip() {
    // Test roundtrip conversion for all valid values
    for i in 0..34 {
      let digit = int_to_digit(i);
      let num = digit_to_int(digit);
      assert_eq!(i, num, "Roundtrip failed for number {i}");
    }
  }

  #[test_log::test]
  fn test_ship_setters_and_getters() {
    let initial_position = Vec3::new(0.0, 0.0, 0.0);
    let initial_velocity = Vec3::new(1.0, 1.0, 1.0);

    let mut ship = Ship::new(
      "TestShip".to_string(),
      initial_position,
      initial_velocity,
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test initial values
    assert_eq!(ship.get_name(), "TestShip");
    assert_eq!(ship.get_position(), initial_position);
    assert_eq!(ship.get_velocity(), initial_velocity);

    // Test setters
    let new_name = "UpdatedShip".to_string();
    let new_position = Vec3::new(10.0, 20.0, 30.0);
    let new_velocity = Vec3::new(2.0, 3.0, 4.0);
    let new_plan = FlightPlan::acceleration(Vec3::new(1.0, 1.0, 1.0));

    ship.set_name(new_name.clone());
    ship.set_position(new_position);
    ship.set_velocity(new_velocity);
    assert!(ship.set_flight_plan(&new_plan).is_ok());

    // Test updated values
    assert_eq!(ship.get_name(), new_name);
    assert_eq!(ship.get_position(), new_position);
    assert_eq!(ship.get_velocity(), new_velocity);
    assert_eq!(ship.plan, new_plan);

    // Test hull and structure
    assert_eq!(ship.current_hull, 160); // 2 * usp.hull (3 for '3' in the USP)

    // Test invalid flight plan
    let invalid_plan = FlightPlan::acceleration(Vec3::new(100.0, 100.0, 100.0)); // Assuming this exceeds max acceleration
    assert!(ship.set_flight_plan(&invalid_plan).is_err());
    assert_eq!(ship.plan, new_plan); // Plan should not have changed
  }
  #[test_log::test]
  fn test_flight_plan_set_first_and_second() {
    let mut flight_plan = FlightPlan::default();

    // Test set_first
    let accel1 = Vec3::new(1.0, 2.0, 3.0);
    let time1 = 5000;
    flight_plan.set_first(accel1, time1);

    assert_eq!(flight_plan.0 .0, accel1);
    assert_eq!(flight_plan.0 .1, time1);
    assert_eq!(flight_plan.1, None);

    // Test set_second
    let accel2 = Vec3::new(-2.0, -1.0, 0.0);
    let time2 = 3000;
    flight_plan.set_second(accel2, time2);

    assert_eq!(flight_plan.0 .0, accel1);
    assert_eq!(flight_plan.0 .1, time1);
    assert_eq!(flight_plan.1, Some(AccelPair(accel2, time2)));

    // Test overwriting first acceleration
    let new_accel1 = Vec3::new(4.0, 5.0, 6.0) * G;
    let new_time1 = 2000;
    flight_plan.set_first(new_accel1, new_time1);

    assert_eq!(flight_plan.0 .0, new_accel1);
    assert_eq!(flight_plan.0 .1, new_time1);
    assert_eq!(flight_plan.1, None);

    // Test overwriting second acceleration
    flight_plan.set_second(accel2, time2);
    let new_accel2 = Vec3::new(-3.0, -4.0, -5.0) * G;
    let new_time2 = 4000;
    flight_plan.set_second(new_accel2, new_time2);

    assert_eq!(flight_plan.0 .0, new_accel1);
    assert_eq!(flight_plan.0 .1, new_time1);
    assert_eq!(flight_plan.1, Some(AccelPair(new_accel2, new_time2)));
  }

  #[test_log::test]
  fn test_flight_plan_ensure_thrust_limit() {
    let mut flight_plan = FlightPlan::default();

    // Test case 1: Acceleration within limit
    let accel1 = Vec3::new(3.0, 4.0, 0.0) * G; // magnitude 5
    let time1 = 5000;
    flight_plan.set_first(accel1, time1);
    flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0) * G, 3000); // magnitude 3

    flight_plan.ensure_thrust_limit(6.0 * G);

    assert_ulps_eq!(flight_plan.0 .0, accel1);
    assert_eq!(flight_plan.0 .1, time1);
    assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0) * G);
    assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

    // Test case 2: First acceleration exceeds limit
    let accel2 = Vec3::new(6.0, 8.0, 0.0) * G; // magnitude 10
    flight_plan.set_first(accel2, time1);
    flight_plan.set_second(Vec3::new(1.0, 2.0, 2.0) * G, 3000); // magnitude 3

    flight_plan.ensure_thrust_limit(6.0 * G);

    let expected_accel2 = accel2.normalize() * 6.0 * G;
    assert_ulps_eq!(flight_plan.0 .0, expected_accel2);
    assert_eq!(flight_plan.0 .1, time1);
    assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(1.0, 2.0, 2.0) * G);
    assert_eq!(flight_plan.1.as_ref().unwrap().1, 3000);

    // Test case 3: Second acceleration exceeds limit
    flight_plan.set_second(Vec3::new(4.0, 4.0, 4.0) * G, 2000); // magnitude ~6.93G

    flight_plan.ensure_thrust_limit(6.0 * G);

    assert_ulps_eq!(flight_plan.0 .0, expected_accel2);
    assert_eq!(flight_plan.0 .1, time1);
    let expected_accel3 = Vec3::new(4.0, 4.0, 4.0).normalize() * 6.0 * G;
    assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, expected_accel3);
    assert_eq!(flight_plan.1.as_ref().unwrap().1, 2000);

    // Test case 4: Both accelerations exceed limit
    flight_plan.set_first(Vec3::new(10.0, 0.0, 0.0) * G, 1000);
    flight_plan.set_second(Vec3::new(0.0, 8.0, 6.0) * G, 1500);

    flight_plan.ensure_thrust_limit(4.0 * G);

    assert_ulps_eq!(flight_plan.0 .0, Vec3::new(4.0, 0.0, 0.0) * G);
    assert_eq!(flight_plan.0 .1, 1000);
    assert_ulps_eq!(flight_plan.1.as_ref().unwrap().0, Vec3::new(0.0, 3.2, 2.4) * G);
    assert_eq!(flight_plan.1.as_ref().unwrap().1, 1500);
  }

  #[test_log::test]
  fn test_flight_plan_advance_time() {
    let mut flight_plan = FlightPlan::default();
    let accel1 = Vec3::new(1.0, 2.0, 3.0) * G;
    let time1 = 5000;
    let accel2 = Vec3::new(-2.0, -1.0, 0.0) * G;
    let time2 = 3000;
    flight_plan.set_first(accel1, time1);
    flight_plan.set_second(accel2, time2);

    // Test case 1: Advance time less than first duration
    let result = flight_plan.advance_time(2000);
    assert_eq!(result.0 .0, accel1);
    assert_eq!(result.0 .1, 2000);
    assert_eq!(result.1, None);
    assert_eq!(flight_plan.0 .0, accel1);
    assert_eq!(flight_plan.0 .1, 3000);
    assert_eq!(flight_plan.1, Some(AccelPair(accel2, time2)));

    // Test case 2: Advance time equal to remaining first duration
    let result = flight_plan.advance_time(3000);
    assert_eq!(result.0 .0, accel1);
    assert_eq!(result.0 .1, 3000);
    assert_eq!(result.1, None);
    assert_eq!(flight_plan.0 .0, accel2);
    assert_eq!(flight_plan.0 .1, time2);
    assert_eq!(flight_plan.1, None);

    // Reset flight plan for next test
    flight_plan.set_first(accel1, time1);
    flight_plan.set_second(accel2, time2);

    // Test case 3: Advance time more than first duration but less than total duration
    let result = flight_plan.advance_time(6000);
    assert_eq!(result.0 .0, accel1);
    assert_eq!(result.0 .1, time1);
    assert_eq!(result.1, Some(AccelPair(accel2, 1000)));
    assert_eq!(flight_plan.0 .0, accel2);
    assert_eq!(flight_plan.0 .1, 2000);
    assert_eq!(flight_plan.1, None);

    // Test case 4: Advance time more than total duration
    let result = flight_plan.advance_time(3000);
    assert_eq!(result.0 .0, accel2);
    assert_eq!(result.0 .1, 2000);
    assert_eq!(result.1, None);
    assert_eq!(flight_plan.0 .0, Vec3::zero());
    assert_eq!(flight_plan.0 .1, 0);
    assert_eq!(flight_plan.1, None);
  }

  #[test_log::test]
  fn test_ship_set_flight_plan() {
    let initial_position = Vec3::new(0.0, 0.0, 0.0);
    let initial_velocity = Vec3::new(1.0, 1.0, 1.0);

    let mut ship = Ship::new(
      "TestShip".to_string(),
      initial_position,
      initial_velocity,
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test case 1: Set a valid flight plan
    let valid_plan = FlightPlan::new(
      AccelPair(Vec3::new(2.0, 2.0, 1.0), 5000),
      Some(AccelPair(Vec3::new(-1.0, -1.0, -1.0), 3000)),
    );
    assert!(ship.set_flight_plan(&valid_plan).is_ok());
    assert_eq!(ship.plan, valid_plan);

    // Test case 2: Set a flight plan with acceleration exceeding ship's capabilities
    let invalid_plan = FlightPlan::new(AccelPair(Vec3::new(100.0, 100.0, 100.0), 5000), None);
    assert!(ship.set_flight_plan(&invalid_plan).is_err());
    assert_eq!(ship.plan, valid_plan); // Plan should not have changed

    // Test case 3: Set a flight plan with only one acceleration
    let single_accel_plan = FlightPlan::new(AccelPair(Vec3::new(2.0, 2.0, 1.0), 4000), None);
    assert!(ship.set_flight_plan(&single_accel_plan).is_ok());
    assert_eq!(ship.plan, single_accel_plan);

    // Test case 4: Set a flight plan with zero acceleration
    let zero_accel_plan = FlightPlan::new(AccelPair(Vec3::zero(), 5000), Some(AccelPair(Vec3::zero(), 3000)));
    assert!(ship.set_flight_plan(&zero_accel_plan).is_ok());
    assert_eq!(ship.plan, zero_accel_plan);

    // Test case 5: Set a flight plan with acceleration at the ship's limit
    let max_accel = f64::from(ship.max_acceleration()) * G;
    let max_accel_plan = FlightPlan::new(
      AccelPair(Vec3::new(max_accel, 0.0, 0.0), 5000),
      Some(AccelPair(Vec3::new(0.0, max_accel, 0.0), 3000)),
    );
    assert!(
      ship.set_flight_plan(&max_accel_plan).is_ok(),
      "Setting flight plan to {max_accel_plan:?} failed."
    );
    assert_eq!(ship.plan, max_accel_plan);

    // Test case 6: Set a flight plan with a second acceleration exceeding ship's capabilities
    let invalid_plan2 = FlightPlan::new(
      AccelPair(Vec3::new(2.0, 2.0, 0.0), 5000),
      Some(AccelPair(Vec3::new(100.0, 100.0, 100.0), 3000)),
    );
    assert!(ship.set_flight_plan(&invalid_plan2).is_err());
    assert_eq!(ship.plan, max_accel_plan); // Plan should not have changed
  }

  #[test_log::test]
  fn test_ship_ordering() {
    let ship1 = Ship::new(
      "ship1".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    let ship2 = Ship::new(
      "ship2".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    assert!(ship1 < ship2);
    assert!(ship2 > ship1);
    assert!(ship1 <= ship2);
    assert!(ship2 >= ship1);
    assert_ne!(ship1, ship2);
  }

  #[test_log::test]
  fn test_flight_plan_iterator() {
    // Test case 1: FlightPlan with two accelerations
    let accel1 = Vec3::new(1.0, 2.0, 3.0);
    let time1 = 5000;
    let accel2 = Vec3::new(-2.0, -1.0, 0.0);
    let time2 = 3000;
    let flight_plan = FlightPlan::new(AccelPair(accel1, time1), Some(AccelPair(accel2, time2)));

    let mut iter = flight_plan.iter();
    assert_eq!(iter.next(), Some(AccelPair(accel1, time1)));
    assert_eq!(iter.next(), Some(AccelPair(accel2, time2)));
    assert_eq!(iter.next(), None);

    // Test case 2: FlightPlan with only one acceleration
    let flight_plan = FlightPlan::new(AccelPair(accel1, time1), None);

    let mut iter = flight_plan.iter();
    assert_eq!(iter.next(), Some(AccelPair(accel1, time1)));
    assert_eq!(iter.next(), None);

    // Test case 3: Empty FlightPlan
    let flight_plan = FlightPlan::default();

    let mut iter = flight_plan.iter();
    assert_eq!(iter.next(), Some(AccelPair(Vec3::zero(), DEFAULT_ACCEL_DURATION)));
    assert_eq!(iter.next(), None);

    // Test case 4: FlightPlan with zero acceleration
    let zero_accel = Vec3::zero();
    let flight_plan = FlightPlan::new(AccelPair(zero_accel, time1), Some(AccelPair(zero_accel, time2)));

    let mut iter = flight_plan.iter();
    assert_eq!(iter.next(), Some(AccelPair(zero_accel, time1)));
    assert_eq!(iter.next(), Some(AccelPair(zero_accel, time2)));
    assert_eq!(iter.next(), None);

    // Test case 5: Using a for loop with the iterator
    let flight_plan = FlightPlan::new(AccelPair(accel1, time1), Some(AccelPair(accel2, time2)));

    let mut count = 0;
    for (index, accel_pair) in flight_plan.iter().enumerate() {
      match index {
        0 => assert_eq!(accel_pair, AccelPair(accel1, time1)),
        1 => assert_eq!(accel_pair, AccelPair(accel2, time2)),
        _ => panic!("Unexpected iteration"),
      }
      count += 1;
    }
    assert_eq!(count, 2);
  }

  #[test_log::test]
  fn test_set_agility_thrust() {
    let mut ship = Ship::new(
      "TestShip".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test setting a valid agility thrust
    assert!(ship.set_pilot_actions(Some(1), None).is_ok());
    assert_eq!(ship.get_dodge_thrust(), 1);

    // Test setting agility thrust to 0
    assert!(ship.set_pilot_actions(Some(0), None).is_ok());
    assert_eq!(ship.get_dodge_thrust(), 0);

    // Test setting agility thrust to max acceleration
    assert!(ship.set_pilot_actions(Some(3), None).is_ok());
    assert_eq!(ship.get_dodge_thrust(), 3);

    // Test setting agility thrust above max acceleration
    let result = ship.set_pilot_actions(Some(11), None);
    assert!(result.is_err());
    assert_eq!(ship.get_dodge_thrust(), 3); // Should remain unchanged

    // Test that the error returned is of type InvalidAgilityError
    assert!(matches!(result, Err(InvalidThrustError(_))));
    let err = result.unwrap_err();
    assert!(err.source().is_none());
    assert!(format!("{err}").contains("Invalid thrust attempted:"));

    // Test resetting agility
    ship.reset_pilot_actions();
    assert_eq!(ship.get_dodge_thrust(), 0);
  }

  #[test_log::test]
  fn test_get_crew() {
    let ship = Ship::new(
      "TestShip".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      Some(Crew::new()),
      None,
    );

    // Test get_crew
    let crew = ship.get_crew();
    assert_eq!(crew.get_pilot(), 0);
    assert_eq!(crew.get_engineering_jump(), 0);
    assert_eq!(crew.get_engineering_power(), 0);
    assert_eq!(crew.get_engineering_maneuver(), 0);
    assert_eq!(crew.get_sensors(), 0);
    assert_eq!(crew.get_gunnery(0), 0);
  }

  #[test_log::test]
  fn test_get_crew_mut() {
    let mut ship = Ship::new(
      "TestShip".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      Some(Crew::new()),
      None,
    );

    // Test get_crew_mut
    {
      let crew_mut = ship.get_crew_mut();
      crew_mut.set_skill(Skills::Pilot, 3);
      crew_mut.set_skill(Skills::EngineeringJump, 2);
      crew_mut.set_skill(Skills::EngineeringPower, 1);
      crew_mut.set_skill(Skills::EngineeringManeuver, 4);
      crew_mut.set_skill(Skills::Sensors, 5);
      crew_mut.add_gunnery(2);
    }

    // Verify changes
    let crew = ship.get_crew();
    assert_eq!(crew.get_pilot(), 3);
    assert_eq!(crew.get_engineering_jump(), 2);
    assert_eq!(crew.get_engineering_power(), 1);
    assert_eq!(crew.get_engineering_maneuver(), 4);
    assert_eq!(crew.get_sensors(), 5);
    assert_eq!(crew.get_gunnery(0), 2);
  }

  #[test_log::test]
  fn test_weapon_ordering() {
    // Create test weapons with different mounts and types
    let large_bay_beam = Weapon::single(WeaponType::Beam, WeaponMount::Bay(BaySize::Large));
    let large_bay_pulse = Weapon::single(WeaponType::Pulse, WeaponMount::Bay(BaySize::Large));
    let medium_bay = Weapon::single(WeaponType::Beam, WeaponMount::Bay(BaySize::Medium));

    let medium_bay_missile = Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Medium));

    let small_bay = Weapon::single(WeaponType::Beam, WeaponMount::Bay(BaySize::Small));

    let small_bay_pulse = Weapon::single(WeaponType::Pulse, WeaponMount::Bay(BaySize::Small));

    let barbette = Weapon::single(WeaponType::Beam, WeaponMount::Barbette);
    let turret = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 2);
    let turret_pulse = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 2);

    // Test ordering between same mount types
    assert!(large_bay_beam < large_bay_pulse); // Same mount, different types

    // Test ordering between different mount types
    assert!(large_bay_beam < medium_bay); // Large bay < Medium bay
    assert!(medium_bay > large_bay_pulse); // Large bay < Medium bay
    assert!(medium_bay < small_bay); // Medium bay < Small bay
    assert!(small_bay > medium_bay_missile); // Medium bay < Small bay
    assert!(medium_bay_missile > medium_bay); // Medium bay < Small bay
    assert!(small_bay < barbette); // Small bay < Barbette
    assert!(small_bay_pulse > small_bay); // Small bay < Barbette
    assert!(small_bay < small_bay_pulse); // Small bay < Barbette
    assert!(small_bay > large_bay_pulse);
    assert!(barbette < turret); // Barbette < Turret
    assert!(turret > barbette); // Barbette < Turret
    assert!(turret < turret_pulse); // Barbette < Turret

    // Test transitivity
    assert!(large_bay_beam < small_bay); // Large bay < Small bay
    assert!(medium_bay < barbette); // Medium bay < Barbette
    assert!(small_bay < turret); // Small bay < Turret

    // Test turret comparison with bays
    assert!(turret > large_bay_beam); // Turret > Large bay
    assert!(turret > medium_bay); // Turret > Medium bay
    assert!(turret > small_bay); // Turret > Small bay

    // A fixed mount is the least capable mount, so it sorts after everything.
    let fixed = Weapon::single(WeaponType::Beam, WeaponMount::FixedMount);
    let fixed_pulse = Weapon::single(WeaponType::Pulse, WeaponMount::FixedMount);
    assert!(fixed > turret);
    assert!(turret < fixed);
    assert!(fixed > barbette);
    assert!(fixed > small_bay);
    assert!(fixed < fixed_pulse);
  }

  #[test_log::test]
  fn test_fixed_mount_naming_and_serde() {
    let fixed = Weapon::single(WeaponType::Missile, WeaponMount::FixedMount);
    assert_eq!(String::from(&fixed), "missile fixed mount");

    // The wire form is a bare string, like the other unit variant (Barbette).
    let json = serde_json::to_string(&fixed).unwrap();
    assert_eq!(json, r#"{"kind":"Missile","mount":"FixedMount"}"#);
    assert_eq!(serde_json::from_str::<Weapon>(&json).unwrap(), fixed);
  }

  #[test_log::test]
  fn test_sensors() {
    // Test Sensors::max
    assert_eq!(Sensors::max(Sensors::Basic, Sensors::Military), Sensors::Military);
    assert_eq!(Sensors::max(Sensors::Advanced, Sensors::Civilian), Sensors::Advanced);
    assert_eq!(Sensors::max(Sensors::Military, Sensors::Military), Sensors::Military);

    // Test conversion to i32
    assert_eq!(i32::from(Sensors::Basic), -4);
    assert_eq!(i32::from(Sensors::Civilian), -2);
    assert_eq!(i32::from(Sensors::Military), 0);
    assert_eq!(i32::from(Sensors::Improved), 1);
    assert_eq!(i32::from(Sensors::Advanced), 2);

    // Test ordering
    assert!(Sensors::Basic < Sensors::Civilian);
    assert!(Sensors::Civilian < Sensors::Military);
    assert!(Sensors::Military < Sensors::Improved);
    assert!(Sensors::Improved < Sensors::Advanced);
  }

  #[test_log::test]
  fn test_fixup_current_values() {
    // Create a ship design template with some values
    let design = Arc::new(ShipDesignTemplate {
      name: "Test Ship".to_string(),
      displacement: 400,
      hull: 100,
      armor: 50,
      maneuver: 4,
      jump: 2,
      power: 200,
      fuel: 1000,
      crew: 20,
      sensors: Sensors::Military,
      stealth: None,
      countermeasures: None,
      computer: 10,
      crew_skills: None,
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 2),
        Weapon::single(WeaponType::Pulse, WeaponMount::Bay(BaySize::Small)),
      ],
      screens: vec![],
      auxiliary: vec![],
      software: vec![],
      computer_bis: false,
      computer_fib: false,
      tl: 12,
      role: None,
      source: None,
    });

    // Create a ship with lower current values
    let mut ship = Ship::new("TestShip".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    // Manually set current values to be lower than design values
    ship.current_hull = 50; // Lower than design.hull (100)
    ship.current_armor = 25; // Lower than design.armor (50)
    ship.current_power = 100; // Lower than design.power (200)
    ship.current_maneuver = 2; // Lower than design.maneuver (4)
    ship.current_jump = 1; // Lower than design.jump (2)
    ship.current_fuel = 500; // Lower than design.fuel (1000)
    ship.current_crew = 10; // Lower than design.crew (20)
    ship.current_sensors = Sensors::Basic; // Lower than design.sensors (Military)
    ship.active_weapons = vec![false, false]; // All false
    ship.crit_level = [1; 11]; // All ones
    ship.attack_dm = -2; // Negative value
    ship.dodge_thrust = 2; // Non-zero value

    // Call fixup_current_values
    ship.fixup_current_values();

    // Verify all values are restored to design values
    assert_eq!(ship.current_hull, design.hull);
    assert_eq!(ship.current_armor, design.armor);
    assert_eq!(ship.current_power, design.power);
    assert_eq!(ship.current_maneuver, design.maneuver);
    assert_eq!(ship.current_jump, design.jump);
    assert_eq!(ship.current_fuel, design.fuel);
    assert_eq!(ship.current_crew, design.crew);
    assert_eq!(ship.current_sensors, design.sensors);
    assert_eq!(ship.active_weapons, vec![true, true]);
    assert_eq!(ship.crit_level, [0; 11]);
    assert_eq!(ship.attack_dm, 0);
    assert_eq!(ship.dodge_thrust, 0);

    // Test that values higher than design values are not reduced
    ship.current_hull = 150; // Higher than design.hull
    ship.current_armor = 75; // Higher than design.armor
    ship.current_sensors = Sensors::Advanced; // Higher than design.sensors

    ship.fixup_current_values();

    // Verify higher values are preserved
    assert_eq!(ship.current_hull, 150);
    assert_eq!(ship.current_armor, 75);
    assert_eq!(ship.current_sensors, Sensors::Advanced);
  }

  #[test]
  fn test_stealth_to_i32_conversion() {
    assert_eq!(i32::from(Stealth::Basic), -2);
    assert_eq!(i32::from(Stealth::Improved), -2);
    assert_eq!(i32::from(Stealth::Enhanced), -4);
    assert_eq!(i32::from(Stealth::Advanced), -6);
  }

  #[test]
  fn test_stealth_to_string_conversion() {
    assert_eq!(String::from(Stealth::Basic), "Basic");
    assert_eq!(String::from(Stealth::Improved), "Improved");
    assert_eq!(String::from(Stealth::Enhanced), "Enhanced");
    assert_eq!(String::from(Stealth::Advanced), "Advanced");
  }

  #[test]
  fn test_stealth_string_case_sensitivity() {
    // Verify that the strings match exactly, including case
    let basic = String::from(Stealth::Basic);
    assert_ne!(basic, "basic");
    assert_ne!(basic, "BASIC");

    let improved = String::from(Stealth::Improved);
    assert_ne!(improved, "improved");
    assert_ne!(improved, "IMPROVED");
  }

  #[test]
  fn test_sensors_to_string_conversion() {
    assert_eq!(String::from(Sensors::Basic), "Basic");
    assert_eq!(String::from(Sensors::Civilian), "Civilian");
    assert_eq!(String::from(Sensors::Military), "Military");
    assert_eq!(String::from(Sensors::Improved), "Improved");
    assert_eq!(String::from(Sensors::Advanced), "Advanced");
  }

  #[test]
  fn test_sensors_string_case_sensitivity() {
    // Verify that the strings match exactly, including case
    let improved = String::from(Sensors::Improved);
    assert_ne!(improved, "improved");
    assert_ne!(improved, "IMPROVED");

    let advanced = String::from(Sensors::Advanced);
    assert_ne!(advanced, "advanced");
    assert_ne!(advanced, "ADVANCED");
  }

  #[test]
  fn test_sensors_to_i32_and_string() {
    // Test both conversions for each variant
    let test_cases = vec![
      (Sensors::Basic, -4, "Basic"),
      (Sensors::Civilian, -2, "Civilian"),
      (Sensors::Military, 0, "Military"),
      (Sensors::Improved, 1, "Improved"),
      (Sensors::Advanced, 2, "Advanced"),
    ];

    for (sensor, expected_i32, expected_string) in test_cases {
      assert_eq!(i32::from(sensor), expected_i32);
      assert_eq!(String::from(sensor), expected_string);
    }
  }

  #[test]
  fn test_best_thrust() {
    let design = ShipDesignTemplate {
      name: "Test Ship".to_string(),
      displacement: 400,
      hull: 100,
      armor: 50,
      maneuver: 4,
      jump: 2,
      power: 250,
      fuel: 1000,
      crew: 20,
      sensors: Sensors::Military,
      stealth: None,
      countermeasures: None,
      computer: 10,
      crew_skills: None,
      weapons: vec![],
      screens: vec![],
      auxiliary: vec![],
      software: vec![],
      computer_bis: false,
      computer_fib: false,
      tl: 12,
      role: None,
      source: None,
    };

    // Test normal case
    assert_eq!(design.best_thrust(250), 4);

    // Test with reduced power
    assert_eq!(design.best_thrust(180), 2);

    // Test case where power calculation results in <= 0
    // With displacement 400, basic systems use 80 power (400/5)
    // Military sensors use 2 more power
    // Thrust 1 requires 40 more power
    // So providing 121 or less power should result in 0 thrust
    assert_eq!(design.best_thrust(122), 1);
    assert_eq!(design.best_thrust(121), 0);
    assert_eq!(design.best_thrust(0), 0);
  }

  #[test]
  fn test_weapon_type_is_laser() {
    // Test laser weapons
    assert!(WeaponType::Beam.is_laser());
    assert!(WeaponType::Pulse.is_laser());

    // Test non-laser weapons
    assert!(!WeaponType::Missile.is_laser());
    assert!(!WeaponType::Sand.is_laser());
    assert!(!WeaponType::Particle.is_laser());
  }

  #[test]
  fn test_ship_system_to_string() {
    let test_cases = vec![
      (ShipSystem::Hull, "hull"),
      (ShipSystem::Armor, "armor"),
      (ShipSystem::Jump, "jump drive"),
      (ShipSystem::Maneuver, "maneuver drive"),
      (ShipSystem::Powerplant, "power plant"),
      (ShipSystem::Crew, "crew"),
      (ShipSystem::Weapon, "a weapon"),
      (ShipSystem::Sensors, "sensors"),
      (ShipSystem::Fuel, "fuel"),
      (ShipSystem::Bridge, "bridge"),
      (ShipSystem::Cargo, "cargo"),
    ];

    for (system, expected) in test_cases {
      assert_eq!(String::from(system), expected);
    }
  }

  #[test_log::test]
  fn test_reset_temporary_bonuses_clears_evade_boost_used() {
    let mut ship = Ship::new(
      "TestShip".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Engineer flag also covered alongside evade for symmetry.
    ship.set_engineer_action_taken(true);
    ship.set_evade_boost_used(true);
    assert!(ship.has_engineer_action_taken());
    assert!(ship.has_evade_boost_used());

    ship.reset_temporary_bonuses();

    assert!(!ship.has_engineer_action_taken());
    assert!(!ship.has_evade_boost_used());
  }

  /// "+1 per Thrust" reads the loudest burn of the round and rounds down.
  #[test]
  fn thrust_in_g_takes_the_loudest_segment() {
    let design = Arc::new(ShipDesignTemplate::default());
    let mut ship = Ship::new("Test".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);

    // Drifting.
    ship.plan = FlightPlan::acceleration(Vec3::zero());
    assert_eq!(ship.thrust_in_g(), 0);

    // Just under 1G rounds down to nothing.
    ship.plan = FlightPlan::acceleration(Vec3::new(9.0, 0.0, 0.0));
    assert_eq!(ship.thrust_in_g(), 0);

    // Exactly 3G.
    ship.plan = FlightPlan::acceleration(Vec3::new(3.0 * crate::entity::G, 0.0, 0.0));
    assert_eq!(ship.thrust_in_g(), 3);

    // Two segments: the louder one is what a sensop notices, whichever order
    // they come in.
    ship.plan = FlightPlan::new(
      (Vec3::new(crate::entity::G, 0.0, 0.0), 100).into(),
      Some((Vec3::new(4.0 * crate::entity::G, 0.0, 0.0), 100).into()),
    );
    assert_eq!(ship.thrust_in_g(), 4);

    ship.plan = FlightPlan::new(
      (Vec3::new(4.0 * crate::entity::G, 0.0, 0.0), 100).into(),
      Some((Vec3::new(crate::entity::G, 0.0, 0.0), 100).into()),
    );
    assert_eq!(ship.thrust_in_g(), 4);
  }

  /// Heat is the sum of every critical the ship is carrying.
  #[test]
  fn crit_severity_sums_across_systems() {
    let design = Arc::new(ShipDesignTemplate::default());
    let mut ship = Ship::new("Test".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    assert_eq!(ship.total_crit_severity(), 0);

    ship.crit_level[0] = 2;
    ship.crit_level[5] = 3;
    assert_eq!(ship.total_crit_severity(), 5);
  }
}

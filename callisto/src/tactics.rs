//! Tactical verbs: what a commander says, turned into what a console sends.
//!
//! A commander -- the AI, or one day a human pressing "Intercept to Medium"
//! -- decides in coarse terms: close with that ship to Medium range, shoot
//! her drive, keep quiet. Nothing here decides anything. It does the
//! arithmetic a crew would do by hand and returns the same requests a crew's
//! consoles would send: a plan from the same solver the pilot's computer
//! uses, fire orders that name only guns that can reach, emissions set from a
//! posture.
//!
//! [`orders`] is the entry point. The helpers it uses to read the board --
//! [`band_between`], [`weapons_that_bear`] -- are public too, because a
//! commander needs the same answers to decide.

use cgmath::{InnerSpace, Zero};
use serde::{Deserialize, Serialize};

use crate::action::{BoostTarget, ShipAction};
use crate::combat::{
  attack_terms, find_range_band, point_defense_score, AttackSituation, HitMods, STANDARD_ROLL_THRESHOLD,
};
use crate::computer::{plot_course, PathMode};
use crate::entity::{detection_terms, emissions_of, Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME_F64, G};
use crate::payloads::{
  CaptainActionMsg, RequestMsg, SetPilotActions, SetPlanMsg, SetShipEmissions, SetSoftwareRunning,
};
use crate::rules_tables::{damage_multiple, profile_for, RANGE_BANDS};
use crate::ship::{
  uses_munitions, FlightPlan, Magazine, MountClass, PowerSystem, Range, Salvo, Ship, ShipSystem, Weapon, WeaponMount,
  WeaponProfile, WeaponType,
};
use crate::software::SoftwareKind;

/// How hard a ship tries not to be seen. A constraint over every other order,
/// not an order of its own.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
  /// Nothing radiates: no active sensors, no transmitting, no sharing.
  Silent,
  /// Listen, and share the picture with the team. Sharing is an emission.
  Passive,
  /// Sweep and lock: active sensors on.
  Active,
}

/// What the helm does this round.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub enum Helm {
  /// Keep the current plan.
  #[default]
  Hold,
  /// Close with a ship and arrive inside a range band, matching her velocity.
  Intercept { target: String, band: Range },
  /// Stay inside a range band of a ship, matching her velocity.
  HoldRange { target: String, band: Range },
  /// Burn straight away from a ship at full thrust.
  OpenRange { target: String },
  /// Go to a point and stop there. Metres, in the scenario's frame.
  Station { position: [f64; 3] },
  /// Fly in a squadron behind its guide: keep station on her while she flies
  /// `guide_helm`, held to `guide_cap` G. In station -- inside Short of her
  /// and drifting less than that in a round -- a follower flies the guide's
  /// own course, so the two stay together; out of it, she plots a rendezvous
  /// back to the guide, knowing the guide's burn because the guide is a friend.
  /// The guide flies `guide_helm` under her own order; this is for the rest.
  Follow {
    guide: String,
    guide_helm: Box<Helm>,
    guide_cap: Option<f64>,
  },
}

/// Fire on a ship.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Engage {
  pub target: String,
  /// Which mounts, by index. Empty means every one that can reach.
  #[serde(default)]
  pub weapons: Vec<usize>,
  /// A called shot on this system, for the guns that can call one.
  #[serde(default)]
  pub called: Option<ShipSystem>,
}

/// A sensor operator's order.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum SensorVerb {
  Lock { target: String },
  BreakLock { target: String },
  JamMissiles,
  JamComms { target: String },
}

/// An engineer's order.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum EngineerVerb {
  /// Repair the worst-damaged of these systems, in this order of preference.
  Repair {
    priority: Vec<ShipSystem>,
  },
  OverloadDrive,
  OverloadPlant,
  /// Shut a system down to free its Power, or bring it back (Offline
  /// System). Shutting down is an Engineer (power) check.
  Power {
    system: PowerSystem,
    online: bool,
  },
  /// Jump out: the ship leaves the fight. Needs the drive, fuel, Jump Control
  /// running, the computer and astrogation stations working, and clear
  /// space: 100 diameters from every body.
  Jump,
}

/// Something the captain's leadership points can be spent on: a +1 to one of
/// this ship's rolls this round. Listed in priority order; the captain's
/// check decides how many are bought.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Boost {
  /// One mount's attack.
  Fire { mount: usize },
  /// One mount's point-defence shot.
  PointDefence { mount: usize },
  /// One sensor operator's action.
  Sensor { operator: usize },
  /// One engineer's check.
  Engineer { engineer: usize },
  /// The pilot's dodge against the first attack.
  Dodge,
  /// The pilot's assist-gunners check.
  AssistGunners,
  /// The search for one ship not yet found.
  Detection { target: String },
}

/// Start or stop a program on the ship's computer. Free; takes effect at once,
/// within the computer's bandwidth.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SoftwareOrder {
  pub program: SoftwareKind,
  pub running: bool,
}

/// Flight operations: what a carrier does with her craft, and what a craft
/// does about her carrier. A commander may give one; left to the crews
/// ([`flight`]), a carrier launches when an enemy is within Very Long and a
/// craft goes home when her loads are spent, when nothing is left to fight,
/// or when her carrier is about to jump.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub enum Flight {
  /// A carrier launches: `craft` by name, or every ready craft aboard when
  /// empty (see [`ready_craft`]), the first `count` of them (the tubes'
  /// rate when `None`). More than the tubes take in a round wait aboard.
  Launch {
    #[serde(default)]
    craft: Vec<String>,
    #[serde(default)]
    count: Option<u32>,
  },
  /// A craft goes home: the helm closes on `carrier` and matches her
  /// velocity, and the pilot gives *Land* once she is within reach.
  Land { carrier: String },
  /// No flight operation: a carrier keeps her craft aboard; a craft stays
  /// out, going home only once her loads are spent, and any *Land* she had
  /// standing is withdrawn.
  Hold,
}

/// One ship's orders for a round, in a commander's terms.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Intent {
  /// `None` leaves emissions as they are.
  #[serde(default)]
  pub posture: Option<Posture>,
  #[serde(default)]
  pub helm: Helm,
  /// The most thrust the helm may use, in G, fractions allowed. Every G of
  /// burn is +1 to anyone looking for the ship, so a ship creeping up unseen
  /// burns gently. `None` uses the whole drive.
  #[serde(default)]
  pub max_thrust: Option<f64>,
  /// Thrust to hold back for dodging. `None` leaves it as it is.
  #[serde(default)]
  pub evade: Option<u8>,
  #[serde(default)]
  pub fire: Vec<Engage>,
  /// Mounts to put on point defence this round, by index: laser turrets,
  /// which then do not fire. Batteries defend on their own and sand is thrown
  /// on its own, so neither needs ordering.
  #[serde(default)]
  pub defend: Vec<usize>,
  /// Mounts to reload this round, by index, in the order spare gunners take
  /// them: launchers and sandcasters below their load. Once the spares are
  /// spoken for, a mount's own gunner reloads it, and it then neither fires
  /// nor defends. `None` leaves the choice to [`reloads`].
  #[serde(default)]
  pub reload: Option<Vec<usize>>,
  /// A flight operation: a carrier's launch, a craft's return to her
  /// carrier, or neither. `None` leaves it to [`flight`].
  #[serde(default)]
  pub flight: Option<Flight>,
  /// One per sensor operator aboard, in order; extras are dropped.
  #[serde(default)]
  pub sensors: Vec<SensorVerb>,
  /// One per engineer aboard, in order; extras are dropped.
  #[serde(default)]
  pub engineering: Vec<EngineerVerb>,
  /// Programs to stop or start. Stops are applied first, so a program can be
  /// swapped for another in one round.
  #[serde(default)]
  pub software: Vec<SoftwareOrder>,
  /// The pilot assists the gunners. `None` leaves it as it is.
  #[serde(default)]
  pub assist_gunners: Option<bool>,
  /// Systems for the Auto-Repair program to attempt, one per point it has.
  #[serde(default)]
  pub computer_repair: Vec<ShipSystem>,
  /// The captain's boosts, most wanted first. Empty: the captain does not
  /// make the check.
  #[serde(default)]
  pub boosts: Vec<Boost>,
  /// One sentence on why, for the people this ship reports to.
  #[serde(default)]
  pub rationale: String,
}

/// The range band two ships are in of each other.
#[must_use]
pub fn band_between(a: &Ship, b: &Ship) -> Range {
  band_at((a.get_position() - b.get_position()).magnitude())
}

/// The range band a distance in metres falls into.
#[must_use]
pub fn band_at(distance: f64) -> Range {
  // Saturating: anything past u32 metres is Distant anyway.
  #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
  find_range_band(distance.clamp(0.0, f64::from(u32::MAX)) as u32)
}

/// A distance comfortably inside a band, to aim for rather than its edge: a
/// quarter of the way in from its near side, so a little drift does not tip
/// the ship into the next band.
#[must_use]
pub fn aim_point(band: Range) -> f64 {
  let edges: Vec<f64> = std::iter::once(0.0)
    .chain(RANGE_BANDS.iter().map(|edge| f64::from(*edge)))
    .collect();
  let index = band as usize;
  match (edges.get(index), edges.get(index + 1)) {
    (Some(near), Some(far)) => near + (far - near) / 4.0,
    // Distant has no far side: aim a little beyond Very Long.
    _ => f64::from(RANGE_BANDS[RANGE_BANDS.len() - 1]) * 1.2,
  }
}

/// The edges of a band, in metres.
fn band_edges(band: Range) -> (f64, f64) {
  let edges: Vec<f64> = std::iter::once(0.0)
    .chain(RANGE_BANDS.iter().map(|edge| f64::from(*edge)))
    .collect();
  let index = band as usize;
  (
    edges.get(index).copied().unwrap_or(f64::MAX),
    edges.get(index + 1).copied().unwrap_or(f64::MAX),
  )
}

/// Whether a gun is an attack at all: point defence, sand and repulsors are
/// not fired at a ship.
fn offensive(kind: WeaponType) -> bool {
  !matches!(kind, WeaponType::Sand | WeaponType::PointDefense | WeaponType::Repulsor)
}

/// How many missiles one launch from this mount puts in flight, firing
/// `kind`, and how many points of a point-defence pool each one costs: a
/// torpedo two, a missile one. `None` if `kind` is not a launcher here.
#[must_use]
pub fn launch_size(weapon: &Weapon, kind: WeaponType) -> Option<(u32, u32)> {
  let firing = weapon.firing(kind)?;
  let missiles = match profile_for(kind, firing.mount)?.salvo? {
    Salvo::PerGun => u32::from(firing.count),
    Salvo::Fixed(missiles) => u32::from(missiles),
  };
  // A missile battery throws every launcher's salvo as one.
  let missiles = missiles * u32::from(weapon.mounts());
  Some((missiles, if kind == WeaponType::Torpedo { 2 } else { 1 }))
}

/// The mounts on `ship` that can fire on a target at this range, each with the
/// gun kind that would fire: its direct-fire guns if they reach, else its
/// launcher. Launchers always bear -- the salvo flies to its target. Mounts
/// switched off do not bear.
#[must_use]
pub fn weapons_that_bear(ship: &Ship, range: Range) -> Vec<(usize, WeaponType)> {
  ship
    .weapons()
    .iter()
    .enumerate()
    .filter(|(index, _)| ship.active_weapons.get(*index).copied().unwrap_or(true))
    .filter_map(|(index, weapon)| {
      weapon
        .kinds()
        .into_iter()
        .filter(|kind| offensive(*kind))
        .filter_map(|kind| {
          let firing = weapon.firing(kind)?;
          let profile = profile_for(kind, firing.mount)?.with_modifiers(kind, firing.modifiers);
          let reaches = profile.salvo.is_some() || profile.reaches(range);
          // A mount fires one kind of gun a round. Direct fire whenever it
          // reaches -- it lands this round, cannot be shot down, and a mixed
          // turret usually has more of it -- and the launcher only when
          // nothing else does. Among direct-fire guns, the most of them, then
          // the longest reach.
          let guns = weapon.guns.iter().filter(|gun| gun.kind == kind).count();
          reaches.then_some((profile.salvo.is_none(), guns, profile.max_range, kind))
        })
        .max_by(|a, b| {
          (a.0, a.1, a.2)
            .partial_cmp(&(b.0, b.1, b.2))
            .unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(_, _, _, kind)| (index, kind))
    })
    .collect()
}

/// What one mount can do, as a gunner's card would put it: the gun it
/// attacks with and that gun's profile in this mount, the mount's damage
/// multiple, and the reactions it offers.
#[derive(Debug, Clone, PartialEq)]
pub struct MountCard {
  /// The attacking gun: its direct-fire guns if it has any (most of them,
  /// then the longest reach), otherwise its launcher.
  /// `None` for a mount that cannot attack (sand alone, a battery).
  pub attack: Option<(WeaponType, WeaponProfile)>,
  /// How many of that gun the mount holds.
  pub guns: usize,
  /// A launcher in the same mount, fired instead when its direct-fire guns
  /// do not reach.
  pub launcher: Option<(WeaponType, WeaponProfile)>,
  /// Damage multiple from the mount: 1 for turrets, 3 for a barbette, 10 and
  /// up for bays.
  pub multiple: u32,
  /// Can be ordered to point defence: a turret with lasers in it.
  pub point_defence: bool,
  /// Carries a sandcaster, which reacts on its own.
  pub sand: bool,
  /// A point-defence battery, which defends on its own.
  pub battery: bool,
}

/// The card for one mount.
#[must_use]
pub fn mount_card(weapon: &Weapon) -> MountCard {
  let candidates: Vec<(WeaponType, WeaponProfile, usize)> = weapon
    .kinds()
    .into_iter()
    .filter(|kind| offensive(*kind))
    .filter_map(|kind| {
      let profile = profile_for(kind, &weapon.mount)?;
      Some((kind, profile, weapon.guns.iter().filter(|gun| gun.kind == kind).count()))
    })
    .collect();
  // As `weapons_that_bear` chooses: direct fire first (most guns, then the
  // longest reach), the launcher only when there is nothing else.
  let direct = candidates
    .iter()
    .filter(|(_, profile, _)| profile.salvo.is_none())
    .max_by_key(|(_, profile, guns)| (*guns, profile.max_range.map_or(0, |range| range as u8)));
  let launcher = candidates.iter().find(|(_, profile, _)| profile.salvo.is_some());
  let chosen = direct.or(launcher);
  MountCard {
    attack: chosen.map(|(kind, profile, _)| (*kind, *profile)),
    guns: chosen.map_or(0, |(_, _, guns)| *guns),
    launcher: if direct.is_some() {
      launcher.map(|(kind, profile, _)| (*kind, *profile))
    } else {
      None
    },
    multiple: damage_multiple(MountClass::from(&weapon.mount)),
    point_defence: point_defense_score(weapon) > 0,
    sand: weapon.has_kind(WeaponType::Sand),
    battery: weapon.has_kind(WeaponType::PointDefense),
  }
}

/// Every range band, nearest first.
pub const BANDS: [Range; 5] = [
  Range::Short,
  Range::Medium,
  Range::Long,
  Range::VeryLong,
  Range::Distant,
];

/// What one ship's guns can do to another at one range, in a round.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct BandOdds {
  pub band: Range,
  /// The best chance any one mount has to hit.
  pub best_chance: f64,
  /// Hull damage expected from every mount that bears, after armour,
  /// penetration and the mount's multiple. Launchers count every missile in
  /// the salvo, before point defence. Ion weapons, which take Power rather
  /// than hull, count nothing here.
  pub expected_damage: f64,
  /// How many mounts can fire at all.
  pub mounts: usize,
  /// Every mount that bears is a launcher.
  pub missiles_only: bool,
  /// Each bearing mount's share of `expected_damage`, by mount index: for
  /// sharing a ship's guns between targets. Not part of any wire format.
  #[serde(skip)]
  pub by_mount: Vec<(usize, f64)>,
}

/// The chance distribution of the sum of `dice` six-sided dice.
fn dice_distribution(dice: u8) -> Vec<f64> {
  (0..dice).fold(vec![1.0], |sums, _| {
    let mut next = vec![0.0; sums.len() + 6];
    for (sum, chance) in sums.iter().enumerate() {
      for face in 1..=6 {
        next[sum + face] += chance / 6.0;
      }
    }
    next
  })
}

/// Expected hull damage from one hit with this effect: the same arithmetic
/// `combat::attack` does, over every way the damage dice can fall. Armour
/// clips low rolls to nothing, which is why this is not the average roll
/// minus armour: a weak gun against a heavy hull is nearly harmless.
///
/// `percent` is the share armour lets through for a weapon that loses a
/// percentage a point (a particle or railgun spinal), and 100 for the rest,
/// which subtract `armour` instead; it is taken with the multiple, as
/// `combat::attack` takes it.
fn expected_hit_damage(dice: &[f64], effect: i32, armour: u32, turret_bonus: u32, multiple: u32, percent: u32) -> f64 {
  dice
    .iter()
    .enumerate()
    .map(|(roll, chance)| {
      #[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
      let raw = roll as i32 + effect - i32::try_from(armour).unwrap_or(i32::MAX);
      let through = u32::try_from(raw).unwrap_or(0);
      let damage = if through == 0 {
        0
      } else {
        u64::from(through + turret_bonus) * u64::from(multiple) * u64::from(percent) / 100
      };
      #[allow(clippy::cast_precision_loss)]
      let damage = damage as f64;
      chance * damage
    })
    .sum()
}

/// What `attacker`'s guns can do to `defender` at each range band in a round.
///
/// Built on `combat::attack_terms`, the sum the dice are rolled against, so
/// the odds are the rules' own. `gunnery` is each mount's gunner skill, by
/// mount; a side cannot see an enemy's crew, so for a contact the caller
/// passes what it assumes. `armour` is the defender's, likewise as known.
/// Dodging is not assumed: these are the odds against a ship flying straight.
#[must_use]
pub fn engagement(attacker: &Ship, defender: &Ship, gunnery: &dyn Fn(usize) -> i32, armour: u32) -> Vec<BandOdds> {
  let actual = band_between(attacker, defender);
  #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
  let metres = (attacker.get_position() - defender.get_position())
    .magnitude()
    .clamp(0.0, f64::from(u32::MAX)) as u32;
  BANDS
    .iter()
    .map(|band| {
      let bearing = weapons_that_bear(attacker, *band);
      let mut best_chance: f64 = 0.0;
      let mut expected = 0.0;
      let mut launchers = 0;
      // Mounts that can attack: a spinal bears on a fighter but cannot lay
      // on her (`combat::attack_terms`).
      let mut engaged = 0;
      let mut by_mount = Vec::with_capacity(bearing.len());
      for (index, kind) in &bearing {
        let weapon = &attacker.weapons()[*index];
        let Some(firing) = weapon.firing(*kind) else { continue };
        let Some(profile) = profile_for(*kind, firing.mount).map(|p| p.with_modifiers(*kind, firing.modifiers)) else {
          continue;
        };
        let situation = AttackSituation {
          hit: HitMods::gunner(gunnery(*index)),
          attacker,
          defender,
          firing: &firing,
          profile: &profile,
          range_band: *band,
          // Where they really are, for the band they are really in.
          distance: (actual == *band).then_some(metres),
          called_shot: false,
        };
        let Some(terms) = attack_terms(&situation, 0, 0) else {
          continue;
        };
        engaged += 1;
        let dm: i32 = terms.iter().map(|(_, value)| value).sum();
        let dice = dice_distribution(profile.damage_dice);
        let percent = profile.armour_through(armour);
        let armour = profile.armour_subtracted(armour);
        let (count, turret_bonus, multiple) = match profile.salvo {
          Some(Salvo::PerGun) => (f64::from(firing.count), 0, 1),
          Some(Salvo::Fixed(missiles)) => (f64::from(missiles), 0, 1),
          None => (
            1.0,
            if matches!(firing.mount, WeaponMount::Turret) {
              (u32::from(firing.count) - 1) * u32::from(profile.damage_dice)
            } else {
              0
            },
            if profile.use_multiple {
              damage_multiple(MountClass::from(firing.mount))
            } else {
              1
            },
          ),
        };
        if profile.salvo.is_some() {
          launchers += 1;
        }
        // Every way two dice can fall for the attack roll.
        let (chance, per_attack) = (2..=12_i32).fold((0.0, 0.0), |(chance, damage), roll| {
          let ways = f64::from(6 - (7 - roll).abs());
          let total = roll + dm;
          if total < STANDARD_ROLL_THRESHOLD {
            return (chance, damage);
          }
          let p = ways / 36.0;
          let hit = if profile.ion {
            0.0
          } else {
            // A missile's Effect adds nothing to its damage (see `combat::attack`).
            let effect = if profile.salvo.is_some() {
              0
            } else {
              total - STANDARD_ROLL_THRESHOLD
            };
            if percent == 0 {
              0.0
            } else {
              expected_hit_damage(&dice, effect, armour, turret_bonus, multiple, percent)
            }
          };
          (chance + p, damage + p * hit)
        });
        best_chance = best_chance.max(chance);
        // A battery is one entry and an attack per mount in it.
        let count = count * f64::from(weapon.mounts());
        expected += per_attack * count;
        by_mount.push((*index, per_attack * count));
      }
      BandOdds {
        band: *band,
        best_chance,
        expected_damage: expected,
        mounts: engaged,
        missiles_only: engaged > 0 && launchers == engaged,
        by_mount,
      }
    })
    .collect()
}

/// The chance two dice plus `dm` make the standard 8.
fn chance_of_success(dm: i32) -> f64 {
  (2..=12_i32)
    .filter(|roll| roll + dm >= STANDARD_ROLL_THRESHOLD)
    .map(|roll| f64::from(6 - (7 - roll).abs()) / 36.0)
    .sum()
}

/// How exposed a ship is to one observer, under one posture.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct DetectionOdds {
  pub posture: Posture,
  /// Whether the ship fires this round. Firing gives a ship away.
  pub firing: bool,
  /// The chance the observer finds the ship this round if it does not hold
  /// it already. Assumes the observer is sweeping with active sensors, since
  /// it can switch them on at will.
  pub found: f64,
  /// For an observer that already holds the ship: the chance of slipping
  /// away by opening the range a band. Zero for a ship with no stealth --
  /// only stealth can be lost.
  pub slip_by_opening: f64,
}

/// How exposed `target` is to `observer` this round, for each posture and
/// whether it fires, at its present burn and damage.
///
/// Built on the detection pass's own sum, so the odds are the rules' own.
/// Range does not enter the roll; Distant ends contact outright, so nothing
/// is found there.
#[must_use]
pub fn detection(observer: &Ship, target: &Ship) -> Vec<DetectionOdds> {
  let distant = band_between(observer, target) == Range::Distant;
  [Posture::Silent, Posture::Passive, Posture::Active]
    .into_iter()
    .flat_map(|posture| [false, true].map(|firing| (posture, firing)))
    .map(|(posture, firing)| {
      let mut emissions = emissions_of(target, firing);
      emissions.active_sensors = posture == Posture::Active;
      emissions.transmitting = posture != Posture::Silent;
      let dm: i32 = detection_terms(observer, target, emissions)
        .iter()
        .map(|(_, value)| i32::from(*value))
        .sum();
      let found = if distant { 0.0 } else { chance_of_success(dm) };
      let slip_by_opening = if target.design.stealth.is_some() {
        1.0 - chance_of_success(dm)
      } else {
        0.0
      };
      DetectionOdds {
        posture,
        firing,
        found,
        slip_by_opening,
      }
    })
    .collect()
}

/// A plan that arrives at `distance` from a ship, matching her velocity.
///
/// Her burn is used if this ship has contact on her, as the pilot's console
/// does: a blip's thrust is not known, so a course to one assumes she holds
/// her velocity (FAQ, "a course can be plotted toward a ship with no
/// contact").
fn rendezvous(own: &Ship, target: &Ship, distance: f64, thrust: f64) -> FlightPlan {
  let burn = own.detects(target).then_some(target.plan.0 .0);
  rendezvous_with(own, target, distance, thrust, burn)
}

/// A rendezvous, given what is known of the target's burn.
///
/// The computer assumes a burn goes on for the whole course. Against a ship
/// burning as hard as this one can, that leaves no rendezvous, and the
/// computer falls back to a pursuit that never brakes: ships closing at
/// hundreds of km/s and flashing through the fight. So if her burn gives only
/// a pursuit, plot against her velocity instead -- a course that burns, turns
/// and brakes, which every round's re-plot then corrects for her burn, as a
/// navigator would.
fn rendezvous_with(own: &Ship, target: &Ship, distance: f64, thrust: f64, burn: Option<Vec3>) -> FlightPlan {
  let start = own.get_position();
  let aim = target.get_position();
  let line = aim - start;
  let end = if line.magnitude() > distance && distance > 0.0 {
    aim - line.normalize() * distance
  } else if distance > 0.0 {
    // Already inside: hold station at the same bearing, further out.
    aim
      - (if line.magnitude() > 0.0 {
        line.normalize()
      } else {
        Vec3::unit_x()
      }) * distance
  } else {
    aim
  };
  let plot = |burn: Option<Vec3>| {
    plot_course(
      start,
      own.get_velocity(),
      thrust,
      end,
      target.get_velocity(),
      Some(target.get_velocity()),
      burn,
    )
  };
  let course = plot(burn);
  if burn.is_some() && course.mode != PathMode::Intercept {
    return plot(None).plan;
  }
  course.plan
}

/// The acceleration a helm order may use, in m/s²: the drive's, or less if
/// the commander capped it.
fn helm_thrust(own: &Ship, cap: Option<f64>, running: bool) -> f64 {
  // Running uses the whole drive: the dodge reserve and the gunner assist
  // are released for the round (see `orders`).
  let drive = if running {
    own.max_acceleration() + own.get_dodge_thrust() + u8::from(own.get_assist_gunners())
  } else {
    own.max_acceleration()
  };
  let drive = f64::from(drive);
  G * cap.map_or(drive, |cap| cap.clamp(0.0, drive))
}

/// Whether this helm order means running: burning straight away from a ship.
/// `OpenRange` always does; `HoldRange` does when she will be inside the
/// band's near edge next round.
fn running(world: &Entities, own: &Ship, helm: &Helm) -> bool {
  match helm {
    Helm::OpenRange { .. } => true,
    Helm::HoldRange { target, band } => world.ships.get(target).is_some_and(|target| {
      let target = target.read().unwrap();
      let (near, _) = band_edges(*band);
      let line = own.get_position() - target.get_position();
      let drift = own.get_velocity() - target.get_velocity();
      let next = (line + drift * DELTA_TIME_F64).magnitude();
      near > 0.0 && next.min(line.magnitude()) < near * 1.1
    }),
    _ => false,
  }
}

fn helm_plan(world: &Entities, own: &Ship, helm: &Helm, cap: Option<f64>) -> Result<Option<FlightPlan>, String> {
  let thrust = helm_thrust(own, cap, running(world, own, helm));
  if thrust <= 0.0 && !matches!(helm, Helm::Hold) {
    // Capped at nothing: coast.
    return Ok(Some(FlightPlan::new((Vec3::zero(), DEFAULT_ACCEL_DURATION).into(), None)));
  }
  let ship = |name: &str| -> Result<Ship, String> {
    world
      .ships
      .get(name)
      .map(|ship| ship.read().unwrap().clone())
      .ok_or_else(|| format!("no ship named {name}"))
  };
  Ok(match helm {
    Helm::Hold => None,
    Helm::Intercept { target, band } => Some(rendezvous(own, &ship(target)?, aim_point(*band), thrust)),
    Helm::HoldRange { target, band } => {
      let target = ship(target)?;
      let (near, far) = band_edges(*band);
      let now = (own.get_position() - target.get_position()).magnitude();
      // Where the range will be next round if neither burns. If she will
      // be inside the band's near edge, a rendezvous is always a round
      // behind a ship coming on: burn straight away until the range opens,
      // as a pilot would, and match her only once it has.
      let line = own.get_position() - target.get_position();
      if running(world, own, helm) {
        let direction = if line.magnitude() > 0.0 {
          line.normalize()
        } else {
          Vec3::unit_x()
        };
        return Ok(Some(FlightPlan::new((direction * thrust, DEFAULT_ACCEL_DURATION).into(), None)));
      }
      // Where we are, if that is inside the band with room to spare;
      // otherwise the band's aim point.
      let keep = if now > near * 1.1 && now < far * 0.9 {
        now
      } else {
        aim_point(*band)
      };
      Some(rendezvous(own, &target, keep, thrust))
    }
    Helm::OpenRange { target } => {
      let away = own.get_position() - ship(target)?.get_position();
      let direction = if away.magnitude() > 0.0 {
        away.normalize()
      } else {
        Vec3::unit_x()
      };
      let accel = direction * thrust;
      Some(FlightPlan::new((accel, DEFAULT_ACCEL_DURATION).into(), None))
    }
    Helm::Station { position } => {
      let end = Vec3::new(position[0], position[1], position[2]);
      Some(plot_course(own.get_position(), own.get_velocity(), thrust, end, Vec3::zero(), None, None).plan)
    }
    Helm::Follow {
      guide,
      guide_helm,
      guide_cap,
    } => Some(follow_plan(world, own, &ship(guide)?, guide_helm, *guide_cap, thrust)?),
  })
}

/// A follower's course: the guide's own, while in station; a rendezvous back
/// to her otherwise.
fn follow_plan(
  world: &Entities, own: &Ship, guide: &Ship, guide_helm: &Helm, guide_cap: Option<f64>, thrust: f64,
) -> Result<FlightPlan, String> {
  if matches!(guide_helm, Helm::Follow { .. }) {
    return Err(format!("{} cannot guide while following", guide.get_name()));
  }
  // The guide's course this round, worked out as her own order will be.
  let course = helm_plan(world, guide, guide_helm, guide_cap)?.unwrap_or_else(|| guide.plan.clone());
  let (_, short) = band_edges(Range::Short);
  let apart = (own.get_position() - guide.get_position()).magnitude();
  let drift = (own.get_velocity() - guide.get_velocity()).magnitude() * DELTA_TIME_F64;
  // Matched: close by and drifting less than a kilometre a round. Only then
  // is copying her course enough; any more drift and copying carries it
  // forward round after round until it takes her out of station, so a
  // follower keeps making small corrections for as long as she is not quite
  // matched -- with whatever thrust is spare, however little.
  if apart <= short && drift <= 1_000.0 {
    // In station: fly her course, no harder than this ship can.
    let scale = |accel: Vec3| {
      let magnitude = accel.magnitude();
      if magnitude > thrust && magnitude > 0.0 {
        accel * (thrust / magnitude)
      } else {
        accel
      }
    };
    let first = (scale(course.0 .0), course.0 .1);
    let second = course.1.as_ref().map(|leg| (scale(leg.0), leg.1));
    return Ok(FlightPlan::new(first.into(), second.map(Into::into)));
  }
  // Out of station: close on her in her own frame. She is burning, and a
  // rendezvous plotted in the system's frame has to allow for that burn for
  // the whole course -- which the computer cannot always do, and a course that
  // leaves it out is a round's burn wrong every round, so the follower swings
  // through her guide and out the other side. In her frame she is standing
  // still: fly her burn, plus a rendezvous with a point beside her made with
  // the thrust left over. The guide is held a G below the slowest of her
  // squadron, so there is always some.
  let burn = course.0 .0;
  // Exactly what is left over, so the plan is one she can fly: planning
  // with more than she has undershoots every round, and she never closes.
  let spare = (thrust - burn.magnitude()).max(G / 100.0);
  let offset = own.get_position() - guide.get_position();
  let beside = if offset.magnitude() > 0.0 {
    offset.normalize() * (short / 2.0)
  } else {
    Vec3::unit_x() * (short / 2.0)
  };
  let correction = plot_course(
    offset,
    own.get_velocity() - guide.get_velocity(),
    spare,
    beside,
    Vec3::zero(),
    None,
    None,
  )
  .plan;
  // The correction spread evenly over the round: the same change of
  // velocity by the round's end, and never a part of the round left
  // coasting. A rendezvous with the thrust left over can finish inside a
  // round, and a plan holds only two legs, so flying it as plotted left the
  // follower coasting for the rest of the round while her guide burned on.
  // Re-plotted every round, so the averaging costs nothing over time.
  let round = DELTA_TIME_F64;
  // Leg durations are seconds, far inside f64's exact range.
  #[allow(clippy::cast_precision_loss)]
  let first_time = (correction.0 .1 as f64).min(round);
  #[allow(clippy::cast_precision_loss)]
  let second_time = correction.1.as_ref().map_or(0.0, |leg| (leg.1 as f64).min(round - first_time));
  let nudge =
    (correction.0 .0 * first_time + correction.1.as_ref().map_or(Vec3::zero(), |leg| leg.0) * second_time) / round;
  let scale = |accel: Vec3| {
    if accel.magnitude() > thrust && accel.magnitude() > 0.0 {
      accel * (thrust / accel.magnitude())
    } else {
      accel
    }
  };
  // If the guide turns this round -- her first leg ends inside it -- turn
  // with her: a follower still flying her old burn after she has turned is
  // off by twice her thrust for the rest of the round.
  match &course.1 {
    Some(then) if course.0 .1 < DELTA_TIME_ROUND => Ok(FlightPlan::new(
      (scale(burn + nudge), course.0 .1).into(),
      Some((scale(then.0 + nudge), DEFAULT_ACCEL_DURATION).into()),
    )),
    // The nudge for this round only, then the guide's burn: if no fresh
    // order comes next round -- the model fails to answer -- she flies on
    // with her guide rather than nudging for ever.
    _ => Ok(FlightPlan::new(
      (scale(burn + nudge), DELTA_TIME_ROUND).into(),
      Some((scale(burn), course.0 .1.saturating_sub(DELTA_TIME_ROUND).max(1)).into()),
    )),
  }
}

/// A round, in whole seconds.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
const DELTA_TIME_ROUND: u64 = DELTA_TIME_F64 as u64;

fn emissions(ship: &str, posture: Posture) -> RequestMsg {
  let (active, transmitting, handoff) = match posture {
    Posture::Silent => (false, false, false),
    Posture::Passive => (false, true, true),
    Posture::Active => (true, true, true),
  };
  RequestMsg::SetShipEmissions(SetShipEmissions {
    ship_name: ship.to_string(),
    active_sensors: Some(active),
    transmitting: Some(transmitting),
    handoff_sensors: Some(handoff),
  })
}

fn sensor_action(verb: &SensorVerb, operator: usize) -> ShipAction {
  match verb {
    SensorVerb::Lock { target } => ShipAction::SensorLock {
      target: target.clone(),
      operator,
    },
    SensorVerb::BreakLock { target } => ShipAction::BreakSensorLock {
      target: target.clone(),
      operator,
    },
    SensorVerb::JamMissiles => ShipAction::JamMissiles { operator },
    SensorVerb::JamComms { target } => ShipAction::JamComms {
      target: target.clone(),
      operator,
    },
  }
}

fn engineer_action(own: &Ship, verb: &EngineerVerb, engineer: usize) -> Option<ShipAction> {
  match verb {
    EngineerVerb::Repair { priority } => {
      // The worst-damaged system on the list; ties go to the earlier one.
      let worst = priority
        .iter()
        .map(|system| (own.crit_level[*system as usize], *system))
        .filter(|(level, _)| *level > 0)
        .fold(None::<(u8, ShipSystem)>, |best, candidate| match best {
          Some(best) if best.0 >= candidate.0 => Some(best),
          _ => Some(candidate),
        })?;
      Some(ShipAction::Repair {
        system: worst.1,
        engineer,
      })
    }
    EngineerVerb::OverloadDrive => Some(ShipAction::OverloadDrive { engineer }),
    EngineerVerb::Jump => Some(ShipAction::Jump { engineer }),
    EngineerVerb::Power { system, online } => Some(ShipAction::SetPower {
      system: *system,
      online: *online,
      engineer,
    }),
    EngineerVerb::OverloadPlant => Some(ShipAction::OverloadPlant { engineer }),
  }
}

/// The helm's and the pilot's requests: the release of any held-back thrust
/// when running, then the plan, then the pilot's standing orders.
fn helm_requests(world: &Entities, own: &Ship, ship_name: &str, intent: &Intent) -> Result<Vec<RequestMsg>, String> {
  let mut requests = Vec::new();
  // Running means the whole drive: a pilot told to get away does not keep
  // a G back to dodge or to steady the guns. Sent before the plan, which the
  // drive less any reserve would otherwise refuse.
  let run = running(world, own, &intent.helm);
  if run {
    let mut pilot = SetPilotActions::new(ship_name);
    pilot.dodge_thrust = Some(0);
    pilot.assist_gunners = Some(false);
    requests.push(RequestMsg::SetPilotActions(pilot));
  }
  if let Some(plan) = helm_plan(world, own, &intent.helm, intent.max_thrust)? {
    requests.push(RequestMsg::SetPlan(SetPlanMsg {
      name: ship_name.to_string(),
      plan,
    }));
  }
  requests.extend(software_requests(own, ship_name, &intent.software));
  if !run && (intent.evade.is_some() || intent.assist_gunners.is_some()) {
    let mut pilot = SetPilotActions::new(ship_name);
    pilot.dodge_thrust = intent.evade.map(|thrust| thrust.min(own.max_acceleration()));
    pilot.assist_gunners = intent.assist_gunners;
    requests.push(RequestMsg::SetPilotActions(pilot));
  }

  Ok(requests)
}

/// The requests that carry out one ship's intent for the round.
///
/// Fire orders name only mounts that can reach their target; a target that is
/// not in this ship's contacts is not fired on, since its gunners could not
/// pick it. Sensor and engineering orders beyond the crew aboard are dropped.
///
/// # Errors
/// Returns an error if the ship, or a ship its helm order names, does not
/// exist.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
pub fn orders(world: &Entities, ship_name: &str, intent: &Intent) -> Result<Vec<RequestMsg>, String> {
  let own = world
    .ships
    .get(ship_name)
    .map(|ship| ship.read().unwrap().clone())
    .ok_or_else(|| format!("no ship named {ship_name}"))?;
  let mut requests = Vec::new();

  if let Some(posture) = intent.posture {
    requests.push(emissions(ship_name, posture));
  }
  // A craft going home flies to her carrier, whatever else she was told.
  let flight = resolve_flight(world, &own, intent);
  let homing = homing_intent(world, &own, intent, flight.as_ref());
  let intent = homing.as_ref().unwrap_or(intent);
  requests.extend(helm_requests(world, &own, ship_name, intent)?);

  let mut actions = flight_actions(world, &own, flight.as_ref());
  let mut committed = std::collections::HashSet::new();
  // Reloads first. Spare gunners take them in order, and a mount whose own
  // gunner reloads it is spoken for: it neither fires nor defends.
  let mut reload: Vec<usize> = Vec::new();
  for weapon_id in intent.reload.clone().unwrap_or_else(|| reloads(world, &own, intent)) {
    if own.can_reload(weapon_id) && !reload.contains(&weapon_id) {
      reload.push(weapon_id);
    }
  }
  committed.extend(reload.iter().skip(own.spare_gunners()).copied());
  actions.extend(reload.iter().map(|&weapon_id| ShipAction::ReloadAction { weapon_id }));
  // Defence next: a mount on point defence is spoken for before any fire
  // order can claim it.
  for &weapon_id in &intent.defend {
    let can_defend = own
      .weapons()
      .get(weapon_id)
      .is_some_and(|weapon| point_defense_score(weapon) > 0);
    if can_defend && committed.insert(weapon_id) {
      actions.push(ShipAction::PointDefenseAction {
        weapon_id,
        protecting: None,
      });
    }
  }
  for engage in &intent.fire {
    if !own.contacts.contains(&engage.target) {
      continue;
    }
    let Some(target) = world.ships.get(&engage.target) else {
      continue;
    };
    let target = target.read().unwrap();
    let range = band_between(&own, &target);
    // A called shot is only offered at Short range, as the gunner's console
    // offers it; further out the gun fires as an ordinary shot.
    let called = engage.called.filter(|_| range == Range::Short);
    for (weapon_id, kind) in weapons_that_bear(&own, range) {
      let chosen = (engage.weapons.is_empty() || engage.weapons.contains(&weapon_id))
        && can_engage(&own, weapon_id, kind, &target);
      // A mount fires once a round, at whoever asked for it first.
      if chosen && committed.insert(weapon_id) {
        actions.push(ShipAction::FireAction {
          weapon_id,
          target: engage.target.clone(),
          called_shot_system: called,
          firing_kind: Some(kind),
          salvo_size: None,
          fire_control_dm: 0,
          computer_fired: false,
        });
      }
    }
  }
  let operators = own.get_crew().sensor_operators().max(1);
  actions.extend(
    intent
      .sensors
      .iter()
      .take(operators)
      .enumerate()
      .map(|(operator, verb)| sensor_action(verb, operator)),
  );
  let engineers = own.get_crew().engineers().len().max(1);
  actions.extend(
    intent
      .engineering
      .iter()
      .take(engineers)
      .enumerate()
      .filter_map(|(engineer, verb)| engineer_action(&own, verb, engineer)),
  );
  actions.extend(
    intent
      .computer_repair
      .iter()
      .filter(|system| own.crit_level[**system as usize] > 0)
      .map(|system| ShipAction::ComputerRepair { system: *system }),
  );
  if !intent.boosts.is_empty() && own.get_crew().get_leadership() > 0 {
    requests.push(RequestMsg::CaptainAction(CaptainActionMsg {
      ship_name: ship_name.to_string(),
    }));
    actions.push(ShipAction::LeadershipCheck {
      boosts: intent.boosts.iter().map(|boost| boost_target(ship_name, boost)).collect(),
    });
  }
  spend_fire_control(&own, &mut actions);
  if !actions.is_empty() {
    requests.push(RequestMsg::ModifyActions(vec![(ship_name.to_string(), actions)]));
  }
  Ok(requests)
}

/// Whether mount `weapon_id`, firing `kind`, can be laid on `target` at all,
/// beyond bearing on her: a spinal mount cannot attack a ship under 2,000
/// tons that manoeuvred last round (High Guard p. 34), nor fire with its
/// rounds spent. Left out, a spinal stays free for a target it can hit.
fn can_engage(own: &Ship, weapon_id: usize, kind: WeaponType, target: &Ship) -> bool {
  if own.get_weapon(weapon_id).mount.spinal_multiple().is_none() {
    return true;
  }
  let loaded = !crate::ship::spinal_uses_rounds(kind) || own.magazine.rounds > 0;
  spinal_can_lay_on(target.design.displacement, target.held_still) && loaded
}

/// Whether a spinal mount can be laid on a ship of `displacement` tons at
/// all: not under 2,000 tons unless she held still last round (High Guard
/// p. 34). The rule `combat::attack_terms` applies, for those that must know
/// it before firing.
#[must_use]
pub fn spinal_can_lay_on(displacement: u32, held_still: bool) -> bool {
  crate::rules_tables::spinal_size_dm(displacement).is_some() || held_still
}

/// The mounts `intent`'s fire orders would use, each with the gun it fires:
/// the first engagement to claim a mount has it, as [`orders`] gives them.
fn engaged_mounts(world: &Entities, own: &Ship, intent: &Intent) -> Vec<(usize, WeaponType)> {
  let mut engaged: Vec<(usize, WeaponType)> = Vec::new();
  for engage in &intent.fire {
    if !own.contacts.contains(&engage.target) {
      continue;
    }
    let Some(target) = world.ships.get(&engage.target) else {
      continue;
    };
    let target = target.read().unwrap();
    let range = band_between(own, &target);
    for (weapon_id, kind) in weapons_that_bear(own, range) {
      let chosen =
        (engage.weapons.is_empty() || engage.weapons.contains(&weapon_id)) && can_engage(own, weapon_id, kind, &target);
      if chosen && engaged.iter().all(|(id, _)| *id != weapon_id) {
        engaged.push((weapon_id, kind));
      }
    }
  }
  engaged
}

/// Which mounts to reload this round, when a commander leaves it to tactics.
///
/// While spare gunners are free, every launcher and sandcaster below its load,
/// emptiest first: it costs the mount nothing. With none free, a mount's own
/// gunner reloads only where that gives up nothing worth having: an empty
/// launcher or sandcaster, or a partly empty one with nothing to fire on (for
/// sand, no fight this round). A mixed mount -- a rack beside a laser -- is
/// reloaded only when the rest of it has no target.
#[must_use]
pub fn reloads(world: &Entities, own: &Ship, intent: &Intent) -> Vec<usize> {
  let mut candidates: Vec<(usize, f64)> = (0..own.weapons().len())
    .filter(|&weapon_id| own.active_weapons.get(weapon_id).copied().unwrap_or(true) && own.can_reload(weapon_id))
    .map(|weapon_id| {
      let held = own.loaded(weapon_id);
      let full = own.capacity(weapon_id);
      let share = f64::from(held.missiles + held.torpedoes + held.sand)
        / f64::from((full.missiles + full.torpedoes + full.sand).max(1));
      (weapon_id, share)
    })
    .collect();
  candidates.sort_by(|a, b| a.1.total_cmp(&b.1));
  let spares = own.spare_gunners();
  let engaged = engaged_mounts(world, own, intent);
  let fighting = !engaged.is_empty();
  candidates
    .into_iter()
    .enumerate()
    .filter(|&(place, (weapon_id, _))| {
      if place < spares {
        return true;
      }
      if intent.defend.contains(&weapon_id) {
        return false;
      }
      let weapon = own.get_weapon(weapon_id);
      let held = own.loaded(weapon_id);
      let using = engaged.iter().find(|(id, _)| *id == weapon_id).map(|(_, kind)| *kind);
      let mixed = weapon.guns.iter().any(|gun| !uses_munitions(gun.kind));
      match using {
        // The other gun of a mixed mount has a target: keep it firing.
        Some(kind) if !uses_munitions(kind) => false,
        // A launcher with a target is reloaded only once it is empty.
        Some(kind) => held.of(kind) == 0,
        // Sand is a reaction, wanted whenever there is a fight.
        None if !mixed && weapon.has_kind(WeaponType::Sand) && fighting => held.sand == 0,
        None => true,
      }
    })
    .map(|(_, (weapon_id, _))| weapon_id)
    .collect()
}

/// Whether `own` has an enemy in contact within `band`: a contact on another
/// team (any contact, for a ship on no team) that close by true position.
fn enemy_within(world: &Entities, own: &Ship, band: Range) -> bool {
  own
    .contacts
    .iter()
    .filter_map(|name| world.ships.get(name))
    .map(|ship| ship.read().unwrap())
    .any(|ship| (own.team.is_none() || ship.team != own.team) && band_between(own, &ship) <= band)
}

/// The guns a ship attacks with.
fn attacking_kinds(ship: &Ship) -> Vec<WeaponType> {
  ship
    .weapons()
    .iter()
    .flat_map(Weapon::kinds)
    .filter(|kind| offensive(*kind))
    .collect()
}

/// Whether the ship's launchers have nothing left to fire: she has one, and
/// neither her mounts nor her stores hold a missile or a torpedo. A ship
/// with no launcher is never spent, whatever else she carries.
#[must_use]
pub fn loads_spent(ship: &Ship) -> bool {
  let aboard = ship.aboard();
  attacking_kinds(ship).iter().any(|kind| uses_munitions(*kind)) && aboard.missiles == 0 && aboard.torpedoes == 0
}

/// Whether `carrier`'s stores hold anything `craft`'s mounts could take.
fn can_rearm(carrier: &Ship, craft: &Ship) -> bool {
  let capacity = (0..craft.weapons().len()).fold(Magazine::default(), |sum, mount| sum.plus(craft.capacity(mount)));
  !carrier.magazine.min(capacity).is_empty()
}

/// Whether the ship can do nothing out there: her launchers are spent and
/// she has no other gun.
fn nothing_to_fight_with(ship: &Ship) -> bool {
  loads_spent(ship) && attacking_kinds(ship).iter().all(|kind| uses_munitions(*kind))
}

/// Whether `ship` is about to jump: Jump Control is running, or this
/// round's orders start it or make the jump.
fn leaving(ship: &Ship, intent: &Intent) -> bool {
  ship.running_level(SoftwareKind::JumpControl).is_some()
    || intent.engineering.iter().any(|verb| matches!(verb, EngineerVerb::Jump))
    || intent
      .software
      .iter()
      .any(|order| order.running && order.program == SoftwareKind::JumpControl)
}

/// The craft aboard `carrier` fit to launch, wing by wing: not already in
/// a flight operation, not waiting on the stores to rearm her, and with
/// something to fight with. In squadron order, then by name, so a round's
/// launches come from one wing before the next.
#[must_use]
pub fn ready_craft(carrier: &Ship) -> Vec<String> {
  let mut ready: Vec<(String, String)> = carrier
    .embarked
    .iter()
    .enumerate()
    .filter(|(index, craft)| {
      !carrier.in_flight_op(craft.get_name()) && !carrier.craft_wants_rearming(*index) && !nothing_to_fight_with(craft)
    })
    .map(|(_, craft)| (craft.squadron.clone().unwrap_or_default(), craft.get_name().to_string()))
    .collect();
  ready.sort();
  ready.into_iter().map(|(_, name)| name).collect()
}

/// The crews' own flight operation, when a commander gives none.
///
/// A carrier with craft aboard launches them when an enemy is in contact
/// within Very Long, unless she is pinned or about to jump. A craft goes
/// home when her launchers are spent and her carrier can refill them, when
/// no enemy is in contact at all, or when her carrier is about to jump; a
/// craft already being taken in (pinned) is left to it. Otherwise nothing.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn flight(world: &Entities, own: &Ship, intent: &Intent) -> Option<Flight> {
  if let Some(carrier) = own.carrier.as_ref().filter(|name| world.ships.contains_key(*name)) {
    if own.pinned {
      return None;
    }
    let home = {
      let home = world.ships[carrier].read().unwrap();
      (loads_spent(own) && can_rearm(&home, own))
        || !enemy_within(world, own, Range::Distant)
        || home.running_level(SoftwareKind::JumpControl).is_some()
    };
    return home.then(|| Flight::Land {
      carrier: carrier.clone(),
    });
  }
  if own.embarked.is_empty() || own.pinned || leaving(own, intent) {
    return None;
  }
  enemy_within(world, own, Range::VeryLong).then_some(Flight::Launch {
    craft: Vec::new(),
    count: None,
  })
}

/// A carrier burning harder than this is not taking craft aboard: the
/// landing check is DM-1 per G she spends (HG p. 104), so a craft waits on
/// station until she eases off.
const LANDING_CARRIER_G: f64 = 1.0;

/// Whether `own` is in a position to land on `carrier` at the round's end
/// if neither burns: within Short and inside the velocity limit now.
fn on_the_approach(own: &Ship, carrier: &Ship) -> bool {
  let closing = (own.get_velocity() - carrier.get_velocity()).magnitude();
  band_between(own, carrier) <= Range::Short && closing <= crate::flight::landing_velocity_limit(own.current_maneuver)
}

/// The squadron whose turn it is to land on `carrier`: of her craft on the
/// approach, the first squadron by name (a craft in none counts as her own).
/// The landing check loses a point for every other craft coming in that is
/// not of the same squadron (HG p. 104), so the wings take turns.
fn wing_landing_now(world: &Entities, carrier_name: &str, carrier: &Ship) -> Option<String> {
  world
    .ships
    .values()
    .map(|ship| ship.read().unwrap())
    .filter(|ship| ship.carrier.as_deref() == Some(carrier_name) && on_the_approach(ship, carrier))
    .map(|ship| wing_key(&ship))
    .min()
}

fn wing_key(ship: &Ship) -> String {
  ship.squadron.clone().unwrap_or_else(|| ship.get_name().to_string())
}

/// `intent` with the helm closing on the carrier and matching her velocity,
/// for a craft going home to one that is still there -- coasting once she is
/// on the approach, since every G she spends counts against the landing
/// check; `None` otherwise.
fn homing_intent(world: &Entities, own: &Ship, intent: &Intent, flight: Option<&Flight>) -> Option<Intent> {
  match flight {
    Some(Flight::Land { carrier }) => world.ships.get(carrier).map(|home| Intent {
      helm: Helm::Intercept {
        target: carrier.clone(),
        band: Range::Short,
      },
      max_thrust: on_the_approach(own, &home.read().unwrap()).then_some(0.0),
      ..intent.clone()
    }),
    _ => None,
  }
}

/// The flight operation `intent` means: the commander's own, or the crews'.
/// A craft told to hold still goes home once her launchers are spent and
/// her carrier can refill them: rearming is the crews' cycle, not an order.
fn resolve_flight(world: &Entities, own: &Ship, intent: &Intent) -> Option<Flight> {
  match &intent.flight {
    Some(Flight::Hold) if loads_spent(own) => own
      .carrier
      .as_ref()
      .and_then(|name| world.ships.get(name))
      .filter(|home| can_rearm(&home.read().unwrap(), own))
      .and(own.carrier.clone())
      .map(|carrier| Flight::Land { carrier }),
    Some(flight) => Some(flight.clone()),
    None => flight(world, own, intent),
  }
}

/// The actions a flight operation needs this round: the carrier's *Launch*
/// of the craft the tubes can take; a craft's *Land* once she is on the
/// approach (within Short, matched), her carrier is not burning hard --
/// since a check made under a heavy burn fails and damages both ships --
/// and it is her wing's turn; and
/// for a craft not going home, the withdrawal of any *Land* she had
/// standing, since the engine keeps one until she is aboard.
fn flight_actions(world: &Entities, own: &Ship, flight: Option<&Flight>) -> Vec<ShipAction> {
  match flight {
    Some(Flight::Launch { craft, count }) => {
      let chosen = launch_list(own, craft, *count);
      if chosen.is_empty() {
        Vec::new()
      } else {
        vec![ShipAction::Launch { craft: chosen }]
      }
    }
    Some(Flight::Land { carrier }) => world
      .ships
      .get(carrier)
      .filter(|home| {
        let home = home.read().unwrap();
        on_the_approach(own, &home)
          && home.plan.0 .0.magnitude() <= LANDING_CARRIER_G * G
          && wing_landing_now(world, carrier, &home).as_ref() == Some(&wing_key(own))
      })
      .map(|_| ShipAction::Land {
        carrier: carrier.clone(),
      })
      .into_iter()
      .collect(),
    _ if own.carrier.is_some() => vec![ShipAction::ClearLand],
    _ => Vec::new(),
  }
}

/// The craft a *Launch* names this round: those asked for that are aboard
/// and not already on their way out (every ready craft when none are
/// named), the first `count`, and no more than the tubes release in a round
/// when she has tubes for them.
fn launch_list(own: &Ship, named: &[String], count: Option<u32>) -> Vec<String> {
  let mut chosen: Vec<String> = if named.is_empty() {
    ready_craft(own)
  } else {
    let mut seen = std::collections::HashSet::new();
    named
      .iter()
      .filter(|name| own.craft_aboard(name).is_some() && !own.in_flight_op(name) && seen.insert(name.as_str()))
      .cloned()
      .collect()
  };
  let rate = chosen
    .iter()
    .filter_map(|name| own.craft_aboard(name))
    .map(|craft| own.launch_per_round(craft.design.displacement))
    .max()
    .unwrap_or(0);
  let limit = match (count, rate) {
    (Some(count), 0) => count,
    (Some(count), rate) => count.min(rate),
    (None, 0) => u32::MAX,
    (None, rate) => rate,
  };
  chosen.truncate(usize::try_from(limit).unwrap_or(usize::MAX));
  chosen
}

/// Start and stop programs: stops first, so the bandwidth is free for what
/// starts. Each names the package of that kind aboard at its highest level.
fn software_requests(own: &Ship, ship_name: &str, orders: &[SoftwareOrder]) -> Vec<RequestMsg> {
  let mut requests = Vec::new();
  // Software: stops first, so the bandwidth is free for what starts.
  let mut software: Vec<&SoftwareOrder> = orders.iter().collect();
  software.sort_by_key(|order| order.running);
  for order in software {
    // The package aboard of that kind, at its highest level.
    let Some(package) = own
      .software
      .iter()
      .filter(|package| package.kind == order.program)
      .max_by_key(|package| package.level)
    else {
      continue;
    };
    requests.push(RequestMsg::SetSoftwareRunning(SetSoftwareRunning {
      ship_name: ship_name.to_string(),
      software: *package,
      running: order.running,
    }));
  }
  requests
}

fn boost_target(ship: &str, boost: &Boost) -> BoostTarget {
  let ship = ship.to_string();
  match boost {
    Boost::Fire { mount } => BoostTarget::Fire {
      ship,
      weapon_id: *mount,
    },
    Boost::PointDefence { mount } => BoostTarget::PointDefense {
      ship,
      weapon_id: *mount,
    },
    Boost::Sensor { operator } => BoostTarget::Sensor {
      ship,
      operator: *operator,
    },
    Boost::Engineer { engineer } => BoostTarget::Engineer {
      ship,
      engineer: *engineer,
    },
    Boost::Dodge => BoostTarget::Evade { ship },
    Boost::AssistGunners => BoostTarget::AssistGunner { ship },
    Boost::Detection { target } => BoostTarget::Detection {
      ship,
      target: target.clone(),
    },
  }
}

/// Spend the Fire Control program's points as to-hit DMs, spread over this
/// round's fire orders: a point each, round robin, until they run out. The
/// console lets a gunner choose; a commander would not want to.
fn spend_fire_control(own: &Ship, actions: &mut [ShipAction]) {
  let mut pool = own.running_level(SoftwareKind::FireControl).unwrap_or(0);
  // A mount nobody is at fires only if the computer fires it, for a point.
  for action in actions.iter_mut() {
    if let ShipAction::FireAction {
      weapon_id,
      computer_fired,
      ..
    } = action
    {
      if pool > 0 && !own.get_crew().has_gunner(*weapon_id) {
        *computer_fired = true;
        pool -= 1;
      }
    }
  }
  let firing = actions
    .iter()
    .filter(|action| matches!(action, ShipAction::FireAction { .. }))
    .count();
  if firing == 0 {
    return;
  }
  while pool > 0 {
    let before = pool;
    for action in actions.iter_mut() {
      if let ShipAction::FireAction { fire_control_dm, .. } = action {
        if pool > 0 {
          *fire_control_dm += 1;
          pool -= 1;
        }
      }
    }
    if pool == before {
      break;
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ship::{config_test_ship_templates, get_ship_template};
  use std::sync::Arc;

  const KM: f64 = 1000.0;

  /// A Buccaneer (three pulse turrets and a sandcaster, thrust 3) at the
  /// origin, holding contact on a second Buccaneer `distance` metres off.
  async fn two_ships(distance: f64) -> Entities {
    config_test_ship_templates().await;
    let design = get_ship_template("Buccaneer").unwrap();
    let mut world = Entities::new();
    world.add_ship("Hunter".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    world.add_ship(
      "Quarry".to_string(),
      Vec3::new(distance, 0.0, 0.0),
      Vec3::new(0.0, 1000.0, 0.0),
      &design,
      None,
      None,
    );
    world.ships["Hunter"].write().unwrap().contacts = vec!["Quarry".to_string()];
    world
  }

  /// The plan among a ship's requests.
  fn plan_of(requests: &[RequestMsg]) -> FlightPlan {
    requests
      .iter()
      .find_map(|request| match request {
        RequestMsg::SetPlan(plan) => Some(plan.plan.clone()),
        _ => None,
      })
      .unwrap_or_else(|| panic!("expected a plan, got {requests:?}"))
  }

  fn hunter(world: &Entities) -> Ship {
    world.ships["Hunter"].read().unwrap().clone()
  }

  fn fire_actions(requests: &[RequestMsg]) -> Vec<ShipAction> {
    requests
      .iter()
      .filter_map(|request| match request {
        RequestMsg::ModifyActions(list) => Some(list[0].1.clone()),
        _ => None,
      })
      .flatten()
      .collect()
  }

  /// Two Patrol Corvettes `distance` metres apart: pulse turrets at mounts 1
  /// and 2, triple-rack turrets at 3 and 4, and `gunners` at the guns.
  async fn corvettes(distance: f64, gunners: usize) -> Entities {
    config_test_ship_templates().await;
    let design = get_ship_template("Patrol Corvette - Type T").unwrap();
    let mut crew = crate::crew::Crew::new();
    (0..gunners).for_each(|_| crew.add_gunnery(1));
    let mut world = Entities::new();
    world.add_ship("Hunter".to_string(), Vec3::zero(), Vec3::zero(), &design, Some(crew), None);
    world.add_ship(
      "Quarry".to_string(),
      Vec3::new(distance, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    world.ships["Hunter"].write().unwrap().contacts = vec!["Quarry".to_string()];
    world
  }

  fn set_racks(world: &Entities, loads: [u32; 2]) {
    let mut hunter = world.ships["Hunter"].write().unwrap();
    hunter.loaded[2].missiles = loads[0];
    hunter.loaded[3].missiles = loads[1];
  }

  fn engage_quarry() -> Intent {
    Intent {
      fire: vec![Engage {
        target: "Quarry".to_string(),
        ..Engage::default()
      }],
      ..Intent::default()
    }
  }

  fn reloads_of(actions: &[ShipAction]) -> Vec<usize> {
    actions
      .iter()
      .filter_map(|action| match action {
        ShipAction::ReloadAction { weapon_id } => Some(*weapon_id),
        _ => None,
      })
      .collect()
  }

  fn fired(actions: &[ShipAction]) -> Vec<usize> {
    actions
      .iter()
      .filter_map(|action| match action {
        ShipAction::FireAction { weapon_id, .. } => Some(*weapon_id),
        _ => None,
      })
      .collect()
  }

  /// A spare gunner reloads for free, so it goes to the emptiest rack, and
  /// both racks still fire. A rack with no spare for it and missiles left
  /// keeps firing.
  #[tokio::test]
  async fn a_spare_gunner_reloads_the_emptiest_rack() {
    let world = corvettes(50_000.0 * KM, 5).await;
    set_racks(&world, [6, 3]);
    let actions = fire_actions(&orders(&world, "Hunter", &engage_quarry()).unwrap());
    assert_eq!(reloads_of(&actions), vec![3]);
    assert_eq!(fired(&actions), vec![2, 3], "both racks bear at Very Long; the lasers do not");
  }

  /// With no spare, an empty rack is reloaded by its own gunner and does not
  /// fire; a part-full one with a target fires.
  #[tokio::test]
  async fn with_no_spare_only_an_empty_rack_with_a_target_is_reloaded() {
    let world = corvettes(50_000.0 * KM, 4).await;
    set_racks(&world, [0, 6]);
    let actions = fire_actions(&orders(&world, "Hunter", &engage_quarry()).unwrap());
    assert_eq!(reloads_of(&actions), vec![2]);
    assert_eq!(fired(&actions), vec![3]);

    // With nothing to shoot at, a part-full rack is topped up too.
    let actions = fire_actions(&orders(&world, "Hunter", &Intent::default()).unwrap());
    assert_eq!(reloads_of(&actions), vec![2, 3]);
  }

  /// A mixed turret -- a rack beside two lasers -- is not reloaded while its
  /// lasers have a target, even with the rack empty; with no target it is.
  #[tokio::test]
  async fn a_mixed_turret_reloads_only_when_its_lasers_have_no_target() {
    let world = corvettes(5.0 * KM, 1).await;
    {
      let mut hunter = world.ships["Hunter"].write().unwrap();
      let mixed = Weapon {
        mount: WeaponMount::Turret,
        guns: vec![
          crate::ship::Gun::new(WeaponType::Pulse),
          crate::ship::Gun::new(WeaponType::Pulse),
          crate::ship::Gun::new(WeaponType::Missile),
        ],
        count: 1,
      };
      hunter.set_weapons(Some(vec![mixed]));
      hunter.fixup_current_values();
      hunter.loaded[0].missiles = 0;
      hunter.magazine.missiles = 20;
    }
    let actions = fire_actions(&orders(&world, "Hunter", &engage_quarry()).unwrap());
    assert_eq!(reloads_of(&actions), Vec::<usize>::new());
    assert_eq!(fired(&actions), vec![0], "the lasers fire");
    let actions = fire_actions(&orders(&world, "Hunter", &Intent::default()).unwrap());
    assert_eq!(reloads_of(&actions), vec![0]);
  }

  /// A commander that names its reloads gets them, and a mount it names
  /// beyond the spares is left out of the fire it would otherwise join.
  #[tokio::test]
  async fn named_reloads_are_carried_out() {
    let world = corvettes(50_000.0 * KM, 4).await;
    set_racks(&world, [6, 6]);
    let intent = Intent {
      reload: Some(vec![3, 3, 0]),
      ..engage_quarry()
    };
    let actions = fire_actions(&orders(&world, "Hunter", &intent).unwrap());
    assert_eq!(reloads_of(&actions), vec![3], "a full laser and a repeat are dropped");
    assert_eq!(fired(&actions), vec![2]);
    let intent = Intent {
      reload: Some(vec![]),
      ..engage_quarry()
    };
    assert_eq!(fired(&fire_actions(&orders(&world, "Hunter", &intent).unwrap())), vec![2, 3]);
  }

  #[test]
  fn aim_points_sit_inside_their_bands() {
    for band in [
      Range::Short,
      Range::Medium,
      Range::Long,
      Range::VeryLong,
      Range::Distant,
    ] {
      assert_eq!(band_at(aim_point(band)), band, "{band:?}");
    }
    assert_eq!(band_at(1_000.0 * KM), Range::Short);
    assert_eq!(band_at(30_000.0 * KM), Range::VeryLong);
    assert_eq!(band_at(1.0e12), Range::Distant);
  }

  #[tokio::test]
  async fn only_guns_that_reach_bear_and_sand_never_does() {
    let world = two_ships(20_000.0 * KM).await;
    let own = hunter(&world);
    assert_eq!(
      weapons_that_bear(&own, Range::Long),
      vec![(0, WeaponType::Pulse), (1, WeaponType::Pulse), (2, WeaponType::Pulse)]
    );
    assert!(weapons_that_bear(&own, Range::VeryLong).is_empty(), "pulse lasers stop at Long");
  }

  /// Jump Control is switched on for the jump, and Evade switched off to make
  /// room, in one round: the stop goes out before the start.
  #[tokio::test]
  async fn software_orders_stop_before_they_start() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      software: vec![
        SoftwareOrder {
          program: SoftwareKind::JumpControl,
          running: true,
        },
        SoftwareOrder {
          program: SoftwareKind::Manoeuvre,
          running: false,
        },
      ],
      engineering: vec![EngineerVerb::Jump],
      ..Intent::default()
    };
    let requests = orders(&world, "Hunter", &intent).unwrap();
    let changes: Vec<(SoftwareKind, bool)> = requests
      .iter()
      .filter_map(|request| match request {
        RequestMsg::SetSoftwareRunning(change) => Some((change.software.kind, change.running)),
        _ => None,
      })
      .collect();
    assert_eq!(
      changes,
      vec![(SoftwareKind::Manoeuvre, false), (SoftwareKind::JumpControl, true)],
      "{requests:?}"
    );
    assert_eq!(
      fire_actions(&requests),
      vec![ShipAction::Jump { engineer: 0 }],
      "the jump itself is the engineer's action"
    );
  }

  /// A captain with boosts rolls and queues them; the pilot can assist; the
  /// engineer can shut a mount down.
  #[tokio::test]
  async fn captain_pilot_and_power_orders() {
    let world = two_ships(5_000.0 * KM).await;
    world.ships["Hunter"]
      .write()
      .unwrap()
      .get_crew_mut()
      .set_skill(crate::crew::Skills::Leadership, 2);
    let intent = Intent {
      assist_gunners: Some(true),
      boosts: vec![Boost::Fire { mount: 0 }, Boost::Dodge],
      engineering: vec![EngineerVerb::Power {
        system: PowerSystem::Weapon(2),
        online: false,
      }],
      ..Intent::default()
    };
    let requests = orders(&world, "Hunter", &intent).unwrap();
    assert!(
      requests.iter().any(|r| matches!(r, RequestMsg::CaptainAction(_))),
      "{requests:?}"
    );
    assert!(requests
      .iter()
      .any(|r| matches!(r, RequestMsg::SetPilotActions(p) if p.assist_gunners == Some(true))));
    let actions = fire_actions(&requests);
    assert!(actions.contains(&ShipAction::SetPower {
      system: PowerSystem::Weapon(2),
      online: false,
      engineer: 0
    }));
    assert!(actions
      .iter()
      .any(|a| matches!(a, ShipAction::LeadershipCheck { boosts } if boosts.len() == 2)));
  }

  /// A laser turret on point defence is spoken for: it defends and does not
  /// fire, even if a fire order would have used it.
  #[tokio::test]
  async fn a_mount_on_point_defence_does_not_fire() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      defend: vec![0, 3, 9],
      fire: vec![Engage {
        target: "Quarry".to_string(),
        ..Engage::default()
      }],
      ..Intent::default()
    };
    let actions = fire_actions(&orders(&world, "Hunter", &intent).unwrap());
    let defending: Vec<usize> = actions
      .iter()
      .filter_map(|action| match action {
        ShipAction::PointDefenseAction { weapon_id, .. } => Some(*weapon_id),
        _ => None,
      })
      .collect();
    let firing: Vec<usize> = actions
      .iter()
      .filter_map(|action| match action {
        ShipAction::FireAction { weapon_id, .. } => Some(*weapon_id),
        _ => None,
      })
      .collect();
    assert_eq!(
      defending,
      vec![0],
      "the pulse turret defends; the sandcaster (3) cannot, 9 does not exist"
    );
    assert_eq!(firing, vec![1, 2]);
  }

  #[tokio::test]
  async fn a_mount_card_knows_a_barbette_from_a_turret() {
    config_test_ship_templates().await;
    let executor = ship_of("HMS Executor", 0.0);
    let barbette = mount_card(&executor.weapons()[0]);
    let (kind, profile) = barbette.attack.unwrap();
    assert_eq!((kind, profile.damage_dice, barbette.multiple), (WeaponType::Particle, 4, 3));
    assert!(!barbette.point_defence, "a barbette cannot track a missile");
    let turret = mount_card(&executor.weapons()[1]);
    assert_eq!(turret.attack.map(|(kind, _)| kind), Some(WeaponType::Missile));
    assert!(
      turret.sand && !turret.point_defence,
      "missiles and sand: no laser to defend with"
    );
  }

  #[tokio::test]
  async fn a_called_shot_is_only_called_at_short_range() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      fire: vec![Engage {
        target: "Quarry".to_string(),
        weapons: vec![0],
        called: Some(ShipSystem::Maneuver),
      }],
      ..Intent::default()
    };
    match &fire_actions(&orders(&world, "Hunter", &intent).unwrap())[..] {
      [ShipAction::FireAction { called_shot_system, .. }] => {
        assert_eq!(*called_shot_system, None, "Medium range: the gun fires, but not a called shot");
      }
      other => panic!("expected one fire action, got {other:?}"),
    }
  }

  #[tokio::test]
  async fn engage_fires_each_mount_once_and_only_at_contacts() {
    let world = two_ships(1_000.0 * KM).await;
    let intent = Intent {
      fire: vec![
        Engage {
          target: "Quarry".to_string(),
          weapons: vec![0, 1],
          called: Some(ShipSystem::Maneuver),
        },
        // Everything else, at the same ship: only mount 2 is left.
        Engage {
          target: "Quarry".to_string(),
          ..Engage::default()
        },
        // Not a contact: nobody aboard can pick it.
        Engage {
          target: "Ghost".to_string(),
          ..Engage::default()
        },
      ],
      ..Intent::default()
    };
    let actions = fire_actions(&orders(&world, "Hunter", &intent).unwrap());
    let fired: Vec<(usize, Option<ShipSystem>)> = actions
      .iter()
      .filter_map(|action| match action {
        ShipAction::FireAction {
          weapon_id,
          called_shot_system,
          ..
        } => Some((*weapon_id, *called_shot_system)),
        _ => None,
      })
      .collect();
    assert_eq!(
      fired,
      vec![
        (0, Some(ShipSystem::Maneuver)),
        (1, Some(ShipSystem::Maneuver)),
        (2, None)
      ]
    );
  }

  #[tokio::test]
  async fn a_target_out_of_reach_is_not_fired_on() {
    let world = two_ships(40_000.0 * KM).await;
    let intent = Intent {
      fire: vec![Engage {
        target: "Quarry".to_string(),
        ..Engage::default()
      }],
      ..Intent::default()
    };
    assert!(orders(&world, "Hunter", &intent).unwrap().is_empty());
  }

  #[tokio::test]
  async fn silence_turns_everything_off() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      posture: Some(Posture::Silent),
      ..Intent::default()
    };
    match &orders(&world, "Hunter", &intent).unwrap()[..] {
      [RequestMsg::SetShipEmissions(emissions)] => {
        assert_eq!(emissions.active_sensors, Some(false));
        assert_eq!(emissions.transmitting, Some(false));
        assert_eq!(emissions.handoff_sensors, Some(false));
      }
      other => panic!("expected one emissions order, got {other:?}"),
    }
  }

  #[tokio::test]
  async fn repair_takes_the_worst_listed_system_and_nothing_if_whole() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      engineering: vec![EngineerVerb::Repair {
        priority: vec![ShipSystem::Maneuver, ShipSystem::Powerplant, ShipSystem::Weapon],
      }],
      ..Intent::default()
    };
    assert!(orders(&world, "Hunter", &intent).unwrap().is_empty(), "nothing to repair");

    {
      let mut own = world.ships["Hunter"].write().unwrap();
      own.crit_level[ShipSystem::Maneuver as usize] = 1;
      own.crit_level[ShipSystem::Weapon as usize] = 2;
      // Worse, but not on the list.
      own.crit_level[ShipSystem::Cargo as usize] = 3;
    }
    assert_eq!(
      fire_actions(&orders(&world, "Hunter", &intent).unwrap()),
      vec![ShipAction::Repair {
        system: ShipSystem::Weapon,
        engineer: 0
      }]
    );
  }

  #[tokio::test]
  async fn orders_beyond_the_crew_aboard_are_dropped() {
    let world = two_ships(5_000.0 * KM).await;
    let operators = hunter(&world).get_crew().sensor_operators().max(1);
    let intent = Intent {
      sensors: vec![
        SensorVerb::Lock {
          target: "Quarry".to_string()
        };
        operators + 2
      ],
      ..Intent::default()
    };
    assert_eq!(fire_actions(&orders(&world, "Hunter", &intent).unwrap()).len(), operators);
  }

  #[tokio::test]
  async fn an_intercept_burns_toward_the_target_within_the_drive() {
    let world = two_ships(30_000.0 * KM).await;
    let intent = Intent {
      helm: Helm::Intercept {
        target: "Quarry".to_string(),
        band: Range::Medium,
      },
      ..Intent::default()
    };
    let plan = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    let burn = plan.0 .0;
    let drive = G * f64::from(hunter(&world).max_acceleration());
    assert!(burn.magnitude() <= drive * 1.001, "{burn:?} exceeds {drive}");
    assert!(burn.x > 0.0, "the first burn closes on a target out along +x: {burn:?}");
  }

  /// Holding Very Long against a ship coming on fast: a rendezvous would be
  /// a round behind her, so the helm burns straight away until the range
  /// opens.
  #[tokio::test]
  async fn holding_a_band_against_a_ship_closing_burns_away() {
    let world = two_ships(27_000.0 * KM).await;
    // The quarry is coming straight at us at 20 km/s: 7,200 km a round.
    world.ships["Quarry"]
      .write()
      .unwrap()
      .set_velocity(Vec3::new(-20_000.0, 0.0, 0.0));
    let intent = Intent {
      helm: Helm::HoldRange {
        target: "Quarry".to_string(),
        band: Range::VeryLong,
      },
      ..Intent::default()
    };
    let plan = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    let drive = G * f64::from(hunter(&world).max_acceleration());
    assert!((plan.0 .0 - Vec3::new(-drive, 0.0, 0.0)).magnitude() < 1e-6, "{:?}", plan.0 .0);
  }

  /// Opening the range uses the whole drive: the dodge reserve and the gunner
  /// assist are let go first, then the plan burns at full thrust.
  #[tokio::test]
  async fn running_releases_the_thrust_held_back() {
    let world = two_ships(5_000.0 * KM).await;
    world.ships["Hunter"]
      .write()
      .unwrap()
      .set_pilot_actions(Some(1), Some(true))
      .unwrap();
    let held = hunter(&world).max_acceleration();
    let intent = Intent {
      helm: Helm::OpenRange {
        target: "Quarry".to_string(),
      },
      evade: Some(1),
      assist_gunners: Some(true),
      ..Intent::default()
    };
    match &orders(&world, "Hunter", &intent).unwrap()[..] {
      [RequestMsg::SetPilotActions(pilot), RequestMsg::SetPlan(plan)] => {
        assert_eq!((pilot.dodge_thrust, pilot.assist_gunners), (Some(0), Some(false)));
        let burn = plan.plan.0 .0.magnitude() / G;
        assert!((burn - f64::from(held + 2)).abs() < 1e-6, "burns {burn} G, held back {held} G");
      }
      other => panic!("expected a release then a plan, got {other:?}"),
    }
  }

  /// An intercept on a ship burning hard away still brakes: the course is
  /// plotted against her velocity, not her burn, so it is a rendezvous (burn,
  /// then turn and brake) rather than a pursuit that never slows down.
  #[tokio::test]
  async fn an_intercept_on_a_burning_ship_still_brakes() {
    let world = two_ships(1_000_000.0 * KM).await;
    {
      let mut quarry = world.ships["Quarry"].write().unwrap();
      let away = Vec3::new(G * 3.0, 0.0, 0.0);
      quarry.plan = FlightPlan::new((away, DEFAULT_ACCEL_DURATION).into(), None);
    }
    let intent = Intent {
      helm: Helm::Intercept {
        target: "Quarry".to_string(),
        band: Range::Medium,
      },
      ..Intent::default()
    };
    let plan = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    assert!(plan.1.is_some(), "a rendezvous has a braking leg: {plan:?}");
  }

  /// With contact, the course allows for her burn; without it, a blip's burn
  /// is not known and the course assumes she holds her velocity.
  #[tokio::test]
  async fn only_a_contacts_burn_is_used() {
    let world = two_ships(1_000_000.0 * KM).await;
    {
      let mut quarry = world.ships["Quarry"].write().unwrap();
      let sideways = Vec3::new(0.0, G, 0.0);
      quarry.plan = FlightPlan::new((sideways, DEFAULT_ACCEL_DURATION).into(), None);
    }
    let intent = Intent {
      helm: Helm::Intercept {
        target: "Quarry".to_string(),
        band: Range::Medium,
      },
      ..Intent::default()
    };
    let in_contact = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    world.ships["Hunter"].write().unwrap().contacts.clear();
    let blip = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    assert_ne!(in_contact, blip, "the contact's burn changes the course");
  }

  /// Two ships of a squadron side by side: the follower flies the guide's
  /// course, so they stay together.
  #[tokio::test]
  async fn a_follower_in_station_flies_the_guides_course() {
    let world = two_ships(1_000_000.0 * KM).await;
    let design = get_ship_template("Buccaneer").unwrap();
    let mut world = world;
    world.add_ship(
      "Wing".to_string(),
      Vec3::new(0.0, 300.0 * KM, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let guide_helm = Helm::Intercept {
      target: "Quarry".to_string(),
      band: Range::Medium,
    };
    let guide = plan_of(
      &orders(
        &world,
        "Hunter",
        &Intent {
          helm: guide_helm.clone(),
          max_thrust: Some(1.0),
          ..Intent::default()
        },
      )
      .unwrap(),
    );
    let wing = plan_of(
      &orders(
        &world,
        "Wing",
        &Intent {
          helm: Helm::Follow {
            guide: "Hunter".to_string(),
            guide_helm: Box::new(guide_helm),
            guide_cap: Some(1.0),
          },
          ..Intent::default()
        },
      )
      .unwrap(),
    );
    assert_eq!(wing, guide);
  }

  /// A follower far from her guide closes on the guide, not on the enemy.
  #[tokio::test]
  async fn a_follower_out_of_station_closes_on_her_guide() {
    let mut world = two_ships(1_000_000.0 * KM).await;
    let design = get_ship_template("Buccaneer").unwrap();
    world.add_ship(
      "Wing".to_string(),
      Vec3::new(-200_000.0 * KM, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let wing = plan_of(
      &orders(
        &world,
        "Wing",
        &Intent {
          helm: Helm::Follow {
            guide: "Hunter".to_string(),
            guide_helm: Box::new(Helm::Hold),
            guide_cap: None,
          },
          ..Intent::default()
        },
      )
      .unwrap(),
    );
    // The guide sits still at the origin: this round the wing burns toward
    // her (+x), and then flies the guide's own course -- here, none -- until
    // the next round's order. Braking is decided round by round.
    assert!(wing.0 .0.x > 0.0, "{wing:?}");
    assert_eq!(wing.0 .1, DELTA_TIME_ROUND, "{wing:?}");
    assert!(wing.1.as_ref().is_some_and(|leg| leg.0.magnitude() < 1e-9), "{wing:?}");
  }

  /// Fly a plan for one round: each leg for as much of the round as it lasts,
  /// then coast.
  fn fly(ship: &mut Ship, plan: &FlightPlan) {
    let mut left = DELTA_TIME_F64;
    let mut position = ship.get_position();
    let mut velocity = ship.get_velocity();
    let legs = std::iter::once(&plan.0).chain(plan.1.as_ref());
    for leg in legs {
      #[allow(clippy::cast_precision_loss)]
      let t = (leg.1 as f64).min(left);
      position += velocity * t + leg.0 * (0.5 * t * t);
      velocity += leg.0 * t;
      left -= t;
    }
    position += velocity * left;
    ship.set_position(position);
    ship.set_velocity(velocity);
    ship.plan = plan.clone();
  }

  /// A follower 20,000 km out of station and drifting away, while her guide
  /// burns for a distant enemy a quarter G below her drive (and turns to
  /// brake partway): round by round she closes on the guide on that quarter
  /// G and then stays with her, rather than swinging through and out the
  /// other side.
  #[tokio::test]
  async fn a_follower_closes_up_and_stays_in_station() {
    follower_rejoins(
      Vec3::new(-12_000.0 * KM, 16_000.0 * KM, 0.0),
      Vec3::new(-1_000.0, 1_700.0, 0.0),
      0.25,
      30,
    )
    .await;
  }

  /// The opening of Fire over Jupiter as it went wrong: a follower left to
  /// coast for a round while her guide burned, 2,667 km behind and 17.6 km/s
  /// slower. With the whole G the guide gives up for that, she rejoins.
  #[tokio::test]
  async fn a_follower_left_behind_for_a_round_rejoins() {
    follower_rejoins(Vec3::new(-2_667.0 * KM, 0.0, 0.0), Vec3::new(-17_653.0, 0.0, 0.0), 1.0, 15).await;
  }

  async fn follower_rejoins(offset: Vec3, drift: Vec3, margin: f64, within: u32) {
    let mut world = two_ships(2_000_000.0 * KM).await;
    let design = get_ship_template("Buccaneer").unwrap();
    world.add_ship("Wing".to_string(), offset, drift, &design, None, None);
    let drive = hunter(&world).max_acceleration();
    let guide_helm = Helm::Intercept {
      target: "Quarry".to_string(),
      band: Range::Long,
    };
    let cap = Some(f64::from(drive) - margin);
    let (_, short) = band_edges(Range::Short);
    let mut in_station_from = None;
    for round in 0..40 {
      let guide = plan_of(
        &orders(
          &world,
          "Hunter",
          &Intent {
            helm: guide_helm.clone(),
            max_thrust: cap,
            ..Intent::default()
          },
        )
        .unwrap(),
      );
      let wing = plan_of(
        &orders(
          &world,
          "Wing",
          &Intent {
            helm: Helm::Follow {
              guide: "Hunter".to_string(),
              guide_helm: Box::new(guide_helm.clone()),
              guide_cap: cap,
            },
            ..Intent::default()
          },
        )
        .unwrap(),
      );
      fly(&mut world.ships["Hunter"].write().unwrap(), &guide);
      fly(&mut world.ships["Wing"].write().unwrap(), &wing);
      let (hunter_ship, wing_ship) = (hunter(&world), world.ships["Wing"].read().unwrap().clone());
      let apart = (hunter_ship.get_position() - wing_ship.get_position()).magnitude();
      let drift = (hunter_ship.get_velocity() - wing_ship.get_velocity()).magnitude() * DELTA_TIME_F64;
      let in_station = apart <= short && drift <= short;
      match (in_station_from, in_station) {
        (None, true) => in_station_from = Some(round),
        (Some(from), false) => panic!(
          "in station from round {from}, out again at round {round}: {:.0} km apart, drifting {:.0} km a round",
          apart / KM,
          drift / KM
        ),
        _ => {}
      }
    }
    assert!(
      in_station_from.is_some_and(|round| round < within),
      "never closed up: in station from {in_station_from:?}"
    );
  }

  #[tokio::test]
  async fn a_thrust_cap_holds_the_burn_down() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      helm: Helm::OpenRange {
        target: "Quarry".to_string(),
      },
      max_thrust: Some(1.0),
      ..Intent::default()
    };
    let plan = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    assert!((plan.0 .0.magnitude() - G).abs() < 1e-6, "{:?}", plan.0 .0);
  }

  #[tokio::test]
  async fn opening_the_range_burns_straight_away_at_full_thrust() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      helm: Helm::OpenRange {
        target: "Quarry".to_string(),
      },
      ..Intent::default()
    };
    let plan = plan_of(&orders(&world, "Hunter", &intent).unwrap());
    let drive = G * f64::from(hunter(&world).max_acceleration());
    assert!((plan.0 .0 - Vec3::new(-drive, 0.0, 0.0)).magnitude() < 1e-6);
  }

  /// Going quiet hides a ship; firing gives it away; only a stealthy hull can
  /// slip a contact by opening the range.
  #[tokio::test]
  async fn silence_hides_and_firing_reveals() {
    config_test_ship_templates().await;
    let watcher = ship_of("Buccaneer", 0.0);
    let executor = ship_of("HMS Executor", 20_000.0 * KM);
    let odds = detection(&watcher, &executor);
    let find = |posture, firing| {
      odds
        .iter()
        .find(|o| o.posture == posture && o.firing == firing)
        .unwrap()
        .clone()
    };
    assert!(
      find(Posture::Silent, false).found < find(Posture::Active, false).found,
      "{odds:?}"
    );
    assert!(
      find(Posture::Silent, false).found < find(Posture::Silent, true).found,
      "{odds:?}"
    );

    assert!(
      find(Posture::Silent, false).slip_by_opening > 0.0,
      "the Executor's stealth can shake a contact"
    );

    let plain = ship_of("Buccaneer", 20_000.0 * KM);
    assert!(detection(&watcher, &plain).iter().all(|o| o.slip_by_opening == 0.0));
    let far = ship_of("Buccaneer", 1.0e12);
    assert!(
      detection(&watcher, &far).iter().all(|o| o.found == 0.0),
      "nothing is found at Distant"
    );
  }

  #[test]
  fn dice_add_up() {
    assert!((chance_of_success(0) - 15.0 / 36.0).abs() < 1e-12);
    assert!((chance_of_success(6) - 1.0).abs() < 1e-12);
    assert!(chance_of_success(-5).abs() < 1e-12);
    let two = dice_distribution(2);
    assert!((two.iter().sum::<f64>() - 1.0).abs() < 1e-12);
    assert!((two[7] - 6.0 / 36.0).abs() < 1e-12);
    assert_eq!(dice_distribution(0), vec![1.0]);
  }

  fn ship_of(design: &str, x: f64) -> Ship {
    let design = get_ship_template(design).unwrap();
    Ship::new("x".to_string(), Vec3::new(x, 0.0, 0.0), Vec3::zero(), &design, None, None)
  }

  fn at(table: &[BandOdds], band: Range) -> &BandOdds {
    table.iter().find(|odds| odds.band == band).unwrap()
  }

  /// The Executor's particle barbette reaches Very Long; a Buccaneer's pulse
  /// lasers stop at Long. At Very Long the Executor hits and cannot be hit
  /// back -- the case for staying out there.
  #[tokio::test]
  async fn a_longer_reach_shows_as_a_band_only_one_side_can_shoot_in() {
    config_test_ship_templates().await;
    let executor = ship_of("HMS Executor", 0.0);
    let buccaneer = ship_of("Buccaneer", 30_000.0 * KM);
    let skill = |_: usize| 2;
    let ours = engagement(&executor, &buccaneer, &skill, buccaneer.design.armor);
    let theirs = engagement(&buccaneer, &executor, &skill, executor.design.armor);

    assert!(at(&ours, Range::VeryLong).expected_damage > 0.0, "{ours:?}");
    assert!(at(&ours, Range::VeryLong).best_chance > 0.0);
    assert_eq!(at(&theirs, Range::VeryLong).mounts, 0, "{theirs:?}");
    assert!(at(&theirs, Range::Long).expected_damage > 0.0);
    // A worse range is a worse shot.
    assert!(at(&ours, Range::VeryLong).best_chance < at(&ours, Range::Short).best_chance);
  }

  /// Armour clips low damage rolls to nothing, so a light gun against a heavy
  /// hull loses most of its effect.
  #[tokio::test]
  async fn heavy_armour_shrinks_what_a_light_gun_can_do() {
    config_test_ship_templates().await;
    let buccaneer = ship_of("Buccaneer", 0.0);
    let target = ship_of("Gazulin Trader", 5_000.0 * KM);
    let skill = |_: usize| 1;
    let bare = at(&engagement(&buccaneer, &target, &skill, 0), Range::Medium).expected_damage;
    let armoured = at(&engagement(&buccaneer, &target, &skill, 8), Range::Medium).expected_damage;
    // Twin pulse turrets: about 9 expected against armour 8, 29 against none.
    // The turret's bonus dice come after armour, which is why it is not less.
    assert!(armoured > 0.0 && armoured < bare / 2.0, "{armoured} against {bare}");
  }

  /// A battery is one entry worth all its mounts: the odds it carries are
  /// the single mount's times the count, so sharing out fire assigns it
  /// whole, with its whole weight. A missile battery's launch is every rack.
  #[tokio::test]
  async fn a_battery_prices_as_its_mounts() {
    config_test_ship_templates().await;
    let target = ship_of("Buccaneer", 5_000.0 * KM);
    let barbette = Weapon::single(WeaponType::Particle, WeaponMount::Barbette);
    let mut single = ship_of("Gazulin Trader", 0.0);
    single.set_weapons(Some(vec![barbette.clone()]));
    single.fixup_current_values();
    let mut battery = single.clone();
    battery.set_weapons(Some(vec![barbette.times(4)]));
    battery.fixup_current_values();
    let skill = |_: usize| 2;
    let one = at(&engagement(&single, &target, &skill, 0), Range::Medium).by_mount[0].1;
    let table = engagement(&battery, &target, &skill, 0);
    let four = at(&table, Range::Medium);
    assert_eq!(four.mounts, 1, "one entry");
    assert!(
      (four.by_mount[0].1 - 4.0 * one).abs() < 1e-9,
      "{} against {one}",
      four.by_mount[0].1
    );

    let racks = Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 3).times(4);
    assert_eq!(launch_size(&racks, WeaponType::Missile), Some((12, 1)));
  }

  /// A spinal-armed design for these tests: a 30,000-ton monitor with a 2×
  /// meson and one triple beam turret.
  fn monitor() -> Arc<crate::ship::ShipDesignTemplate> {
    Arc::new(crate::ship::ShipDesignTemplate {
      name: "Monitor".to_string(),
      displacement: 30_000,
      hull: 15_000,
      maneuver: 1,
      power: 10_000,
      tl: 15,
      weapons: vec![
        Weapon::single(WeaponType::Meson, WeaponMount::Spinal { multiple: 2, tl: None }),
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3),
      ],
      ..crate::ship::ShipDesignTemplate::default()
    })
  }

  /// A spinal is priced like any gun, at ×1,000 and with its size DMs: a
  /// cruiser takes thousands from it, and a fighter that is manoeuvring
  /// takes nothing, since it cannot be attacked at all.
  #[tokio::test]
  async fn a_spinal_prices_at_a_thousand_and_not_against_a_fighter() {
    config_test_ship_templates().await;
    let design = monitor();
    let attacker = Ship::new("Monitor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let mut cruiser = Ship::new(
      "Cruiser".to_string(),
      Vec3::new(5_000.0 * KM, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let skill = |_: usize| 2;
    let medium = at(&engagement(&attacker, &cruiser, &skill, 10), Range::Medium).clone();
    let spinal = medium.by_mount.iter().find(|(mount, _)| *mount == 0).unwrap().1;
    assert!(spinal > 10_000.0, "12D ×1,000, through any armour: {spinal}");
    // Past its Long reach it does not bear.
    assert_eq!(
      at(&engagement(&attacker, &cruiser, &skill, 10), Range::VeryLong).by_mount,
      vec![]
    );

    let fighter = |still: bool| {
      let mut fighter = ship_of("Light Fighter", 5_000.0 * KM);
      fighter.held_still = still;
      at(&engagement(&attacker, &fighter, &skill, 0), Range::Medium).clone()
    };
    let moving = fighter(false);
    assert!(moving.by_mount.iter().all(|(mount, _)| *mount != 0), "{moving:?}");
    assert_eq!(moving.mounts, 1, "only the turret can attack");
    // Held still, she can be attacked, at DM-8: with gunner 2 at Medium it
    // needs a 14 on two dice, so it is priced, at nothing.
    let still = fighter(true);
    assert!(still.by_mount.iter().any(|(mount, _)| *mount == 0), "{still:?}");
    assert_eq!(still.mounts, 2);

    // A ship of 5,000 tons or less is DM-8, so it expects less than the
    // cruiser.
    cruiser.design = Arc::new(crate::ship::ShipDesignTemplate {
      displacement: 5_000,
      ..(*design).clone()
    });
    let small = at(&engagement(&attacker, &cruiser, &skill, 10), Range::Medium).clone();
    let small_spinal = small.by_mount.iter().find(|(mount, _)| *mount == 0).unwrap().1;
    assert!(small_spinal < spinal / 4.0, "{small_spinal} against {spinal}");
  }

  /// Fire orders leave a spinal off a target it cannot attack, so it is free
  /// for the next one.
  #[tokio::test]
  async fn a_spinal_is_not_wasted_on_a_fighter() {
    config_test_ship_templates().await;
    let design = monitor();
    let mut world = Entities::new();
    world.add_ship("Monitor".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let fighter = get_ship_template("Light Fighter").unwrap();
    world.add_ship(
      "Fighter".to_string(),
      Vec3::new(5_000.0 * KM, 0.0, 0.0),
      Vec3::zero(),
      &fighter,
      None,
      None,
    );
    world.add_ship(
      "Cruiser".to_string(),
      Vec3::new(6_000.0 * KM, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    world.ships["Monitor"].write().unwrap().contacts = vec!["Fighter".to_string(), "Cruiser".to_string()];
    let intent = Intent {
      fire: vec![
        Engage {
          target: "Fighter".to_string(),
          ..Engage::default()
        },
        Engage {
          target: "Cruiser".to_string(),
          ..Engage::default()
        },
      ],
      ..Intent::default()
    };
    let fired: Vec<(usize, String)> = fire_actions(&orders(&world, "Monitor", &intent).unwrap())
      .into_iter()
      .filter_map(|action| match action {
        ShipAction::FireAction { weapon_id, target, .. } => Some((weapon_id, target)),
        _ => None,
      })
      .collect();
    assert!(fired.contains(&(1, "Fighter".to_string())), "{fired:?}");
    assert!(fired.contains(&(0, "Cruiser".to_string())), "{fired:?}");
  }

  #[tokio::test]
  async fn a_helm_order_naming_a_missing_ship_is_an_error() {
    let world = two_ships(5_000.0 * KM).await;
    let intent = Intent {
      helm: Helm::Intercept {
        target: "Ghost".to_string(),
        band: Range::Short,
      },
      ..Intent::default()
    };
    assert!(orders(&world, "Hunter", &intent).is_err());
  }
}

#[cfg(test)]
mod flight_tests {
  use super::*;
  use crate::ship::{
    config_test_ship_templates, get_ship_template, FeatureKind, ShipDesignTemplate, ShipFeature, Team,
  };
  use crate::software::Software;
  use cgmath::Zero;
  use std::sync::Arc;

  const KM: f64 = 1000.0;

  /// A 50,000-ton carrier with thirty 50-ton docking spaces, one launch
  /// tube and one recovery deck, and a hundred missiles in stores.
  fn carrier_design() -> Arc<ShipDesignTemplate> {
    let feature = |name: &str, power: u32, kind: FeatureKind| ShipFeature {
      name: name.to_string(),
      power,
      default_on: true,
      kind: Some(kind),
    };
    Arc::new(ShipDesignTemplate {
      name: "Test Carrier".to_string(),
      displacement: 50_000,
      hull: 27_500,
      armor: 10,
      maneuver: 4,
      jump: 2,
      power: 60_000,
      fuel: 10_000,
      crew: 500,
      computer: 20,
      features: vec![
        feature(
          "Docking spaces",
          0,
          FeatureKind::DockingSpace {
            craft_tons: 50,
            count: 30,
          },
        ),
        feature(
          "Launch tube",
          500,
          FeatureKind::LaunchTube {
            craft_tons: 50,
            count: 1,
          },
        ),
        feature(
          "Recovery deck",
          500,
          FeatureKind::RecoveryDeck {
            craft_tons: 50,
            count: 1,
          },
        ),
      ],
      magazine: Magazine {
        missiles: 100,
        ..Default::default()
      },
      software: vec![Software::new(SoftwareKind::JumpControl, 2)],
      ..Default::default()
    })
  }

  /// The carrier at the origin on Blue with `craft` heavy fighters aboard,
  /// "Fighter 1" up, the first half in "Wing B" and the rest in "Wing A";
  /// and a Red Buccaneer, "Raider", `distance` metres off along x, in her
  /// contacts when `seen`.
  async fn scene(craft: usize, distance: f64, seen: bool) -> Entities {
    config_test_ship_templates().await;
    let mut world = Entities::new();
    world.add_ship("Carrier".to_string(), Vec3::zero(), Vec3::zero(), &carrier_design(), None, None);
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    {
      let mut carrier = world.ships["Carrier"].write().unwrap();
      carrier.team = Some(Team::Blue);
      for n in 1..=craft {
        let mut ship = Ship::new(format!("Fighter {n:02}"), Vec3::zero(), Vec3::zero(), &fighter, None, None);
        ship.team = Some(Team::Blue);
        ship.squadron = Some(if n <= craft / 2 { "Wing B" } else { "Wing A" }.to_string());
        carrier.embark(ship);
      }
      if seen {
        carrier.contacts = vec!["Raider".to_string()];
      }
    }
    let raider = get_ship_template("Buccaneer").unwrap();
    world.add_ship(
      "Raider".to_string(),
      Vec3::new(distance, 0.0, 0.0),
      Vec3::zero(),
      &raider,
      None,
      None,
    );
    world.ships["Raider"].write().unwrap().team = Some(Team::Red);
    world
  }

  /// A heavy fighter of the carrier's on the plot, `distance` metres from
  /// her along y, with the carrier's contacts.
  fn put_out(world: &mut Entities, name: &str, distance: f64) {
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    world.add_ship(
      name.to_string(),
      Vec3::new(0.0, distance, 0.0),
      Vec3::zero(),
      &fighter,
      None,
      None,
    );
    let contacts = world.ships["Carrier"].read().unwrap().contacts.clone();
    let mut craft = world.ships[name].write().unwrap();
    craft.team = Some(Team::Blue);
    craft.carrier = Some("Carrier".to_string());
    craft.contacts = contacts;
  }

  fn spend(craft: &mut Ship) {
    for load in &mut craft.loaded {
      *load = Magazine::default();
    }
    craft.magazine = Magazine::default();
  }

  fn actions_of(requests: &[RequestMsg]) -> Vec<ShipAction> {
    requests
      .iter()
      .filter_map(|request| match request {
        RequestMsg::ModifyActions(list) => Some(list[0].1.clone()),
        _ => None,
      })
      .flatten()
      .collect()
  }

  fn launched(requests: &[RequestMsg]) -> Option<Vec<String>> {
    actions_of(requests).into_iter().find_map(|action| match action {
      ShipAction::Launch { craft } => Some(craft),
      _ => None,
    })
  }

  fn has_plan(requests: &[RequestMsg]) -> bool {
    requests.iter().any(|request| matches!(request, RequestMsg::SetPlan(_)))
  }

  fn plan_of(requests: &[RequestMsg]) -> FlightPlan {
    requests
      .iter()
      .find_map(|request| match request {
        RequestMsg::SetPlan(plan) => Some(plan.plan.clone()),
        _ => None,
      })
      .unwrap_or_else(|| panic!("expected a plan, got {requests:?}"))
  }

  fn lands(requests: &[RequestMsg]) -> bool {
    actions_of(requests)
      .iter()
      .any(|action| matches!(action, ShipAction::Land { .. }))
  }

  /// With an enemy in contact within Very Long the carrier launches as many
  /// as her tube takes; at Distant, or unseen, she keeps them aboard.
  #[tokio::test]
  async fn a_carrier_launches_when_an_enemy_is_within_very_long() {
    let intent = Intent::default();
    let world = scene(12, 40_000.0 * KM, true).await;
    let requests = orders(&world, "Carrier", &intent).unwrap();
    let out = launched(&requests).expect("a launch order");
    assert_eq!(out.len(), 10, "{out:?}");

    let world = scene(12, 60_000.0 * KM, true).await;
    assert_eq!(launched(&orders(&world, "Carrier", &intent).unwrap()), None, "at Distant");
    let world = scene(12, 40_000.0 * KM, false).await;
    assert_eq!(launched(&orders(&world, "Carrier", &intent).unwrap()), None, "not in contact");
  }

  /// A round's launches come from one wing before the next, and a craft
  /// waiting on the stores to rearm, or with nothing to fight with, stays
  /// aboard.
  #[tokio::test]
  async fn launches_come_wing_by_wing_and_rearming_craft_wait() {
    let world = scene(12, 40_000.0 * KM, true).await;
    {
      let mut carrier = world.ships["Carrier"].write().unwrap();
      // Fighter 07 (Wing A) has fired everything and will rearm next round.
      let index = carrier.embarked.iter().position(|c| c.get_name() == "Fighter 07").unwrap();
      spend(&mut carrier.embarked[index]);
    }
    let out = launched(&orders(&world, "Carrier", &Intent::default()).unwrap()).unwrap();
    let wing_a: Vec<String> = (8..=12).map(|n| format!("Fighter {n:02}")).collect();
    assert_eq!(out[..5], wing_a[..], "Wing A first, without the rearming craft: {out:?}");
    let wing_b: Vec<String> = (1..=5).map(|n| format!("Fighter {n:02}")).collect();
    assert_eq!(out[5..], wing_b[..], "then Wing B: {out:?}");

    // With the stores empty as well there is nothing to rearm her with, so
    // she is ready as she is: she still has her laser.
    world.ships["Carrier"].write().unwrap().magazine = Magazine::default();
    let out = launched(&orders(&world, "Carrier", &Intent::default()).unwrap()).unwrap();
    assert!(out.contains(&"Fighter 07".to_string()), "{out:?}");
  }

  /// A craft whose missiles are all gone flies back to her carrier -- a
  /// plan toward her, whatever helm she was given -- and gives *Land* only
  /// once on the approach: within Short, matched, and with the carrier not
  /// burning; then she coasts, so neither ship's thrust counts against the
  /// check. With nothing in the carrier's stores to rearm her she stays
  /// out: she still has her laser.
  #[tokio::test]
  async fn a_spent_craft_goes_home_and_lands_when_close() {
    let mut world = scene(0, 8_000.0 * KM, true).await;
    put_out(&mut world, "Fighter 1", 20_000.0 * KM);
    spend(&mut world.ships["Fighter 1"].write().unwrap());
    let intent = Intent {
      helm: Helm::Hold,
      ..Intent::default()
    };
    let requests = orders(&world, "Fighter 1", &intent).unwrap();
    assert!(has_plan(&requests), "she plots a course home: {requests:?}");
    assert!(!lands(&requests), "too far to land this round: {requests:?}");
    assert!(plan_of(&requests).0 .0.magnitude() > G, "she burns for home: {requests:?}");

    // Within Short and matched: coast and land.
    world.ships["Fighter 1"]
      .write()
      .unwrap()
      .set_position(Vec3::new(0.0, 800.0 * KM, 0.0));
    let requests = orders(&world, "Fighter 1", &intent).unwrap();
    assert!(lands(&requests), "{requests:?}");
    assert_eq!(plan_of(&requests).0 .0, Vec3::zero(), "she coasts in: {requests:?}");
    let home = Some(Flight::Land {
      carrier: "Carrier".to_string(),
    });
    assert_eq!(flight(&world, &world.ships["Fighter 1"].read().unwrap(), &intent), home);

    // Passing too fast, or the carrier burning: close on her, no Land yet.
    world.ships["Fighter 1"]
      .write()
      .unwrap()
      .set_velocity(Vec3::new(10_000.0, 0.0, 0.0));
    let requests = orders(&world, "Fighter 1", &intent).unwrap();
    assert!(!lands(&requests) && has_plan(&requests), "{requests:?}");
    world.ships["Fighter 1"].write().unwrap().set_velocity(Vec3::zero());
    world.ships["Carrier"].write().unwrap().plan = FlightPlan::new((Vec3::new(0.0, 3.0 * G, 0.0), 1000).into(), None);
    assert!(
      !lands(&orders(&world, "Fighter 1", &intent).unwrap()),
      "the carrier is burning 3 G"
    );
    world.ships["Carrier"].write().unwrap().plan = FlightPlan::default();

    world.ships["Carrier"].write().unwrap().magazine = Magazine::default();
    assert_eq!(flight(&world, &world.ships["Fighter 1"].read().unwrap(), &intent), None);
  }

  /// A craft with her loads full stays out while an enemy is in contact --
  /// withdrawing any *Land* she had standing from before -- and goes home
  /// once none is.
  #[tokio::test]
  async fn a_craft_with_nothing_to_fight_goes_home() {
    let mut world = scene(0, 8_000.0 * KM, true).await;
    put_out(&mut world, "Fighter 1", 800.0 * KM);
    let craft = world.ships["Fighter 1"].read().unwrap().clone();
    assert_eq!(flight(&world, &craft, &Intent::default()), None);
    let requests = orders(&world, "Fighter 1", &Intent::default()).unwrap();
    assert!(actions_of(&requests).contains(&ShipAction::ClearLand), "{requests:?}");
    world.ships["Fighter 1"].write().unwrap().contacts.clear();
    let craft = world.ships["Fighter 1"].read().unwrap().clone();
    assert_eq!(
      flight(&world, &craft, &Intent::default()),
      Some(Flight::Land {
        carrier: "Carrier".to_string()
      })
    );
    assert!(lands(&orders(&world, "Fighter 1", &Intent::default()).unwrap()));
  }

  /// A carrier about to jump launches nothing, and her craft come home
  /// while Jump Control runs.
  #[tokio::test]
  async fn craft_come_home_before_the_carrier_jumps() {
    let mut world = scene(12, 40_000.0 * KM, true).await;
    put_out(&mut world, "Fighter 20", 5_000.0 * KM);
    let jumping = Intent {
      engineering: vec![EngineerVerb::Jump],
      ..Intent::default()
    };
    assert_eq!(
      launched(&orders(&world, "Carrier", &jumping).unwrap()),
      None,
      "no launch on the way out"
    );
    let craft = world.ships["Fighter 20"].read().unwrap().clone();
    assert_eq!(
      flight(&world, &craft, &Intent::default()),
      None,
      "the carrier has not started Jump Control yet"
    );

    world.ships["Carrier"].write().unwrap().software_running = vec![Software::new(SoftwareKind::JumpControl, 2)];
    assert_eq!(launched(&orders(&world, "Carrier", &Intent::default()).unwrap()), None);
    let craft = world.ships["Fighter 20"].read().unwrap().clone();
    assert_eq!(
      flight(&world, &craft, &Intent::default()),
      Some(Flight::Land {
        carrier: "Carrier".to_string()
      })
    );
  }

  /// A commander's flight order stands in place of the crews': a craft told
  /// to hold stays out with her loads full and withdraws any *Land* she had,
  /// but goes home once spent; a carrier launches the craft she is told to,
  /// once each and no more than the tube takes, or none when told to hold.
  #[tokio::test]
  async fn a_commanders_flight_order_overrides_the_crews() {
    let mut world = scene(12, 40_000.0 * KM, true).await;
    put_out(&mut world, "Fighter 20", 800.0 * KM);
    world.ships["Fighter 20"].write().unwrap().contacts.clear();
    let hold = Intent {
      flight: Some(Flight::Hold),
      ..Intent::default()
    };
    let requests = orders(&world, "Fighter 20", &hold).unwrap();
    assert!(!lands(&requests) && !has_plan(&requests), "{requests:?}");
    assert!(actions_of(&requests).contains(&ShipAction::ClearLand), "{requests:?}");
    spend(&mut world.ships["Fighter 20"].write().unwrap());
    assert!(
      lands(&orders(&world, "Fighter 20", &hold).unwrap()),
      "spent, she goes home even when told to hold"
    );

    let named = Intent {
      flight: Some(Flight::Launch {
        craft: ["Fighter 03", "Fighter 01", "Fighter 03", "Nobody"]
          .iter()
          .map(ToString::to_string)
          .collect(),
        count: None,
      }),
      ..Intent::default()
    };
    assert_eq!(
      launched(&orders(&world, "Carrier", &named).unwrap()),
      Some(vec!["Fighter 03".to_string(), "Fighter 01".to_string()])
    );
    let counted = Intent {
      flight: Some(Flight::Launch {
        craft: vec![],
        count: Some(3),
      }),
      ..Intent::default()
    };
    assert_eq!(
      launched(&orders(&world, "Carrier", &counted).unwrap()).map(|out| out.len()),
      Some(3)
    );
    let all = Intent {
      flight: Some(Flight::Launch {
        craft: vec![],
        count: Some(50),
      }),
      ..Intent::default()
    };
    assert_eq!(
      launched(&orders(&world, "Carrier", &all).unwrap()).map(|out| out.len()),
      Some(10),
      "the tube's rate caps it"
    );
    assert_eq!(launched(&orders(&world, "Carrier", &hold).unwrap()), None, "told to hold");
  }

  /// Two wings on the approach at once land one at a time: the check loses
  /// a point for every other craft coming in that is not of the squadron.
  #[tokio::test]
  async fn wings_land_one_at_a_time() {
    let mut world = scene(0, 8_000.0 * KM, true).await;
    for (name, wing) in [
      ("Fighter 1", "Wing B"),
      ("Fighter 2", "Wing A"),
      ("Fighter 3", "Wing B"),
    ] {
      put_out(&mut world, name, 600.0 * KM);
      let mut craft = world.ships[name].write().unwrap();
      craft.squadron = Some(wing.to_string());
      craft.contacts.clear();
    }
    assert!(
      lands(&orders(&world, "Fighter 2", &Intent::default()).unwrap()),
      "Wing A's turn"
    );
    assert!(
      !lands(&orders(&world, "Fighter 1", &Intent::default()).unwrap()),
      "Wing B waits on station"
    );
    assert!(!lands(&orders(&world, "Fighter 3", &Intent::default()).unwrap()));
    // Wing A gone aboard: Wing B's turn.
    world.ships.remove("Fighter 2");
    assert!(lands(&orders(&world, "Fighter 1", &Intent::default()).unwrap()));
    assert!(lands(&orders(&world, "Fighter 3", &Intent::default()).unwrap()));
  }

  /// A ship held still by a berth operation gives no flight order of her
  /// own: she is in one.
  #[tokio::test]
  async fn a_pinned_ship_leaves_flight_operations_alone() {
    let mut world = scene(12, 40_000.0 * KM, true).await;
    put_out(&mut world, "Fighter 20", 5_000.0 * KM);
    spend(&mut world.ships["Fighter 20"].write().unwrap());
    world.ships["Fighter 20"].write().unwrap().pinned = true;
    world.ships["Carrier"].write().unwrap().pinned = true;
    assert_eq!(
      flight(&world, &world.ships["Fighter 20"].read().unwrap(), &Intent::default()),
      None
    );
    assert_eq!(
      flight(&world, &world.ships["Carrier"].read().unwrap(), &Intent::default()),
      None
    );
  }

  /// Spent means the launchers have nothing left, in the mounts or the
  /// stores; a laser fighter is never spent, nor an unarmed ship. A heavy
  /// fighter spent still has her laser, so she is not useless.
  #[tokio::test]
  async fn spent_is_for_launchers_with_nothing_left() {
    config_test_ship_templates().await;
    let heavy = get_ship_template("Heavy Fighter").unwrap();
    let mut fighter = Ship::new("H".to_string(), Vec3::zero(), Vec3::zero(), &heavy, None, None);
    assert!(!loads_spent(&fighter));
    spend(&mut fighter);
    assert!(loads_spent(&fighter));
    assert!(!nothing_to_fight_with(&fighter), "a beam laser beside the rack");
    let light = get_ship_template("Light Fighter").unwrap();
    let laser = Ship::new("L".to_string(), Vec3::zero(), Vec3::zero(), &light, None, None);
    assert!(!loads_spent(&laser));
    let unarmed = Ship::new(
      "U".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    assert!(!loads_spent(&unarmed));
  }
}

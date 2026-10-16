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
  FlightPlan, MountClass, PowerSystem, Range, Salvo, Ship, ShipSystem, Weapon, WeaponMount, WeaponProfile, WeaponType,
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
    guide_cap: Option<u8>,
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

/// One ship's orders for a round, in a commander's terms.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct Intent {
  /// `None` leaves emissions as they are.
  #[serde(default)]
  pub posture: Option<Posture>,
  #[serde(default)]
  pub helm: Helm,
  /// The most thrust the helm may use, in G. Every G of burn is +1 to anyone
  /// looking for the ship, so a ship creeping up unseen burns gently. `None`
  /// uses the whole drive.
  #[serde(default)]
  pub max_thrust: Option<u8>,
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
fn expected_hit_damage(dice: &[f64], effect: i32, armour: u32, turret_bonus: u32, multiple: u32) -> f64 {
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
        (through + turret_bonus) * multiple
      };
      chance * f64::from(damage)
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
  BANDS
    .iter()
    .map(|band| {
      let bearing = weapons_that_bear(attacker, *band);
      let mut best_chance: f64 = 0.0;
      let mut expected = 0.0;
      let mut launchers = 0;
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
          called_shot: false,
        };
        let Some(terms) = attack_terms(&situation, 0, 0) else {
          continue;
        };
        let dm: i32 = terms.iter().map(|(_, value)| value).sum();
        let dice = dice_distribution(profile.damage_dice);
        let armour = armour.saturating_sub(u32::from(profile.ap));
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
            expected_hit_damage(&dice, effect, armour, turret_bonus, multiple)
          };
          (chance + p, damage + p * hit)
        });
        best_chance = best_chance.max(chance);
        expected += per_attack * count;
      }
      BandOdds {
        band: *band,
        best_chance,
        expected_damage: expected,
        mounts: bearing.len(),
        missiles_only: !bearing.is_empty() && launchers == bearing.len(),
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
/// the commander capped it. Thrust is seen, a G of it at a time.
fn helm_thrust(own: &Ship, cap: Option<u8>, running: bool) -> f64 {
  // Running uses the whole drive: the dodge reserve and the gunner assist
  // are released for the round (see `orders`).
  let drive = if running {
    own.max_acceleration() + own.get_dodge_thrust() + u8::from(own.get_assist_gunners())
  } else {
    own.max_acceleration()
  };
  G * f64::from(cap.map_or(drive, |cap| cap.min(drive)))
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

fn helm_plan(world: &Entities, own: &Ship, helm: &Helm, cap: Option<u8>) -> Result<Option<FlightPlan>, String> {
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
  world: &Entities, own: &Ship, guide: &Ship, guide_helm: &Helm, guide_cap: Option<u8>, thrust: f64,
) -> Result<FlightPlan, String> {
  if matches!(guide_helm, Helm::Follow { .. }) {
    return Err(format!("{} cannot guide while following", guide.get_name()));
  }
  // The guide's course this round, worked out as her own order will be.
  let course = helm_plan(world, guide, guide_helm, guide_cap)?.unwrap_or_else(|| guide.plan.clone());
  let (_, short) = band_edges(Range::Short);
  let apart = (own.get_position() - guide.get_position()).magnitude();
  let drift = (own.get_velocity() - guide.get_velocity()).magnitude() * DELTA_TIME_F64;
  if apart <= short && drift <= short {
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
  let spare = (thrust - burn.magnitude()).max(G / 2.0);
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
  let with_burn = |accel: Vec3| {
    let total = accel + burn;
    if total.magnitude() > thrust && total.magnitude() > 0.0 {
      total * (thrust / total.magnitude())
    } else {
      total
    }
  };
  let first = (with_burn(correction.0 .0), correction.0 .1);
  let second = correction.1.as_ref().map(|leg| (with_burn(leg.0), leg.1));
  Ok(FlightPlan::new(first.into(), second.map(Into::into)))
}

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
  requests.extend(helm_requests(world, &own, ship_name, intent)?);

  let mut actions = Vec::new();
  let mut committed = std::collections::HashSet::new();
  // Defence first: a mount on point defence is spoken for before any fire
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
    let range = band_between(&own, &target.read().unwrap());
    // A called shot is only offered at Short range, as the gunner's console
    // offers it; further out the gun fires as an ordinary shot.
    let called = engage.called.filter(|_| range == Range::Short);
    for (weapon_id, kind) in weapons_that_bear(&own, range) {
      let chosen = engage.weapons.is_empty() || engage.weapons.contains(&weapon_id);
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
          max_thrust: Some(1),
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
            guide_cap: Some(1),
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
    // The guide sits still at the origin; the wing burns toward her (+x)
    // and brakes to stop beside her.
    assert!(wing.0 .0.x > 0.0, "{wing:?}");
    assert!(wing.1.as_ref().is_some_and(|leg| leg.0.x < 0.0), "{wing:?}");
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

  /// A follower that starts well out of station, drifting away, while her
  /// guide burns hard for a distant enemy: round by round she closes on the
  /// guide and then stays with her, rather than swinging through and out the
  /// other side.
  #[tokio::test]
  async fn a_follower_closes_up_and_stays_in_station() {
    let mut world = two_ships(2_000_000.0 * KM).await;
    let design = get_ship_template("Buccaneer").unwrap();
    world.add_ship(
      "Wing".to_string(),
      Vec3::new(-60_000.0 * KM, 40_000.0 * KM, 0.0),
      Vec3::new(-5_000.0, 15_000.0, 0.0),
      &design,
      None,
      None,
    );
    let drive = hunter(&world).max_acceleration();
    let guide_helm = Helm::Intercept {
      target: "Quarry".to_string(),
      band: Range::Long,
    };
    let cap = Some(drive - 1);
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
      in_station_from.is_some_and(|round| round < 25),
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
      max_thrust: Some(1),
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

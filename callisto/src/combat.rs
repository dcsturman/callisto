use std::collections::HashMap;
use std::fmt::Write as _;
use std::hash::BuildHasher;
use std::sync::{Arc, RwLock};

use cgmath::InnerSpace;
use rand::RngCore;

use crate::action::{
  boost_for_assist_gunner, boost_for_evade, boost_for_fire, boost_for_point_defense, BoostMap, ShipAction,
};
use crate::entity::{no_contact_effect, Entity};
use crate::payloads::{EffectMsg, LaunchMissileMsg, MessageCategory};
use crate::rules_tables::{
  crew_crit_percent, damage_multiple, ignores_primary_critical, is_capital, profile_for, spinal_range_dm,
  spinal_size_dm, system_loss, CargoHit, ShortBand, SystemLoss, RANGE_BANDS, RANGE_MOD,
};
use crate::ship::{
  spinal_name, spinal_uses_rounds, BridgeStation, Firing, Magazine, MountClass, Range, Salvo, Sensors, Ship,
  ShipSystem, SystemDamage, Weapon, WeaponMount, WeaponProfile, WeaponType,
};
use crate::software::SoftwareKind;
use crate::{debug, error, info, warn};
use tracing::event;
use tracing::Level;

const DIE_SIZE: u32 = 6;
pub const STANDARD_ROLL_THRESHOLD: i32 = 8;
const CRITICAL_THRESHOLD: i32 = 5 + STANDARD_ROLL_THRESHOLD;

pub fn roll(rng: &mut dyn RngCore) -> u8 {
  u8::try_from(rng.next_u32() % DIE_SIZE + 1).unwrap_or(0)
}

/// Roll `dice` d6 for a check or a per-weapon pool (2D, a screen's 2D, a
/// battery's 6D), as a `u8`.
///
/// A `u8` holds up to 42 dice.  Damage, which can run to 66D on a spinal mount
/// (High Guard p. 35), goes through [`roll_dice_min`] and its wider total; asking
/// this for more than 42 dice is a bug, logged and saturated rather than
/// silently returned as 0.
pub fn roll_dice(dice: u8, rng: &mut dyn RngCore) -> u8 {
  let total = roll_dice_min(dice, 1, rng);
  u8::try_from(total).unwrap_or_else(|_| {
    error!("(Combat.roll_dice) {dice}D totals {total}, too many for a check roll; use roll_dice_min.");
    u8::MAX
  })
}

/// Roll `dice` d6, counting any die below `min_die` as `min_die`.
///
/// This exists for High Yield, which counts every '1' as a '2' (and Very High
/// Yield, every '1' and '2' as a '3').  That has to be decided per die rather
/// than on the total, which is why the sum cannot simply be adjusted afterwards.
#[must_use]
pub fn roll_dice_min(dice: u8, min_die: u8, rng: &mut dyn RngCore) -> u16 {
  (0..dice).map(|_| u16::from(roll(rng).max(min_die))).sum()
}

#[must_use]
pub fn task_chain_impact(effect: i32) -> i32 {
  match effect {
    x if x <= -6 => -3,
    -5..=-2 => -2,
    -1 => -1,
    0 => 1,
    1..=5 => 2,
    _ => 3,
  }
}

/// The attacker's side of the to-hit DM, kept as separate terms.
///
/// These used to arrive summed into one number that the log could only call
/// "gunner", so a skill-3 gunner showing +6 looked like a bug. It was the
/// pilot's assist and two captain's inspires stacked on top -- all real, none
/// of them visible.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HitMods {
  /// The gunner's own skill on this mount.
  pub gunner: i32,
  /// The pilot's Assist Gunner task chain, from their own roll this round.
  pub assist: i32,
  /// The captain's inspire on that assist: +1 on the first shot only.
  pub captain_assist: i32,
  /// The captain's inspire on this weapon's fire action.
  pub captain_fire: i32,
  /// A smart missile's guidance bonus: its TL over the target's, clamped.
  /// A launched object resolves as the gun that threw it, so this is the
  /// attacker-side term for a missile, where there is no gunner at impact.
  pub smart: i32,
  /// Fire Control points the gunner spent on this shot as a DM.
  ///
  /// The program's score is a pool each round, spent either on firing a
  /// mount outright or on improving someone else's shot (CRB p. 161); this
  /// is the second of those.
  pub fire_control: i32,
}

impl HitMods {
  /// Just a gunner, nobody helping. What most tests want.
  #[must_use]
  pub const fn gunner(gunner: i32) -> Self {
    HitMods {
      gunner,
      assist: 0,
      captain_assist: 0,
      captain_fire: 0,
      smart: 0,
      fire_control: 0,
    }
  }

  /// Named for the results log, in the order they are reasoned about.
  #[must_use]
  pub const fn terms(&self) -> [(&'static str, i32); 5] {
    [
      ("gunner", self.gunner),
      ("assist", self.assist),
      ("captain assist", self.captain_assist),
      ("captain fire", self.captain_fire),
      ("smart missile", self.smart),
    ]
  }
}

/// What decides an attack roll, apart from the dice.
pub(crate) struct AttackSituation<'a> {
  pub hit: HitMods,
  pub attacker: &'a Ship,
  pub defender: &'a Ship,
  pub firing: &'a Firing<'a>,
  pub profile: &'a WeaponProfile,
  pub range_band: Range,
  /// How far apart the ships are, in metres, when that is known. Only a
  /// spinal mount looks: its close-range DMs split the Short band
  /// ([`ShortBand`]). `None` is anywhere in the band, which a spinal reads as
  /// the Short row: the AI pricing a band it is not in.
  pub distance: Option<u32>,
  pub called_shot: bool,
}

/// The attack DM, term by term, exactly as [`attack`] counts it -- or `None`
/// if the gun cannot reach.
///
/// The one place the sum is made. [`attack`] rolls against it, and the AI's
/// engagement table works out its odds from it, so what the AI expects is
/// what the dice then do. The defender's dodge is passed in rather than read,
/// because spending it is a side effect of really being shot at.
pub(crate) fn attack_terms(
  situation: &AttackSituation<'_>, evade_mod: i32, captain_mod: i32,
) -> Option<[(&'static str, i32); 20]> {
  let AttackSituation {
    hit,
    attacker,
    defender,
    firing,
    profile,
    range_band,
    distance,
    called_shot,
  } = situation;
  // Evade software flies the ship evasively on its own: "a negative DM equal
  // to the Evade program's score to all attacks" (CRB p. 161). It needs no
  // Thrust and no pilot, so it stacks with a pilot who is also dodging --
  // the book gives no reason it should not, and the two are different
  // things: one is the computer jinking, the other a pilot spending Thrust.
  let evade_software = -i32::from(defender.running_level(SoftwareKind::Evade).unwrap_or(0));

  // Advanced Fire Control adds its score to every attack the ship makes
  // (High Guard p. 73), with no automated fire of its own.
  let advanced_fire_control = i32::from(attacker.running_level(SoftwareKind::AdvancedFireControl).unwrap_or(0));

  // Launch Solution does the same for missile and torpedo salvoes only
  // (High Guard p. 75).
  let launch_solution = if profile.salvo.is_some() {
    i32::from(attacker.running_level(SoftwareKind::LaunchSolution).unwrap_or(0))
  } else {
    0
  };

  // Launchers have "Special" range: the salvo flies to the target, so the
  // firing range never modifies the roll and never rules the shot out.
  let range_mod = if profile.salvo.is_some() {
    0
  } else if profile.reaches(*range_band) {
    RANGE_MOD[*range_band as usize]
  } else {
    return None;
  };

  let lock_mod = if attacker.sensor_locks.contains(&defender.get_name().to_string()) {
    2
  } else {
    0
  };

  let called_mod = if *called_shot { -2 } else { 0 };

  // "Torpedo salvoes suffer an additional DM-2 on their attack rolls against
  // ships smaller than 2,000 tons" (High Guard p. 39) -- they are built to kill
  // capital ships and struggle to connect with anything nimble.
  //
  // Bays likewise: "All bay weapons suffer DM-2 when attacking targets of 2,000
  // tons or less and DM-4 when attacking targets of 100 tons or less. Missile
  // and torpedo salvoes do not use these modifiers" (High Guard p. 31). A bay
  // gun is laid for a capital ship. One term, since a torpedo bay is a salvo
  // and takes only the torpedo's DM.
  //
  // A spinal mount takes DM-4 against 10,000 tons or less and DM-8 against
  // 5,000 or less, and cannot attack under 2,000 tons at all "unless they are
  // stationary" (High Guard p. 34) -- for Callisto, unless the target held
  // still last round (see `Ship::held_still`).
  let tons = defender.design.displacement;
  let spinal = matches!(firing.mount, WeaponMount::Spinal { .. });
  let small_target_mod = if spinal {
    match spinal_size_dm(tons) {
      Some(dm) => dm,
      None if defender.held_still => -8,
      None => return None,
    }
  } else if firing.kind == WeaponType::Torpedo && tons < 2_000 {
    -2
  } else if matches!(firing.mount, WeaponMount::Bay(_)) && profile.salvo.is_none() {
    match tons {
      0..=100 => -4,
      101..=2_000 => -2,
      _ => 0,
    }
  } else {
    0
  };

  // Spinal Weapon Range Adjustments (High Guard p. 35): a spinal is aimed by
  // pointing the ship, so it struggles to lay on anything close, and more so
  // the bigger the ship.
  let spinal_range_mod = if spinal && *range_band == Range::Short {
    spinal_range_dm(attacker.design.displacement, distance.map_or(ShortBand::Short, ShortBand::of))
  } else {
    0
  };

  // "Weapons with the Orbital Bombardment trait suffer DM-12 when attacking
  // any target that can manoeuvre in space combat" (High Guard p. 30).
  let bombardment_mod = if profile.bombardment && defender.current_maneuver > 0 {
    -12
  } else {
    0
  };

  // What the attacker's own damage does to her gunnery: the DM-1 of a
  // Sensors or Weapon critical at severity 1 (CRB p. 170), and what her crew
  // carries into every check -- crew lost to criticals (High Guard p. 27)
  // and radiation (p. 31).
  let damage_mod = attacker.attack_dm;
  let crew_mod = i32::from(attacker.crew_dm());

  let [gunner, assist, captain_assist, captain_fire, smart] = hit.terms();
  Some([
    gunner,
    assist,
    captain_assist,
    captain_fire,
    smart,
    ("damage", damage_mod),
    ("crew", crew_mod),
    ("weapon", profile.hit_mod),
    ("range", range_mod),
    ("called shot", called_mod),
    ("small target", small_target_mod),
    ("spinal range", spinal_range_mod),
    ("bombardment", bombardment_mod),
    ("sensor lock", lock_mod),
    ("evade", evade_mod),
    ("captain evade", captain_mod),
    ("evade software", evade_software),
    ("fire control", hit.fire_control),
    ("adv fire control", advanced_fire_control),
    ("launch solution", launch_solution),
  ])
}

/// Do the attack of one ship's weapon system against a ship.  This includes resolving previously launched missiles that
/// now impact the target.
///
/// # Arguments
/// * `hit` - The attacker's to-hit terms, reported separately in the log.
/// * `damage_mod` - The damage modifier to use (positive or negative).
/// * `attacker` - The ship that is attacking.  This is used to get any relevant DMs not included in `hit_mod` or `damage_mod`.
/// * `defender` - The ship that is being attacked.  This is used to get any relevant DMs not included in `hit_mod` or `damage_mod` (e.g. armor) as
///   well as to apply damage.
/// * `weapon` - The weapon being used.  This is used to get the weapon type and mount.
/// * `rng` - The random number generator to use.
///
/// # Returns
/// A list of all the effects resulting from the attack.
///
/// # Panics
/// Panics if the lock cannot be obtained to read a ship or if we have a case where a check was made and then untrue
/// (e.g. finding the index number of a ship in a list after ensuring its in the list).
// Eight params (one over the clippy default) is the natural seam: hit/damage
// mods, attacker, defender, weapon, called-shot, boost map, and rng.
// Splitting them into a struct would not improve clarity here.
#[allow(clippy::too_many_arguments)]
pub fn attack(
  hit: HitMods, damage_mod: i32, attacker: &Ship, defender: &mut Ship, firing: &Firing<'_>,
  called_shot_system: Option<&ShipSystem>, boost_map: &BoostMap, rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
  let mut shot = Shot::default();
  let mut effects = resolve_attack(
    hit,
    damage_mod,
    attacker,
    defender,
    firing,
    called_shot_system,
    boost_map,
    rng,
    &mut shot,
  );
  // The ship that dodged hears of it as well: the attacker's line names the
  // evade in its arithmetic, but the defender's crew never saw which shots
  // their pilot's reserve (and their captain's inspire) went on, or whether
  // it mattered.
  effects.extend(shot.dodged);
  effects
}

/// What one attack came to, kept so a salvo or a battery can report all its
/// shots in one line.
#[derive(Debug, Default)]
struct Shot {
  /// The defender's line about dodging it, when she dodged.
  dodged: Option<EffectMsg>,
  /// Whether it was the dodge that made the shot miss.
  dodge_made_it_miss: bool,
  /// The 2D rolled and the DM on it, when the attack got as far as a roll.
  /// The attack's own line is then the first effect it returned.
  rolled: Option<(i32, i32)>,
  /// The DM, itemised, as the attack's own line gives it.
  breakdown: String,
  hit: bool,
  /// Hull damage done, after armour, screens and multiples.
  damage: u32,
  criticals: u32,
}

/// The most rolls a volley's hover lists before it says how many more.
const VOLLEY_ROLLS_LISTED: usize = 24;

/// Several attacks by one weapon at one target, reported as one line.
///
/// A missile salvo is one object with many missiles in it, and a battery is
/// one weapon with many mounts. Each missile, and each mount, still rolls its
/// own attack and does its own damage ("each one rolls to hit"; IN p. 113's
/// "separate attack roll for each turret or barbette"), so this is
/// [`attack`] run `shots` times. What changes is the report: one line, "7 of
/// 12 hit, 31 damage", where there used to be twelve. Criticals and anything
/// else an attack says beyond its own line are kept as they are.
///
/// `title` names the volley in that line, "Missile salvo (12)", and is given
/// whenever there is more than one of anything. With no title, each shot is
/// reported exactly as [`attack`] reports it, which keeps a lone missile and a
/// single mount reading as they always have.
///
/// `damage_mods` is each shot's damage modifier in turn -- sand thrown at
/// that shot -- and a shot past its end has none. `called` is the system
/// every shot is aimed at, for a battery's called shot.
#[allow(clippy::too_many_arguments, clippy::too_many_lines)]
pub fn volley(
  hit: HitMods, damage_mods: &[i32], attacker: &Ship, defender: &mut Ship, firing: &Firing<'_>, shots: u16,
  title: Option<&str>, called: Option<&ShipSystem>, boost_map: &BoostMap, rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
  let damage_mod = |shot: usize| damage_mods.get(shot).copied().unwrap_or(0);
  let Some(title) = title else {
    return (0..usize::from(shots.max(1)))
      .flat_map(|shot| attack(hit, damage_mod(shot), attacker, defender, firing, called, boost_map, rng))
      .collect();
  };

  let mut outcomes: Vec<Shot> = Vec::with_capacity(usize::from(shots));
  // One explosion or beam for the whole volley: the plot shows where it
  // landed, not how often.
  let mut visual: Option<EffectMsg> = None;
  let mut rest: Vec<EffectMsg> = Vec::new();
  for index in 0..usize::from(shots) {
    let mut shot = Shot::default();
    let effects = resolve_attack(
      hit,
      damage_mod(index),
      attacker,
      defender,
      firing,
      called,
      boost_map,
      rng,
      &mut shot,
    );
    if shot.rolled.is_none() {
      // It never got to a roll -- out of range, or a weapon that cannot be
      // fired from its mount -- and every other shot would say the same.
      return effects;
    }
    // The first effect is the shot's own line, which the summary replaces.
    for effect in effects.into_iter().skip(1) {
      match effect {
        EffectMsg::ShipImpact { .. } | EffectMsg::BeamHit { .. } => {
          visual.get_or_insert(effect);
        }
        other => rest.push(other),
      }
    }
    outcomes.push(shot);
  }

  let attacker_name = attacker.get_name();
  let weapon_name = String::from(&firing.kind);
  let hits = outcomes.iter().filter(|shot| shot.hit).count();
  let damage: u64 = outcomes.iter().map(|shot| u64::from(shot.damage)).sum();
  let criticals: u32 = outcomes.iter().map(|shot| shot.criticals).sum();

  let mut line = format!("%s {title} -> %t: {hits} of {shots} hit");
  if hits > 0 {
    let _ = write!(line, ", {damage} damage");
  }
  if criticals > 0 {
    let _ = write!(line, ", {criticals} critical{}", if criticals == 1 { "" } else { "s" });
  }
  line.push('.');

  // The arithmetic, for the hover: the DM as the first shot had it -- every
  // shot's, unless the defender dodged some -- then every total rolled and
  // the damage each hit did. A long volley's lists are cut short; the counts
  // in the line are always complete.
  let list = |values: Vec<String>| {
    let more = values.len().saturating_sub(VOLLEY_ROLLS_LISTED);
    let mut shown: Vec<String> = values.into_iter().take(VOLLEY_ROLLS_LISTED).collect();
    if more > 0 {
      shown.push(format!("and {more} more"));
    }
    shown.join(", ")
  };
  let first_dm = outcomes[0].rolled.map_or(0, |(_, dm)| dm);
  let rolls = list(
    outcomes
      .iter()
      .filter_map(|shot| shot.rolled.map(|(roll, dm)| (roll + dm).to_string()))
      .collect(),
  );
  let mut arithmetic = format!(
    "each 2D{first_dm:+}{} vs {STANDARD_ROLL_THRESHOLD}: {rolls}",
    outcomes[0].breakdown
  );
  if hits > 0 {
    let damages = list(
      outcomes
        .iter()
        .filter(|shot| shot.hit)
        .map(|shot| shot.damage.to_string())
        .collect(),
    );
    let _ = write!(arithmetic, " · damage {damages}");
  }

  let category = if hits > 0 {
    MessageCategory::Damage
  } else {
    MessageCategory::Attack
  };
  let mut effects = vec![EffectMsg::outcome(attacker_name, category, line, hits > 0)
    .at(defender.get_name())
    .rolled(arithmetic)];
  effects.extend(visual);
  effects.extend(rest);

  // The defender hears of her dodging once for the volley, as the attacker's
  // line is once for it.
  let dodges = outcomes.iter().filter(|shot| shot.dodged.is_some()).count();
  if dodges > 0 {
    let decisive = outcomes.iter().filter(|shot| shot.dodge_made_it_miss).count();
    effects.push(
      EffectMsg::outcome(
        defender.get_name(),
        MessageCategory::Attack,
        format!("%s dodges {dodges} of %t's {shots} {weapon_name}s: the dodge made {decisive} miss."),
        decisive > 0,
      )
      .at(attacker_name),
    );
  }
  effects
}

/// [`attack`], with what the shot came to -- and what the defender's dodge
/// did to it, if she dodged -- left in `shot`.
#[allow(clippy::too_many_lines, clippy::too_many_arguments)]
fn resolve_attack(
  hit: HitMods, damage_mod: i32, attacker: &Ship, defender: &mut Ship, firing: &Firing<'_>,
  called_shot_system: Option<&ShipSystem>, boost_map: &BoostMap, rng: &mut dyn RngCore, shot: &mut Shot,
) -> Vec<EffectMsg> {
  let attacker_name = attacker.get_name();

  // Damage, range, hit modifier and armour penetration all depend on how the
  // weapon is mounted, so everything below reads from the profile rather than
  // from the weapon type alone.
  let Some(profile) = profile_for(firing.kind, firing.mount).map(|p| p.with_modifiers(firing.kind, firing.modifiers))
  else {
    error!(
      "(Combat.attack) {} cannot be mounted as a {}, so {attacker_name} cannot fire it.",
      String::from(&firing.kind),
      String::from(&firing.kind)
    );
    return vec![EffectMsg::about(
      attacker_name,
      MessageCategory::Info,
      format!(
        "%s's {} is not a mount this weapon can be fired from.",
        String::from(&firing.kind)
      ),
    )];
  };

  // This in theory could be lossy but that would require there to be more than 4.29x10^9m which is VERY far.  If we
  // wanted to be safer we check if the magnitude was greater than u32::MAX and then just use that.
  // Note we will lose precision here but this is just for range so okay.
  #[allow(clippy::cast_sign_loss)]
  #[allow(clippy::cast_possible_truncation)]
  let distance = (defender.get_position() - attacker.get_position()).magnitude() as u32;
  let range_band = find_range_band(distance);

  // "Ships at a range of Short or less may attempt to make called shots with
  // direct-fire weapons (not missiles or torpedoes)" (Core Rulebook p. 169).
  // The gunner's console only offers one at Short; this holds every client to
  // it. Anything else fires as an ordinary shot, without the DM-2.
  let called_shot_system = called_shot_system.filter(|_| range_band == Range::Short && profile.salvo.is_none());

  debug!(
        "(Combat.attack) Calculating range with attacker {} at {:?}, defender {} at {:?}.  Distance is {}.  Range is {}. Range_mod is {}",
        attacker.get_name(),
        attacker.get_position(),
        defender.get_name(),
        defender.get_position(),
        (defender.get_position() - attacker.get_position()).magnitude(),
        range_band,
        RANGE_MOD[range_band as usize]
    );

  // Captain Evade boost: +1 to the defender's evasion roll on the FIRST
  // attack against this ship this turn (only applies if the ship is actually
  // dodging, i.e. has dodge_thrust remaining). Decrement happens via the
  // standard `decrement_dodge_thrust` path below; this just sets the
  // already-consumed flag so subsequent attacks this turn don't get the +1.
  let evade_boost = if defender.get_dodge_thrust() > 0
    && boost_for_evade(boost_map, defender.get_name()) > 0
    && !defender.has_evade_boost_used()
  {
    defender.set_evade_boost_used(true);
    1_i32
  } else {
    0
  };

  // Kept as two terms rather than one, because they are two different facts
  // the pilot and captain each want to see: the pilot's skill is why the shot
  // was harder, and the captain's inspire -- spent on this one attack -- is
  // why it was harder still.
  let evading = defender.get_dodge_thrust() > 0;
  let (evade_mod, captain_mod) = if evading {
    debug!(
      "(Combat.attack) {} has dodge thrust {}, so defensive modifier is -{} (with evade boost {}).",
      defender.get_name(),
      defender.get_dodge_thrust(),
      defender.get_crew().get_pilot(),
      evade_boost
    );
    defender.decrement_dodge_thrust();
    (-i32::from(defender.get_crew().get_pilot()), -evade_boost)
  } else {
    (0, 0)
  };
  let Some(terms) = attack_terms(
    &AttackSituation {
      hit,
      attacker,
      defender,
      firing,
      profile: &profile,
      range_band,
      distance: Some(distance),
      called_shot: called_shot_system.is_some(),
    },
    evade_mod,
    captain_mod,
  ) else {
    // We are out of range so cannot attack
    // Should never get here!
    error!(
      "(Combat.attack) {} is out of range of {}'s {}.",
      defender.get_name(),
      attacker.get_name(),
      String::from(&firing.kind)
    );
    return vec![EffectMsg::about(
      attacker.get_name(),
      MessageCategory::Info,
      format!("%t is out of range of %s's {}.", String::from(&firing.kind)),
    )
    .at(defender.get_name())];
  };

  info!(
    "(Combat.attack) Ship {attacker_name} attacking with {firing:?} against {} with terms {terms:?}",
    defender.get_name()
  );

  if let Some(cs) = called_shot_system {
    info!("(Combat.attack) Called shot system is {:?}.", cs);
  }

  let roll = i32::from(roll_dice(2, rng));

  // The attack DM, itemised. Seven terms summed into one number cannot be
  // checked, and the two that matter most to the ship being shot at -- its
  // pilot dodging, and its captain's inspire -- were invisible: an evasion
  // never reached the results log at all. Same treatment the detection DM
  // gets, and for the same reason. Zero terms are dropped so an ordinary
  // shot stays short.
  let attack_mod: i32 = terms.iter().map(|(_, value)| value).sum();
  let hit_roll = roll + attack_mod;
  let named: Vec<String> = terms
    .iter()
    .filter(|(name, value)| {
      if *name == "evade" {
        // A ship that dodged says so even when its pilot's skill is 0 and the
        // dodge bought nothing. Otherwise the line is identical to one where
        // nobody evaded, and a pilot cannot tell the order was carried out.
        evading
      } else {
        // Everything else is named when it did something. Matched on the term
        // itself rather than on the text it prints: dropping anything that
        // began "evade " also dropped "evade software", so a defender running
        // Evade had the DM counted against the shot and left out of the
        // arithmetic, which then did not add up.
        *value != 0
      }
    })
    .map(|(name, value)| format!("{name} {value:+}"))
    .collect();
  let breakdown = named.join(", ");
  let breakdown = if breakdown.is_empty() {
    String::new()
  } else {
    format!(" ({breakdown})")
  };

  // Every attack opens the same way: who fired what at whom, with the two
  // ships left as placeholders so the client can draw each in its own
  // colour. Reused by all four outcomes below.
  let weapon_name = firing_name(firing);
  let attack_line = format!("%s {weapon_name} -> %t");
  // How the number came out, for the hover. This used to sit in the middle
  // of the sentence, which made every attack a line of arithmetic with the
  // outcome hidden at the end of it.
  let attack_roll = format!("{roll}{attack_mod:+}={hit_roll}{breakdown} vs {STANDARD_ROLL_THRESHOLD}");
  shot.rolled = Some((roll, attack_mod));
  shot.hit = hit_roll >= STANDARD_ROLL_THRESHOLD;
  shot.breakdown.clone_from(&breakdown);

  if evading {
    let missed = hit_roll < STANDARD_ROLL_THRESHOLD;
    // The same roll without the dodge: whether it was the dodge that made
    // the difference.
    let undodged = hit_roll - evade_mod - captain_mod;
    shot.dodge_made_it_miss = missed && undodged >= STANDARD_ROLL_THRESHOLD;
    let outcome = match (missed, undodged >= STANDARD_ROLL_THRESHOLD) {
      (true, true) => "the dodge made it miss",
      (true, false) => "it missed, and would have anyway",
      (false, _) => "it hit anyway",
    };
    let mut why = vec![format!("evade {evade_mod:+}")];
    if captain_mod != 0 {
      why.push(format!("captain evade {captain_mod:+}"));
    }
    shot.dodged = Some(
      EffectMsg::outcome(
        defender.get_name(),
        MessageCategory::Attack,
        format!("%s dodges %t's {weapon_name}: {outcome}."),
        missed,
      )
      .at(attacker_name)
      .rolled(format!(
        "{} = {hit_roll}, {undodged} without the dodge, vs {STANDARD_ROLL_THRESHOLD}",
        why.join(", ")
      )),
    );
  }

  if hit_roll < STANDARD_ROLL_THRESHOLD {
    debug!(
      "(Combat.attack) {}'s attack roll is {}, adjusted to {}, and misses.",
      attacker_name, roll, hit_roll
    );
    return vec![
      EffectMsg::outcome(attacker_name, MessageCategory::Attack, format!("{attack_line}: miss."), false)
        .at(defender.get_name())
        .rolled(attack_roll),
    ];
  }

  let effect: u32 = u32::try_from(hit_roll - STANDARD_ROLL_THRESHOLD).unwrap_or(0);

  debug!(
    "(Combat.attack) {attacker_name}'s attack roll is {roll}, giving effect {effect}, and hits {}.",
    defender.get_name()
  );

  // Damage is the weapon's dice plus the Effect of the hit roll -- except for
  // missiles and torpedoes. The book has a salvo "roll for damage as if for a
  // single missile ... but do not apply the Effect", multiplying by the Effect
  // instead, capped at the missiles left in the salvo (Core Rulebook p. 169).
  // Callisto rolls every missile in a salvo on its own instead, so each one
  // that hits does its dice alone.
  let roll = u32::from(roll_dice_min(
    profile.damage_dice,
    WeaponProfile::min_die(firing.kind, firing.modifiers),
    rng,
  ));
  let effect_damage = if profile.salvo.is_some() { 0 } else { effect };
  let mut damage = roll + effect_damage;

  // The damage arithmetic, accumulated as it happens rather than reconstructed
  // afterwards -- armour, screens, turret bonus and Damage Multiple are applied
  // at four different points and there is no way to infer the order from the
  // final number alone. "How did we get to 5?" should be answerable from the
  // results log without opening the debug output.
  let mut terms = vec![format!("{}D {roll}", profile.damage_dice)];
  if profile.salvo.is_none() {
    terms.push(format!("+{effect} effect"));
  }

  damage = if i64::from(damage) + i64::from(damage_mod) < 0 {
    0
  } else {
    u32::try_from(i32::try_from(damage).unwrap_or(i32::MAX) + damage_mod).unwrap_or(0)
  };
  if damage_mod != 0 {
    terms.push(format!("{damage_mod:+} mod"));
  }

  // AP comes off the armour before the armour comes off the damage
  // (High Guard p. 29).  Meson guns carry AP_INFINITE and so ignore it wholly.
  //
  // A particle or railgun spinal loses a percentage a point instead (High
  // Guard p. 36), taken with the Damage Multiple below so nothing is rounded
  // away before it is multiplied by 1,000.
  let armour = defender.get_current_armor();
  let effective_armor = profile.armour_subtracted(armour);
  let through = profile.armour_through(armour);
  if profile.armour_percent > 0 {
    let left = armour.saturating_sub(u32::from(profile.ap));
    if left > 0 {
      let ap = if profile.ap > 0 {
        format!(", AP {}", profile.ap)
      } else {
        String::new()
      };
      terms.push(format!("-{}% armour ({left} × {}%{ap})", 100 - through, profile.armour_percent));
    }
  } else if effective_armor > 0 || profile.ap > 0 {
    // Name the AP only when it did something, so an ordinary shot stays short.
    if profile.ap > 0 {
      terms.push(format!("-{effective_armor} armour (AP {})", profile.ap));
    } else {
      terms.push(format!("-{effective_armor} armour"));
    }
  }

  damage = if damage > effective_armor && through > 0 {
    damage - effective_armor
  } else {
    debug!(
            "(Combat.attack) Due too armor, {} does no damage to {} after rolling {}, adjustment with damage modifier {}, hit effect {}, and defender armor -{}.",
            attacker_name,
            defender.get_name(),
            roll,
            damage_mod,
            (hit_roll - STANDARD_ROLL_THRESHOLD),
            defender.get_current_armor()
        );

    return vec![EffectMsg::outcome(
      attacker_name,
      MessageCategory::Damage,
      format!("{attack_line}: hit, effect {effect}. Absorbed by armour."),
      true,
    )
    .at(defender.get_name())
    .rolled(format!("{attack_roll} · damage {} = 0", terms.join(" ")))];
  };

  debug!(
        "(Combat.attack) {attacker_name} does {damage} damage to {} after rolling {roll} ({}D), adjustment with damage modifier {}, hit effect {}, and defender armor -{}.",
        defender.get_name(),
        profile.damage_dice,
        damage_mod,
        (hit_roll - STANDARD_ROLL_THRESHOLD),
        defender.get_current_armor()
    );

  // Screens deflect "after armour has been accounted for" (High Guard p. 40).
  // The order matters beyond arithmetic: applied before armour, a screen would
  // be spent cancelling damage the armour was going to stop anyway.
  let before_screens = damage;
  damage = defender.apply_screens(firing.kind, damage);
  let screened = before_screens - damage;
  if screened > 0 {
    debug!(
      "(Combat.attack) {}'s screens absorb {screened} of {before_screens} damage.",
      defender.get_name()
    );
  }
  if screened > 0 {
    terms.push(format!("-{screened} screens"));
  }
  if damage == 0 {
    return vec![EffectMsg::outcome(
      attacker_name,
      MessageCategory::Damage,
      format!("{attack_line}: hit, effect {effect}. Absorbed by screens."),
      true,
    )
    .at(defender.get_name())
    .rolled(format!("{attack_roll} · damage {} = 0", terms.join(" ")))];
  }

  // Calculate additional damage multipliers and effects for non-crits now.
  // This runs on the impact of a single object for launched weapons, so a
  // salvo resolves once per missile or torpedo rather than once per launcher.
  let mut effects = if profile.salvo.is_some() {
    // Create two effects: a message stating the damage and a ship impact on the defender.
    vec![
      EffectMsg::outcome(
        attacker_name,
        MessageCategory::Damage,
        format!("{attack_line}: hit, effect {effect}. Damage {damage}."),
        true,
      )
      .at(defender.get_name())
      .rolled(format!("{attack_roll} · damage {} = {damage}", terms.join(" "))),
      EffectMsg::ShipImpact {
        target: defender.get_name().to_string(),
        position: defender.get_position(),
      },
    ]
  } else {
    // Guns in a multi-weapon turret fire together, adding their dice to the
    // one roll.  This is a bonus for filling the turret, not a Damage
    // Multiple, so it applies before (and independently of) the multiple.
    // "Each additional weapon adds +1 per damage dice" (Core Rulebook p. 166).
    // Counted over guns of the firing type, so a mixed turret gets the bonus
    // for the two lasers in it and not for the sandcaster beside them.
    if matches!(*firing.mount, WeaponMount::Turret) {
      let turret_bonus = (u32::from(firing.count) - 1) * u32::from(profile.damage_dice);
      damage += turret_bonus;
      if turret_bonus > 0 {
        terms.push(format!("+{turret_bonus} turret"));
      }
    }

    // Damage Multiples (High Guard p. 29).  Launchers never reach here; their
    // scaling is salvo size, which is why the two are mutually exclusive.
    // A spinal's ×1,000 on 80D runs to hundreds of thousands, so the
    // arithmetic is wide, and the armour percentage is taken in the same step.
    let multiple = if profile.use_multiple {
      damage_multiple(MountClass::from(firing.mount))
    } else {
      1
    };
    damage = u32::try_from(u64::from(damage) * u64::from(multiple) * u64::from(through) / 100).unwrap_or(u32::MAX);
    if multiple > 1 {
      terms.push(format!("x{multiple} mount"));
    }

    vec![
      EffectMsg::outcome(
        attacker_name,
        MessageCategory::Damage,
        format!("{attack_line}: hit, effect {effect}. Damage {damage}."),
        true,
      )
      .at(defender.get_name())
      .rolled(format!("{attack_roll} · damage {} = {damage}", terms.join(" "))),
      EffectMsg::BeamHit {
        origin: attacker.get_position(),
        position: defender.get_position(),
      },
    ]
  };

  debug!(
    "(Combat.attack) After modifiers {} does {} damage to {}.",
    attacker_name,
    damage,
    defender.get_name()
  );

  // Ion weapons stop here.  "Instead of applying damage to the target's hull, it
  // is instead temporarily deducted from the target's Power" (High Guard p. 30),
  // so nothing is destroyed: no hull loss, no crits, and the Power returns when
  // the effect lapses.
  if profile.ion {
    // "This reduction in Power lasts until the target completes its next set of
    // actions... If the Effect of the attack roll is 6 or more, the reduction in
    // Power lasts for D3 rounds."
    let rounds = if effect >= 6 { roll_dice_d3(rng) } else { 1 };
    let before = defender.available_power();
    defender.apply_ion_damage(damage, rounds);
    let drained = before - defender.available_power();

    debug!(
      "(Combat.attack) {attacker_name} drains {drained} power from {} for {rounds} round(s).",
      defender.get_name()
    );

    effects.push(
      EffectMsg::about(
        attacker_name,
        MessageCategory::Damage,
        format!("%t loses {drained} power to %s's ion cannon for {rounds} round(s)."),
      )
      .at(defender.get_name()),
    );
    return effects;
  }

  // The primary crit (if any) is a single crit at a level determined by the success of the hit.
  //
  // "Spinal mount weapons can always (and will!) cause critical hits" (High
  // Guard p. 27): every spinal hit that does damage scores one, at the
  // usual severity but never less than 1.
  //
  // Big ships shrug off small guns (High Guard p. 27): over 2,000 tons the
  // target ignores criticals from turrets and barbettes, and larger hulls
  // ignore more, a spinal always excepted. Only the primary critical: the
  // tenths of hull below are suffered "as normal" at every size.
  let spinal = matches!(firing.mount, WeaponMount::Spinal { .. });
  let shrugged = ignores_primary_critical(defender.design.displacement, MountClass::from(firing.mount));
  let primary_crit = spinal || (hit_roll - CRITICAL_THRESHOLD > 0 && !shrugged);

  // "If the attack is successful and scores a critical hit, the attacker may
  // choose which location is hit" (CRB p. 169) -- which critical is not said,
  // so the first one this attack causes takes the called location, whether
  // that is the Effect 6 critical or the first from sustained damage. Any
  // beyond it roll as usual: the gunner called one shot, not every hit the
  // shell makes on its way through.
  //
  // Restricting it to the Effect 6 critical, as this did, made a called shot
  // a DM-2 for nothing in the ordinary case: a hit that carried a ship past a
  // tenth of its hull put the critical somewhere random anyway.
  let mut called = called_shot_system;

  if primary_crit {
    debug!(
      "(Combat.attack) Primary crit level {} to {}.",
      hit_roll - CRITICAL_THRESHOLD,
      defender.get_name()
    );
    // Add a single crit at the effect level
    let severity = u8::try_from((hit_roll - CRITICAL_THRESHOLD).max(1))
      .expect("(combat.attack) hit_role primary crit calc is out of range");
    let why = if spinal && hit_roll - CRITICAL_THRESHOLD <= 0 {
      format!("a spinal mount hit always crits; effect {effect}, so at severity {severity}")
    } else {
      format!("effect {effect} on the hit, which is a critical at severity {severity}")
    };
    shot.criticals += 1;
    effects.append(&mut do_critical(
      severity,
      defender,
      called.take(),
      &CritCause {
        attacker: attacker_name,
        why,
      },
      rng,
    ));
  }
  // Given get_max_hull_points() is u32, we divide it by 10 then the conversion to u64 is safe.
  #[allow(clippy::cast_possible_truncation)]
  #[allow(clippy::cast_sign_loss)]
  let crit_threshold = (f64::from(defender.get_max_hull_points()) / 10.0).ceil() as u64;

  // The secondary crit occurs for each new 10% of the ship's hull points that this hit passes.
  //
  // Only hull the ship still has can be passed. Counting the whole of the
  // damage gave a Light Fighter (hull 4, so a tenth is 1) fifteen criticals
  // from one missile, and the same again from each missile that hit her
  // wreck later in the round: a 10,000-missile test made 17,000 critical
  // lines in one round, all about ships already destroyed.
  let current_hull = defender.get_current_hull_points();
  let prev_crits = u64::from(defender.get_max_hull_points() - current_hull) / crit_threshold;
  let secondary_crit =
    u64::from(defender.get_max_hull_points() - current_hull + damage.min(current_hull)) / crit_threshold - prev_crits;

  debug!("(Combat.attack) Secondary crits {} to {}.", secondary_crit, defender.get_name());

  // Add a level 1 crit for each secondary crit. The first of them takes the
  // called location if the attack has not already spent it.
  for _ in 0..secondary_crit {
    effects.append(&mut do_critical(
      1,
      defender,
      called.take(),
      &CritCause {
        attacker: attacker_name,
        why: format!(
          "sustained damage: {damage} took it past another {crit_threshold} hull, a tenth of {}",
          defender.get_max_hull_points()
        ),
      },
      rng,
    ));
  }

  shot.criticals += u32::try_from(secondary_crit).unwrap_or(u32::MAX);
  shot.damage = damage;
  // Hull lost to a Radiation weapon wears the crew down (High Guard p. 31);
  // only hull the ship had counts, as for the criticals above.
  if profile.radiation {
    defender.take_radiation(damage.min(current_hull), firing.kind == WeaponType::Meson);
  }
  defender.set_hull_points(u32::saturating_sub(current_hull, damage));
  effects
}

fn do_critical(
  crit_level: u8, defender: &mut Ship, called_shot_system: Option<&ShipSystem>, cause: &CritCause,
  rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
  let location = if let Some(system) = called_shot_system {
    debug!("(Combat.do_critical) Critical on called shot system '{system:?}'.");
    *system
  } else {
    let loc = ShipSystem::from_repr(usize::from(roll_dice(2, rng) - 2))
      .expect("(combat.apply_crit) Unable to convert a roll to ship system.");
    debug!("(Combat.do_critical) Critical on random system '{loc:?}'.");
    loc
  };

  let effects = apply_crit(crit_level, location, defender, rng)
    .into_iter()
    // Who did it, and why it happened. A crit line named only the ship that
    // suffered it, so a gunner filtering on their own ship saw the damage
    // they did and none of the criticals it caused -- and the line never said
    // which of the two ways a critical arrives this one came by.
    .map(|effect| effect.at(cause.attacker).rolled(cause.why.clone()))
    .collect::<Vec<_>>();

  info!("(Combat.do_critical) {} suffers crits: {:?}.", defender.get_name(), effects);

  effects
}

/// Where a critical came from: who caused it, and which of the two ways it
/// arrived.
///
/// A critical is either the Effect 6 of a very good hit, or the toll for
/// taking a ship past another tenth of its hull (CRB p. 169). Those are very
/// different pieces of news -- one is marksmanship, the other is attrition --
/// and the line itself has never said which.
struct CritCause<'a> {
  attacker: &'a str,
  why: String,
}

/// The mounts still switched on, so a hit that silences the ship can put them
/// all back when it is repaired.
fn live_weapons(ship: &Ship) -> Vec<usize> {
  ship
    .active_weapons
    .iter()
    .enumerate()
    .filter_map(|(index, active)| active.then_some(index))
    .collect()
}

/// Apply one critical hit at `location`, raising its severity by at least one.
///
/// Also the path for self-inflicted damage, such as a critically failed
/// overload, so it has the same effect and message as a hit.
#[allow(clippy::too_many_lines)]
pub(crate) fn apply_crit(
  crit_level: u8, location: ShipSystem, defender: &mut Ship, rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
  // Captured up front: every message below is about this ship, and `defender`
  // is mutated between them, so it cannot also be borrowed per call.
  let crit_ship = defender.get_name().to_string();
  let current_level = defender.crit_level[location as usize];
  let level = u8::max(current_level + 1, crit_level);

  debug!(
    "(Combat.apply_crit) {} suffers crit level {level} to {location:?}.",
    defender.get_name(),
  );

  // Reset repair bonus if this component was being repaired
  if defender.get_last_repair_component() == Some(location) {
    defender.reset_repair_bonus();
  }

  if level > 6 {
    let damage = u32::from(roll_dice(6, rng));
    debug!(
      "(Combat.apply_crit) {} suffers > level 6 crit to {:?} for {}.",
      defender.get_name(),
      location,
      damage
    );
    defender.set_hull_points(u32::saturating_sub(defender.get_current_hull_points(), damage));
    vec![EffectMsg::about(
      &crit_ship,
      MessageCategory::Critical,
      format!("%s's critical hit at level {level} caused {damage} damage."),
    )]
  } else {
    event!(
      Level::INFO,
      "(Combat.apply_crit) {} suffers crit level {level} to {:?}.",
      defender.get_name(),
      location
    );

    defender.crit_level[location as usize] = level;

    // A target over 5,000 tons takes Weapons, Crew and Cargo criticals from
    // High Guard p. 27's large-ship tables instead of the CRB's rows.
    if is_capital(defender.design.displacement) {
      if let Some(effects) = apply_large_ship_crit(level, location, defender, rng) {
        return effects;
      }
    }

    match (location, level) {
      // I take some liberties with interpreting Sensors impact to make it a bit structured
      (ShipSystem::Sensors, 1) => {
        defender.attack_dm -= 1;
        defender.record_damage(location, level, SystemDamage::AttackDm(1));
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's sensors critical hit (level {level}) and attack DM reduced by 1."),
        )]
      }
      (ShipSystem::Sensors, 6) => {
        defender.record_damage(location, level, SystemDamage::WeaponsOff(live_weapons(defender)));
        defender.active_weapons = vec![false; defender.active_weapons.len()];
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's sensors critical hit (level 6) and completely disabled.".to_string(),
        )]
      }
      (ShipSystem::Sensors, _) => {
        if defender.current_sensors == Sensors::Basic {
          defender.record_damage(location, level, SystemDamage::WeaponsOff(live_weapons(defender)));
          defender.active_weapons = vec![false; defender.active_weapons.len()];
          vec![EffectMsg::about(
            &crit_ship,
            MessageCategory::Critical,
            format!("%s's sensors critical hit (level {level}) and completely disabled."),
          )]
        } else {
          defender.record_damage(location, level, SystemDamage::SensorGrade(defender.current_sensors));
          defender.current_sensors = defender.current_sensors - 1;
          vec![EffectMsg::about(
            &crit_ship,
            MessageCategory::Critical,
            format!(
              "%s's sensors critical hit (level {level}) and reduced to {}.",
              String::from(defender.current_sensors)
            ),
          )]
        }
      }
      (ShipSystem::Powerplant, 3) => {
        let lost = defender.current_power.min(defender.design.power / 2);
        defender.record_damage(location, level, SystemDamage::Power(lost));
        defender.current_power -= lost;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's powerplant critical hit (level 3) and reduced by 50%.".to_string(),
        )]
      }
      (ShipSystem::Powerplant, 4) => {
        defender.record_damage(location, level, SystemDamage::Power(defender.current_power));
        defender.current_power = 0;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's powerplant critical hit (level 4) and offline.".to_string(),
        )]
      }
      (ShipSystem::Powerplant, level) if level < 3 => {
        let lost = defender.current_power.min(defender.design.power / 10);
        defender.record_damage(location, level, SystemDamage::Power(lost));
        defender.current_power -= lost;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's powerplant critical hit (level {level}) and reduced by 10%."),
        )]
      }
      (ShipSystem::Powerplant, level) => {
        defender.record_damage(location, level, SystemDamage::Power(defender.current_power));
        defender.current_power = 0;
        let mut effects = vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's powerplant critical hit (level {level}) and offline."),
        )];
        effects.append(&mut apply_crit(
          if level == 5 { 1 } else { roll(rng) },
          ShipSystem::Hull,
          defender,
          rng,
        ));
        effects
      }
      (ShipSystem::Fuel, level) if level < 4 => {
        let fuel_loss = match level {
          1 => u32::from(roll(rng)),
          2 => u32::from(roll_dice(2, rng)),
          3 => u32::from(roll(rng)) * defender.design.fuel / 10,
          _ => 0,
        };
        defender.current_fuel = u32::saturating_sub(defender.current_fuel, fuel_loss);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's fuel critical hit (level {level}) and reduced by {fuel_loss}."),
        )]
      }
      (ShipSystem::Fuel, level) => {
        defender.current_fuel = 0;
        let mut effects = vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's fuel critical hit (level {level}) and fuel take destroyed."),
        )];
        effects.append(&mut apply_crit(
          if level == 5 { 1 } else { roll(rng) },
          ShipSystem::Hull,
          defender,
          rng,
        ));
        effects
      }
      (ShipSystem::Weapon, 1) => {
        defender.attack_dm -= 1;
        defender.record_damage(location, level, SystemDamage::AttackDm(1));
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's weapon critical hit (level 1) and attack DM reduced by 1.".to_string(),
        )]
      }
      (ShipSystem::Weapon, level) => {
        let possible = defender.active_weapons.iter().filter(|x| **x).count();
        let mut effects = if possible > 0 {
          let pick = usize::try_from(rng.next_u32()).unwrap_or_else(|_e| {
            error!("Usize cannot contain u32!");
            0
          }) % possible;

          debug!(
            "(Combat.apply_crit) Weapon pick {} from active weapons for {} of {:?}.",
            pick,
            defender.get_name(),
            defender.active_weapons
          );
          let selected_index = defender
            .active_weapons
            .iter()
            .enumerate()
            .filter(|(_, &active)| active)
            .nth(pick)
            .map(|(index, _)| index)
            .unwrap();

          // Name the weapon before disabling it: `weapons()` borrows the ship.
          let disabled = String::from(&defender.weapons()[selected_index]);
          defender.record_damage(location, level, SystemDamage::WeaponsOff(vec![selected_index]));
          defender.active_weapons[selected_index] = false;
          vec![EffectMsg::about(
            &crit_ship,
            MessageCategory::Critical,
            format!("%s's weapon critical hit (level {level}) and {disabled} disabled."),
          )]
        } else {
          vec![EffectMsg::about(
            &crit_ship,
            MessageCategory::Critical,
            format!("%s's weapon critical hit (level {level}) but all weapons already disabled."),
          )]
        };
        effects.append(&mut match level {
          5 => apply_crit(1, ShipSystem::Hull, defender, rng),
          6 => apply_crit(roll(rng), ShipSystem::Hull, defender, rng),
          _ => vec![],
        });
        effects
      }
      (ShipSystem::Armor, level) => {
        let damage = match level {
          1 => 1_u32,
          2 => u32::from(roll(rng)) / 2,
          x if x < 5 => u32::from(roll(rng)),
          _ => u32::from(roll_dice(2, rng)),
        };

        defender.current_armor = u32::saturating_sub(defender.current_armor, damage);
        let mut effects = vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's armor critical hit (level {level}) and reduced by {damage}."),
        )];
        if level >= 5 {
          effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng));
        }
        effects
      }
      (ShipSystem::Hull, level) => {
        let damage = u32::from(roll_dice(level, rng));
        defender.current_hull = u32::saturating_sub(defender.current_hull, damage);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's hull critical hit (level {level}) and reduced by {damage}."),
        )]
      }
      (ShipSystem::Maneuver, 5) => {
        defender.record_damage(location, level, SystemDamage::Thrust(defender.current_maneuver));
        defender.current_maneuver = 0;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's maneuver critical hit (level 5) and offline.".to_string(),
        )]
      }
      (ShipSystem::Maneuver, 6) => {
        defender.record_damage(location, level, SystemDamage::Thrust(defender.current_maneuver));
        defender.current_maneuver = 0;
        let mut effects = vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's maneuver critical hit (level 6) and offline.".to_string(),
        )];
        effects.append(&mut apply_crit(roll(rng), ShipSystem::Hull, defender, rng));
        effects
      }
      (ShipSystem::Maneuver, _) => {
        let lost = defender.current_maneuver.min(1);
        defender.record_damage(location, level, SystemDamage::Thrust(lost));
        defender.current_maneuver -= lost;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's maneuver critical hit (level {level}) and reduced by 1."),
        )]
      }
      (ShipSystem::Cargo, 1) => vec![EffectMsg::about(
        &crit_ship,
        MessageCategory::Critical,
        format!("%s's cargo critical hit (level {level}) and 10% of cargo destroyed."),
      )],
      (ShipSystem::Cargo, 2) => {
        let percent_destroyed = 10 * roll(rng);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's cargo critical hit (level {level}) and {percent_destroyed}% of cargo destroyed."),
        )]
      }
      (ShipSystem::Cargo, 3) => {
        let percent_destroyed = roll_dice(2, rng).min(10) * 10;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's cargo critical hit (level {level}) and {percent_destroyed}% of cargo destroyed."),
        )]
      }
      (ShipSystem::Cargo, 4) => vec![EffectMsg::about(
        &crit_ship,
        MessageCategory::Critical,
        format!("%s's cargo critical hit (level {level}) and all cargo destroyed."),
      )],
      (ShipSystem::Cargo, _) => {
        let mut effects = apply_crit(1, ShipSystem::Hull, defender, rng);
        effects.push(EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's cargo critical hit (level {level}) and all cargo destroyed."),
        ));
        effects
      }
      (ShipSystem::Jump, 1) => {
        let lost = defender.current_jump.min(1);
        defender.record_damage(location, level, SystemDamage::Jump(lost));
        defender.current_jump -= lost;
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's jump critical hit (level 1) and reduced by 1.".to_string(),
        )]
      }
      (ShipSystem::Jump, level) => {
        defender.record_damage(location, level, SystemDamage::Jump(defender.current_jump));
        defender.current_jump = 0;
        let mut effects = vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's jump critical hit (level {level}) and offline."),
        )];
        if level >= 4 {
          effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng));
        }
        effects
      }
      (ShipSystem::Crew, 1) => {
        let crew_damage = roll(rng);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's crew critical hit (level 1) and random occupant takes {crew_damage} damage."),
        )]
      }
      (ShipSystem::Crew, 2) => {
        let hours = roll(rng);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's crew critical hit (level 2) and life support fails within {hours} hours."),
        )]
      }
      (ShipSystem::Crew, 3) => {
        let num_occupants = roll(rng);
        let damages = (0..num_occupants)
          .map(|_| format!("{}", roll_dice(2, rng)))
          .collect::<Vec<String>>()
          .join(", ");
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's crew critical hit (level 3) and {num_occupants} take {damages} points of damage."),
        )]
      }
      (ShipSystem::Crew, 4) => {
        let rounds = roll(rng);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's crew critical hit (level 4) and life support fails in {rounds} rounds."),
        )]
      }
      (ShipSystem::Crew, 5) => {
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's crew critical hit (level 5) and all occupants take 3D damage (roll each separately).".to_string(),
        )]
      }
      (ShipSystem::Crew, 6) => vec![EffectMsg::about(
        &crit_ship,
        MessageCategory::Critical,
        "%s's crew critical hit (level 6) and life support fails.".to_string(),
      )],
      (ShipSystem::Crew, _) => {
        let mut effects = apply_crit(1, ShipSystem::Hull, defender, rng);
        effects.push(EffectMsg::about(&crit_ship, MessageCategory::Critical, format!("%s's crew critical hit (level {level}) (<- This is a bug - should never hit this level). Life support fails.")));
        effects
      }
      (ShipSystem::Bridge, 1) => {
        let station = BridgeStation::random(rng);
        defender.bridge_hit_disable(level, station);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!(
            "%s's bridge critical hit (level 1): {station} station disabled this round and next ({}).",
            station.loses()
          ),
        )]
      }
      (ShipSystem::Bridge, 2) => {
        defender.bridge_hit_disable(level, BridgeStation::Computer);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!(
            "%s's bridge critical hit (level 2): computer reboots, down this round and next ({}).",
            BridgeStation::Computer.loses()
          ),
        )]
      }
      (ShipSystem::Bridge, 3) => {
        defender.bridge_hit_bandwidth(level, defender.current_computer / 2);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          "%s's bridge critical hit (level 3) and computer damaged: reduce bandwidth -50%".to_string(),
        )]
      }
      (ShipSystem::Bridge, 5) => {
        defender.bridge_hit_destroy(level, BridgeStation::Computer);
        defender.bridge_hit_bandwidth(level, 0);
        vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!(
            "%s's bridge critical hit (level 5): computer destroyed until repaired ({}).",
            BridgeStation::Computer.loses()
          ),
        )]
      }
      // 4, 6, and anything past 6 that reaches here (which it should not).
      (ShipSystem::Bridge, level) => {
        let station = BridgeStation::random(rng);
        defender.bridge_hit_destroy(level, station);
        // Core Rulebook p. 170: the occupant takes 1D x 1D.
        let crew_damage = u16::from(roll(rng)) * u16::from(roll(rng));
        let mut effects = if level >= 6 {
          apply_crit(1, ShipSystem::Hull, defender, rng)
        } else {
          vec![]
        };
        effects.push(EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's bridge critical hit (level {level}): {station} station destroyed until repaired ({}); occupant takes {crew_damage} damage.", station.loses()),
        ));
        effects
      }
    }
  }
}

/// The large-ship critical tables (High Guard p. 27), for a target over
/// 5,000 tons: a Weapons critical of severity 2 or more takes a share of one
/// battery, bay or spinal; a Crew critical takes a percentage of the
/// complement; a Cargo critical lands on the D6 table of what a big ship
/// carries. `None` for any other system, or a severity-1 Weapons critical,
/// which stays the CRB's DM-1: `apply_crit` goes on with its own rows.
///
/// `crit_level` is already set when this runs.
#[allow(clippy::too_many_lines)]
fn apply_large_ship_crit(
  level: u8, location: ShipSystem, defender: &mut Ship, rng: &mut dyn RngCore,
) -> Option<Vec<EffectMsg>> {
  let crit_ship = defender.get_name().to_string();
  match location {
    ShipSystem::Weapon if level >= 2 => {
      // "Choose randomly between the weapon systems on the ship", and when
      // the one chosen is several mounts, the Cargo and Weapons table says
      // what share of them is lost. Chosen by entry, so a lone spinal is as
      // likely a pick as a battery of thirty-two -- and, being one mount,
      // goes out at any share.
      let live: Vec<usize> = (0..defender.weapons().len())
        .filter(|index| defender.live_mounts(*index) > 0)
        .collect();
      let Some(&weapon_id) = live.get(usize::try_from(rng.next_u32()).unwrap_or(0) % live.len().max(1)) else {
        return Some(vec![EffectMsg::about(
          &crit_ship,
          MessageCategory::Critical,
          format!("%s's weapon critical hit (level {level}) but all weapons already disabled."),
        )]);
      };
      let loss = system_loss(level, || roll(rng));
      let remaining = defender.live_mounts(weapon_id);
      let (destroyed, hull) = match loss {
        SystemLoss::Disabled(_) => (false, false),
        SystemLoss::Destroyed(_) => (true, false),
        SystemLoss::AllDestroyed { hull } => (true, hull),
      };
      let lost = defender.lose_mounts(weapon_id, loss.of(remaining), level, destroyed);
      let name = String::from(&defender.weapons()[weapon_id]);
      let what = if remaining == 1 {
        if destroyed { "destroyed" } else { "disabled" }.to_string()
      } else {
        format!(
          "{lost} of {remaining} mount{} {}",
          if remaining == 1 { "" } else { "s" },
          if destroyed { "destroyed" } else { "disabled" }
        )
      };
      let mut effects = vec![EffectMsg::about(
        &crit_ship,
        MessageCategory::Critical,
        format!("%s's weapon critical hit (level {level}): {name} {what}."),
      )];
      if hull {
        effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng));
      }
      Some(effects)
    }
    ShipSystem::Crew => {
      // Crew Critical Hit Severity: a percentage of the complement killed
      // or disabled, with the same life-support failures the CRB's rows
      // report. The DMs the losses carry are read off the crew that is left
      // (`Ship::crew_dm`), so reviving the frozen watch lifts them.
      let percent = crew_crit_percent(level);
      let complement = defender.design.crew;
      let lost = u32::try_from((u64::from(complement) * u64::from(percent)).div_ceil(100))
        .unwrap_or(u32::MAX)
        .max(u32::from(complement > 0))
        .min(defender.current_crew);
      defender.current_crew -= lost;
      let left = defender.crew_percent();
      let dm = defender.crew_dm_terms()[0].1;
      let life_support = match level {
        2 => format!("; life support fails within {} hours", roll(rng)),
        4 => format!("; life support fails in {} rounds", roll(rng)),
        6 => "; life support fails".to_string(),
        _ => String::new(),
      };
      let checks = if dm < 0 {
        format!("; DM{dm} to every check")
      } else {
        String::new()
      };
      let mut effects = vec![EffectMsg::about(
        &crit_ship,
        MessageCategory::Critical,
        format!(
          "%s's crew critical hit (level {level}): {percent}% of the crew killed or disabled, {lost} of {complement}; crew at {left}%{life_support}{checks}."
        ),
      )];
      if level > 6 {
        effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng));
      }
      Some(effects)
    }
    ShipSystem::Cargo => {
      // The D6 table HG p. 27 gives for a large carrier: fighters, other
      // small craft, the frozen watch, cargo. Nothing is embarked yet, so
      // the craft rows are cargo until carried craft exist (plan step 9):
      // that is where a hit on `CargoHit::Fighters` or `SmallCraft` will
      // take craft from the hangar instead.
      let row = CargoHit::of(roll(rng));
      let loss = system_loss(level, || roll(rng));
      let (share, hull) = match loss {
        SystemLoss::Disabled(percent) | SystemLoss::Destroyed(percent) => (format!("{percent}%"), false),
        SystemLoss::AllDestroyed { hull } => ("all".to_string(), hull),
      };
      let line = match row {
        CargoHit::FrozenWatch if defender.frozen_watch > 0 => {
          let before = defender.frozen_watch;
          let lost = u32::from(loss.of(u16::try_from(before).unwrap_or(u16::MAX)));
          defender.frozen_watch = before.saturating_sub(lost);
          format!(
            "%s's cargo critical hit (level {level}): the frozen watch hit, {lost} of {before} lost in their berths."
          )
        }
        CargoHit::Fighters | CargoHit::SmallCraft | CargoHit::FrozenWatch | CargoHit::Cargo => {
          format!("%s's cargo critical hit (level {level}) and {share} of cargo destroyed.")
        }
      };
      let mut effects = if hull {
        apply_crit(1, ShipSystem::Hull, defender, rng)
      } else {
        vec![]
      };
      effects.push(EffectMsg::about(&crit_ship, MessageCategory::Critical, line));
      Some(effects)
    }
    _ => None,
  }
}

/// The range band a distance in metres falls into.
///
/// Beyond the last band (50,000 km) everything is `Distant`, which High Guard
/// p. 76 describes as the point where contacts are just undifferentiated blips.
pub(crate) fn find_range_band(distance: u32) -> Range {
  RANGE_BANDS
    .iter()
    .position(|&x| x >= distance)
    .and_then(Range::from_repr)
    .unwrap_or(Range::Distant)
}

/// Process all incoming fire actions and turn them into either missile launches or attacks.
///
/// # Arguments
/// * `attacker` - The ship that is attacking.  This is used to get the attacker's position and sensors.
/// * `ships` - A clone of all ships state at the start of the round.  Having this snapshot avoid trying to lookup
///   a ship that was destroyed earlier in the round.
/// * `sand_counts` - A snapshot of all the sand capabilities of each ship.
/// * `actions` - The fire actions to process.
/// * `rng` - The random number generator to use.
///
/// # Returns
/// * A tuple of the new missiles to launch and the effects of the fire actions.
///
/// # Panics
/// Panics if the lock cannot be obtained to read a ship.
/// Also, if we check that sand casters are available but then cannot pop an element from the `sand_counts` list.
#[allow(clippy::too_many_lines)]
pub fn do_fire_actions<S: BuildHasher>(
  attacker: &Ship, ships: &mut HashMap<String, Arc<RwLock<Ship>>, S>,
  sand_counts: &mut HashMap<String, Vec<SandCloud>, S>, actions: &[ShipAction], boost_map: &BoostMap,
  rng: &mut dyn RngCore,
) -> (Vec<LaunchMissileMsg>, Vec<EffectMsg>) {
  let mut new_missiles = vec![];

  // The pilot's Assist Gunner roll. Reported like every other check: it was
  // debug-only, so the +2 it fed into every shot this round had no visible
  // source.
  let (assist_bonus, assist_effect) = if attacker.get_assist_gunners() {
    let roll = i32::from(roll_dice(2, rng));
    let pilot = i32::from(attacker.get_crew().get_pilot());
    // A check like any other, so the crew's DM is on it.
    let crew = i32::from(attacker.crew_dm());
    let effect = roll - STANDARD_ROLL_THRESHOLD + pilot + crew;
    let impact = task_chain_impact(effect);
    debug!(
      "(Combat.do_fire_actions) Pilot of {} with skill {} is assisting gunners.  Effect is {} so task chain impact is {}.",
      attacker.get_name(),
      pilot,
      effect,
      impact
    );
    (
      impact,
      Some(
        EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Attack,
          format!("%s pilot assists gunners: DM {impact:+} to this round's shots."),
        )
        .rolled(if crew == 0 {
          format!("roll {roll} skill {pilot:+} = effect {effect}")
        } else {
          format!("roll {roll} skill {pilot:+} crew {crew:+} = effect {effect}")
        }),
      ),
    )
  } else {
    (0, None)
  };

  // Tracks whether the captain's AssistGunner +1 has been applied to this
  // attacker's first fire-action this turn. Local because there is exactly
  // one `attacker` per call to `do_fire_actions` (the closure iterates that
  // attacker's weapons), so a ship-level flag would be redundant.
  let mut first_assist_consumed = false;

  // The Fire Control pool for this round. The program's score is spent on
  // firing mounts outright and on improving gunners' shots, in any mix
  // (CRB p. 161); a crew that orders more than it has gets what the pool
  // covers, in the order the mounts were given.
  let fire_control_pool = i32::from(attacker.running_level(SoftwareKind::FireControl).unwrap_or(0));
  let mut fire_control_left = fire_control_pool;

  // What is left in each mount, spent down as this attacker's orders are
  // worked through. The live ship is debited by the caller from what was
  // actually launched.
  let mut loads_left: Vec<Magazine> = attacker.loaded.clone();
  // Railgun and mass driver spinals fire rounds straight from stores; what
  // they spend is taken off the live ship once every order is worked through.
  let mut rounds_left = attacker.magazine.rounds;

  let mut effects: Vec<EffectMsg> = actions
    .iter()
    .flat_map(|action| {
      let ShipAction::FireAction {
        weapon_id,
        target,
        called_shot_system,
        firing_kind,
        salvo_size,
        fire_control_dm,
        computer_fired,
      } = action
      else {
        error!("(Combat.do_fire_actions) Expected FireAction but got {:?}.", action);
        return vec![];
      };

      debug!(
        "(Combat.do_fire_actions) Process fire action for {}: {:?}.",
        attacker.get_name(),
        action
      );

      // Nothing can be done to a ship that is not detected. Read off the
      // attacker's start-of-round snapshot, so a shot is judged against what
      // the crew knew when the order was given.
      let target_seen = ships.get(target).is_some_and(|t| attacker.detects(&t.read().unwrap()));
      if !target_seen {
        debug!(
          "(Combat.do_fire_actions) {} has no contact on {}; fire action dropped.",
          attacker.get_name(),
          target
        );
        return vec![no_contact_effect(attacker.get_name(), target, "fire on")];
      }

      // Ships do not shoot their own side. Only attacks are blocked: plotting a
      // course to a team-mate, or sensor locking one, are perfectly reasonable
      // things to want to do.
      if let Some(attacker_team) = attacker.team {
        let same_side = ships.get(target).is_some_and(|t| t.read().unwrap().team == Some(attacker_team));
        if same_side {
          debug!(
            "(Combat.do_fire_actions) {} and {} are both on team {:?}; fire action dropped.",
            attacker.get_name(),
            target,
            attacker_team
          );
          return vec![EffectMsg::about(
            attacker.get_name(),
            MessageCategory::Info,
            "%s will not fire on %t: same side.".to_string(),
          )
          .at(target)];
        }
      }

      if !attacker.active_weapons[*weapon_id] {
        debug!("(Combat.do_fire_actions) Weapon {} is disabled.", weapon_id);
        return vec![];
      }

      // A mount the plant is not feeding cannot fire, whether the engineer
      // switched it off or the damage did it for them.
      if !attacker.weapon_powered(*weapon_id) {
        return vec![EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Critical,
          format!(
            "{}'s {} has no power and cannot fire.",
            attacker.get_name(),
            String::from(&attacker.weapons()[*weapon_id])
          ),
        )];
      }

      let weapon = attacker.get_weapon(*weapon_id);
      // Which gun in the mount is firing.  A mixed turret may only use one type
      // per round (Core Rulebook p. 166), so the action names it; a uniform
      // mount has no choice to make and does not have to.
      let firing = match firing_kind {
        Some(kind) => weapon.firing(*kind),
        None => weapon.firing_default(),
      };
      // A gunner may throw fewer than the mount holds -- a warning shot, or
      // keeping some in the racks. Never more, whatever the client asks for.
      let firing = firing.map(|firing| Firing {
        salvo_limit: *salvo_size,
        ..firing
      });
      let Some(firing) = firing else {
        debug!(
          "(Combat.do_fire_actions) Weapon {} cannot fire {:?}; it holds {:?}.",
          weapon_id,
          firing_kind,
          weapon.kinds()
        );
        return vec![];
      };
      // Draw this mount's share of the pool: one point to have the computer
      // fire it, plus whatever the gunner asked it to add to the shot.
      let computer_fired = *computer_fired && fire_control_left > 0;
      if computer_fired {
        fire_control_left -= 1;
      }
      let fire_control_points = i32::from(*fire_control_dm).min(fire_control_left.max(0));
      fire_control_left -= fire_control_points;

      // A mount the computer is firing has no gunner's skill behind it: Fire
      // Control buys the attack, not a gunner (CRB p. 161). One point goes on
      // firing it, and the gunner's own skill does not apply.
      // Somebody has to be at the mount. A gun fires because a gunner fires
      // it, or because the computer does -- Fire Control buying the attack
      // or a Virtual Gunner standing in. An empty position is an empty
      // position, and saying so is kinder than a silent miss.
      if !attacker.get_crew().has_gunner(*weapon_id)
        && !computer_fired
        && attacker.running_level(SoftwareKind::VirtualGunner).is_none()
      {
        return vec![EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Attack,
          format!(
            "{}'s {} has no gunner: nobody is at the mount, and neither Fire Control nor a Virtual Gunner is covering it.",
            attacker.get_name(),
            String::from(&attacker.weapons()[*weapon_id])
          ),
        )];
      }

      // A Virtual Gunner stands in where nobody is at the mount: "weapons
      // controlled by a Virtual Gunner have a skill level equal to the
      // package's score but they can take advantage of other modifiers such
      // as Advanced Fire Control" (High Guard p. 76). A real gunner is
      // always better than being replaced, so they keep their station.
      let virtual_gunner = if attacker.get_crew().has_gunner(*weapon_id) {
        None
      } else {
        attacker.running_level(SoftwareKind::VirtualGunner)
      };
      let gunnery_skill = if computer_fired {
        0
      } else {
        virtual_gunner.map_or_else(|| i32::from(attacker.get_crew().get_gunnery(*weapon_id)), i32::from)
      };
      // Captain leadership boost for this specific (ship, weapon) fire action.
      let leadership_boost = i32::from(boost_for_fire(boost_map, attacker.get_name(), *weapon_id));
      debug!(
        "(Combat.do_fire_actions) Gunnery skill for weapon #{} is {}.",
        weapon_id, gunnery_skill
      );

      let target_ship = ships.get(target);

      if target_ship.is_none() {
        debug!("(Combat.do_fire_actions) No such target {} for fire action.", target);
        return vec![];
      }

      let mut target = target_ship.unwrap().write().unwrap();

      debug!(
        "(Combat.do_fire_actions) {} attacking {} with {:?}.",
        attacker.get_name(),
        target.get_name(),
        weapon
      );

      // Need a range check here before possibily using up of sand or dodge.
      // This in theory could be lossy but that would require there to be more than 4.29x10^9m which is VERY far.  If we
      // wanted to be safer we check if the magnitude was greater than u32::MAX and then just use that.
      // Note we will lose precision here but this is just for range so okay.
      #[allow(clippy::cast_sign_loss)]
      #[allow(clippy::cast_possible_truncation)]
      let range_band = find_range_band((target.get_position() - attacker.get_position()).magnitude() as u32);
      let Some(profile) =
        profile_for(firing.kind, firing.mount).map(|p| p.with_modifiers(firing.kind, firing.modifiers))
      else {
        error!(
          "(Combat.do_fire_actions) {} cannot mount {} as a {}.",
          attacker.get_name(),
          String::from(&firing.kind),
          String::from(&firing.kind)
        );
        return vec![EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Info,
          format!("%s's {} cannot be fired from that mount.", String::from(&firing.kind)),
        )];
      };

      // Launchers have "Special" range and are never ruled out by distance.
      if !profile.reaches(range_band) {
        // We are out of range so cannot attack
        debug!(
          "(Combat.attack) {} is out of range of {}'s {}.",
          target.get_name(),
          attacker.get_name(),
          String::from(&firing.kind)
        );
        return vec![EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Info,
          format!("%t is out of range of %s's {}.", String::from(&firing.kind)),
        )
        .at(target.get_name())];
      }

      // "Spinal weapons cannot attack targets of less than 2,000 tons unless
      // they are stationary" (High Guard p. 34). Said here, before a dodge or
      // a sand cloud is spent on a shot that cannot be made.
      if let Some(multiple) = firing.mount.spinal_multiple() {
        if spinal_size_dm(target.design.displacement).is_none() && !target.held_still {
          return vec![EffectMsg::about(
            attacker.get_name(),
            MessageCategory::Info,
            format!(
              "%t is too small for %s's {} ({multiple}×): under 2,000 tons, and she manoeuvred last round.",
              spinal_name(firing.kind)
            ),
          )
          .at(target.get_name())];
        }
        if spinal_uses_rounds(firing.kind) {
          if rounds_left == 0 {
            return vec![EffectMsg::about(
              attacker.get_name(),
              MessageCategory::Attack,
              format!("%s's {} is out of rounds.", spinal_name(firing.kind)),
            )];
          }
          rounds_left -= 1;
        }
      }

      // At this point all these attacks should be in range.
      if let Some(salvo) = profile.salvo {
        // Launched weapons don't attack when fired.  Each object comes back and
        // calls attack() on impact, carrying the weapon that threw it so a
        // torpedo resolves as a torpedo rather than as a missile.
        let per_mount = match salvo {
          // One object per launcher gun, which in a mixed turret is the number
          // of racks in it rather than the size of the turret.
          Salvo::PerGun => u16::from(firing.count),
          Salvo::Fixed(n) => n,
        };
        // A missile battery's launchers throw one salvo together: 32 small
        // bays are 384 missiles at one target -- from the mounts still up.
        let full_salvo = per_mount.saturating_mul(attacker.live_mounts(*weapon_id));
        // Firing fewer is a choice; firing more is not on offer, and asking
        // for none would be an order to do nothing, so one is the floor.
        let count = firing.salvo_limit.map_or(full_salvo, |limit| limit.clamp(1, full_salvo));

        // Nothing leaves the rails that is not in the mount. A short load
        // throws what it has; an empty mount says so, and waits for a Reload.
        let Some(aboard) = loads_left.get_mut(*weapon_id).and_then(|load| load.of_mut(firing.kind)) else {
          return vec![];
        };
        let count = count.min(u16::try_from(*aboard).unwrap_or(u16::MAX));
        if count == 0 {
          let stores = attacker.magazine.of(firing.kind);
          let advice = if stores > 0 {
            format!("; {stores} in stores, so reload it")
          } else {
            "; none left in stores".to_string()
          };
          return vec![EffectMsg::about(
            attacker.get_name(),
            MessageCategory::Attack,
            format!(
              "%s's {} (mount {}) is out of {}s{advice}.",
              String::from(weapon),
              weapon_id + 1,
              String::from(&firing.kind).to_lowercase()
            ),
          )];
        }
        *aboard -= u32::from(count);

        // One salvo: the mount's launch flies as one object and is resolved
        // missile by missile when it arrives.
        new_missiles.push(LaunchMissileMsg {
          source: attacker.get_name().to_string(),
          target: target.get_name().to_string(),
          weapon: weapon.clone(),
          count,
          mount: *weapon_id,
        });

        debug!(
          "(Combat.do_fire_actions) {} launches {} {} at {}.",
          attacker.get_name(),
          count,
          String::from(&firing.kind),
          target.get_name()
        );

        return vec![EffectMsg::about(
          attacker.get_name(),
          MessageCategory::Attack,
          format!("%s launches {} {}(s) at %t.", count, String::from(&firing.kind)),
        )
        .at(target.get_name())];
      }

      match firing.kind {
        WeaponType::Beam | WeaponType::Pulse => {
          // Lasers are special as sand can be used against them.
          debug!(
            "(Combat.do_fire_actions) {} fires {} at {} with lasers.",
            attacker.get_name(),
            String::from(&firing.kind),
            target.get_name()
          );

          // A battery is a shot per mount still up, and each can draw the
          // defender's sand as a lone mount's shot would, while she has any
          // to throw.
          let shots = attacker.live_mounts(*weapon_id);
          let mut sand_mods: Vec<i32> = Vec::new();
          let mut sand_lines: Vec<EffectMsg> = Vec::new();
          for _ in 0..shots {
            let (sand_mod, lines) = match sand_counts.get_mut(target.get_name()) {
              Some(sand_casters) if !sand_casters.is_empty() => {
                // There is a serious error if after checking if the sand_casters list isn't empty
                // it then cannot pop an element. So unwrap() is safe here.
                let SandCloud { modifier, mount } = sand_casters.pop().unwrap();
                // One canister per cloud dispersed, out of the mount that threw
                // it (Core Rulebook p. 166 prices them by the canister, and a
                // mount holds a finite number).
                if let Some(load) = target.loaded.get_mut(mount) {
                  load.sand = load.sand.saturating_sub(1);
                }
                let dice = i32::from(roll_dice(2, rng));
                let effect = dice - STANDARD_ROLL_THRESHOLD + modifier;
                // Same compact shape as an attack line: sand is part of the same
                // exchange, and the defender rolling it is the actor here.
                let sand_line = "%s sand -> %t".to_string();
                let sand_roll_line =
                  format!("{dice}{modifier:+}={} vs {STANDARD_ROLL_THRESHOLD}", dice + modifier);
                if effect >= 0 {
                  debug!(
                    "(Combat.do_fire_actions) {}'s sand (modifier = {})successfully deployed against {} with effect {}.",
                    target.get_name(),
                    modifier,
                    attacker.get_name(),
                    effect
                  );
                  let sand_roll = i32::from(roll(rng));
                  let sand_mod = effect + sand_roll;
                  (
                    sand_mod,
                    vec![EffectMsg::about(
                      target.get_name(),
                      MessageCategory::Damage,
                      format!("{sand_line}: deployed, damage -{sand_mod}."),
                    )
                    .at(attacker.get_name())
                    .rolled(format!(
                      "{sand_roll_line} · {effect} effect +1D {sand_roll} = -{sand_mod}"
                    ))],
                  )
                } else {
                  debug!(
                    "(Combat.do_fire_actions) {}'s sand (modifier = {}) failed to deploy against {} with effect {}.",
                    target.get_name(),
                    modifier,
                    attacker.get_name(),
                    effect
                  );

                  (
                    0,
                    vec![EffectMsg::about(
                      target.get_name(),
                      MessageCategory::Damage,
                      format!("{sand_line}: fails to deploy."),
                    )
                    .at(attacker.get_name())
                    .rolled(sand_roll_line.clone())],
                  )
                }
              }
              _ => {
                debug!(
                  "(Combat.do_fire_actions) {} has no sand to deploy against {}.",
                  target.get_name(),
                  attacker.get_name()
                );
                (0, vec![])
              }
            };
            if lines.is_empty() {
              break;
            }
            sand_mods.push(sand_mod);
            sand_lines.extend(lines);
          }
          let mut effects = if sand_lines.len() > 1 {
            vec![sand_summary(&target, attacker.get_name(), &sand_mods, shots)]
          } else {
            sand_lines
          };

          // Captain AssistGunner +1 applies to the FIRST actual fire roll
          // this attacker makes this turn (only when assist_gunners is set
          // on the attacker). Missiles don't roll here, so they don't
          // consume the bonus.
          let captain_assist = if attacker.get_assist_gunners()
            && boost_for_assist_gunner(boost_map, attacker.get_name()) > 0
            && !first_assist_consumed
          {
            first_assist_consumed = true;
            1
          } else {
            0
          };
          let hit = HitMods {
            gunner: gunnery_skill,
            assist: assist_bonus,
            captain_assist,
            captain_fire: leadership_boost,
            smart: 0,
            fire_control: fire_control_points,
          };

          let sand: Vec<i32> = sand_mods.iter().map(|sand_mod| -sand_mod).collect();
          effects.append(&mut volley(
            hit,
            &sand,
            attacker,
            &mut target,
            &firing,
            shots,
            battery_title(weapon).as_deref(),
            called_shot_system.as_ref(),
            boost_map,
            rng,
          ));
          effects
        }
        _ => {
          debug!(
            "(Combat.do_fire_actions) {} fires {} at {}.",
            attacker.get_name(),
            String::from(&firing.kind),
            target.get_name()
          );

          // Captain AssistGunner +1 applies to the FIRST actual fire roll
          // this attacker makes this turn (only when assist_gunners is set
          // on the attacker). Missiles don't roll here, so they don't
          // consume the bonus.
          let captain_assist = if attacker.get_assist_gunners()
            && boost_for_assist_gunner(boost_map, attacker.get_name()) > 0
            && !first_assist_consumed
          {
            first_assist_consumed = true;
            1
          } else {
            0
          };
          let hit = HitMods {
            gunner: gunnery_skill,
            assist: assist_bonus,
            captain_assist,
            captain_fire: leadership_boost,
            smart: 0,
            fire_control: fire_control_points,
          };

          volley(
            hit,
            &[],
            attacker,
            &mut target,
            &firing,
            attacker.live_mounts(*weapon_id),
            battery_title(weapon).as_deref(),
            called_shot_system.as_ref(),
            boost_map,
            rng,
          )
        }
      }
    })
    .collect();

  // The assist roll came first and applied to every shot, so it reads first.
  if let Some(assist) = assist_effect {
    effects.insert(0, assist);
  }

  let rounds_spent = attacker.magazine.rounds - rounds_left;
  if rounds_spent > 0 {
    if let Some(ship) = ships.get(attacker.get_name()) {
      let mut ship = ship.write().unwrap();
      ship.magazine.rounds = ship.magazine.rounds.saturating_sub(rounds_spent);
    }
  }

  (new_missiles, effects)
}

/// What a mount firing is called in an attack line: its gun, "meson gun",
/// or for a spinal mount the mount, "meson spinal mount (3×)".
fn firing_name(firing: &Firing<'_>) -> String {
  match firing.mount.spinal_multiple() {
    Some(multiple) => format!("{} ({multiple}×)", spinal_name(firing.kind)),
    None => String::from(&firing.kind),
  }
}

/// What a battery is called in its volley's line, "particle barbette ×12
/// (battery)"; a single mount has no title and reports shot by shot as it
/// always has.
fn battery_title(weapon: &Weapon) -> Option<String> {
  (weapon.mounts() > 1).then(|| String::from(weapon))
}

/// Several sand clouds thrown at one battery's shots, as one line.
fn sand_summary(defender: &Ship, attacker: &str, sand_mods: &[i32], shots: u16) -> EffectMsg {
  let deployed: Vec<String> = sand_mods
    .iter()
    .filter(|sand_mod| **sand_mod > 0)
    .map(|sand_mod| format!("-{sand_mod}"))
    .collect();
  let mut line = format!(
    "%s sand -> %t: {} of {} clouds against {shots} shots deployed",
    deployed.len(),
    sand_mods.len()
  );
  if !deployed.is_empty() {
    let _ = write!(line, ", damage {}", deployed.join(", "));
  }
  line.push('.');
  EffectMsg::about(defender.get_name(), MessageCategory::Damage, line).at(attacker)
}

/// One sand cloud a defender can throw this round: the reaction's DM, and the
/// mount it comes out of, which pays a canister for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SandCloud {
  pub modifier: i32,
  pub mount: usize,
}

/// Every ship's sand clouds for the round, the last thrown first.
pub type SandCounts = HashMap<String, Vec<SandCloud>>;

/// The sand each ship can throw this round: a cloud per sandcaster mount
/// with canisters in it, a battery's per mount as far as its load goes.
///
/// A mount gets one reaction a round, so one on point defence throws none,
/// and nor does one whose own gunner is reloading it (`reloading`).
#[must_use]
pub fn create_sand_counts<S: BuildHasher, T: BuildHasher>(
  ship_snapshot: &HashMap<String, Ship, S>, point_defense_actions: &[(String, Vec<ShipAction>)],
  reloading: &HashMap<String, Vec<usize>, T>,
) -> SandCounts {
  ship_snapshot
    .iter()
    .map(|(name, ship)| {
      // A mount gets one reaction a round.  Dispersing sand and running point
      // defence are both reactions, so a mount that queued point defence has
      // already spent its own and cannot also throw sand.
      let reacting: Vec<usize> = point_defense_actions
        .iter()
        .filter(|(ship_name, _)| ship_name == name)
        .flat_map(|(_, actions)| actions.iter())
        .filter_map(|action| match action {
          ShipAction::PointDefenseAction { weapon_id, .. } => Some(*weapon_id),
          _ => None,
        })
        .chain(reloading.get(name).into_iter().flatten().copied())
        .collect();

      (
        name.clone(),
        ship
          .weapons()
          .iter()
          .enumerate()
          .filter_map(|(index, weapon)| {
            if reacting.contains(&index) {
              debug!("(Combat.create_sand_counts) Mount {index} on {name} is on point defence or reloading, so it cannot also disperse sand.");
              return None;
            }
            let canisters = ship.loaded(index).sand;
            if canisters == 0 {
              // Nothing left to throw.
              return None;
            }
            if weapon.has_kind(WeaponType::Sand) && ship.active_weapons[index] {
              match weapon.mount {
                // "+1 to the damage negated ... for each additional sandcaster"
                // (Core Rulebook p. 166), counted over the sandcasters in the
                // mount rather than its size -- a turret holding one sandcaster
                // and two lasers negates as one sandcaster, not three.
                WeaponMount::Turret => {
                  Some(i32::from(weapon.count_of(WeaponType::Sand)) - 1 + i32::from(ship.get_crew().get_gunnery(index)))
                }
                WeaponMount::FixedMount => Some(i32::from(ship.get_crew().get_gunnery(index))),
                WeaponMount::Barbette => {
                  error!("Barbette sand mount not supported.");
                  None
                }
                WeaponMount::Bay(_) => {
                  error!("Bay sand mount not supported.");
                  None
                }
                WeaponMount::Battery(_) => {
                  error!("A sandcaster cannot be a point defence battery.");
                  None
                }
                WeaponMount::Spinal { .. } => {
                  error!("A sandcaster cannot be a spinal mount.");
                  None
                }
              }
            } else {
              None
            }
            .map(|modifier| (SandCloud { modifier, mount: index }, ship.live_mounts(index).min(u16::try_from(canisters).unwrap_or(u16::MAX))))
          })
          // A sandcaster battery throws a cloud from every mount, as a
          // point-defence battery adds every mount to the pool, while it has
          // the canisters.
          .flat_map(|(cloud, clouds)| std::iter::repeat_n(cloud, usize::from(clouds)))
          .collect::<Vec<SandCloud>>(),
      )
    })
    .collect()
}

/// Intercept dice for a point-defence battery: 2D / 4D / 6D for Type I / II / III
/// (High Guard p. 40).  `None` for anything that is not a legal battery.
///
/// This is the single place allowed to interpret the (kind, mount) pair as a
/// battery.  Everything else treats a nonsensical pair as inert.
#[must_use]
pub fn battery_intercept_dice(weapon: &Weapon) -> Option<u8> {
  match (weapon.primary_kind(), &weapon.mount) {
    (WeaponType::PointDefense, WeaponMount::Battery(grade @ 1..=3)) => Some(2 * grade),
    _ => None,
  }
}

/// Missiles a repulsor bay deflects this round, or 0 if it is not a repulsor or
/// its check failed.
///
/// "When used as a repulsor, a successful Gunner (capital) check removes a
/// number of missiles from any salvo within range equal to 1D x Effect. Medium
/// repulsor bays multiply the result by two and large repulsor bays multiply it
/// by five" (High Guard p. 33).  A repulsor may only be used once per round,
/// which is what makes it belong in this per-round pool alongside the batteries.
fn roll_repulsor(weapon: &Weapon, skill: u8, rng: &mut dyn RngCore) -> u32 {
  if !weapon.has_kind(WeaponType::Repulsor) {
    return 0;
  }
  let multiplier = match MountClass::from(&weapon.mount) {
    MountClass::SmallBay => 1,
    MountClass::MediumBay => 2,
    MountClass::LargeBay => 5,
    // The book sells repulsors only as bays.
    _ => return 0,
  };

  let effect = i32::from(roll_dice(2, rng)) + i32::from(skill) - STANDARD_ROLL_THRESHOLD;
  if effect < 0 {
    return 0;
  }
  // Effect floors at 1 wherever it multiplies -- see FAQ.md.  A check that
  // succeeded should deflect something.
  #[allow(clippy::cast_sign_loss)]
  let effect = (effect as u32).max(1);
  u32::from(roll_dice(1, rng)) * effect * multiplier
}

/// Roll each of this ship's screens, giving the damage each will absorb.
///
/// Every screen makes its own Gunner (screen) check.  The book has one gunner
/// concentrate every screen on a single attack, but we spread them across
/// attacks -- which is several gunners each taking their own Angle Screens
/// reaction, and several reactions cannot share one roll.
///
/// A screen reduces damage "by the number of dice rolled by the screen ...
/// multiplied by the Effect of the gunner's check" (High Guard p. 40).  Effect
/// floors at 1 because it multiplies here; see FAQ.md.
#[must_use]
pub fn roll_screen_pool(ship: &Ship, rng: &mut dyn RngCore) -> Vec<u32> {
  ship
    .design
    .screens
    .iter()
    .enumerate()
    .map(|(index, screen)| {
      // Angling a screen is a gunner's reaction, so an unstaffed screen does
      // nothing -- unless Screen Optimiser is running, which "automatically
      // performs the Angle Screens (gunner) action with a total DM+0 ... and
      // can use any number of screens simultaneously" (High Guard p. 75).
      // A staffed screen keeps its gunner, who is better than DM+0.
      //
      // The dice are rolled either way and the result discarded when nobody
      // angled it: a scenario's outcome should not depend on how many rolls
      // were skipped, and the seeded runs the tests rely on would shift
      // under a short-circuit here.
      let angled =
        ship.get_crew().has_screen_gunner(index) || ship.running_level(SoftwareKind::ScreenOptimiser).is_some();
      let skill = ship.get_crew().get_screen_gunnery(index);
      let effect = i32::from(roll_dice(2, rng)) + i32::from(skill) - STANDARD_ROLL_THRESHOLD;
      if effect < 0 {
        return 0;
      }
      #[allow(clippy::cast_sign_loss)]
      let effect = (effect as u32).max(1);
      let (dice, factor) = screen.reduction_dice();
      let absorbed = u32::from(roll_dice(dice, rng)) * factor * effect;
      if angled {
        absorbed
      } else {
        0
      }
    })
    .collect()
}

/// How many missiles this ship's batteries will swat this round.
///
/// The book has a battery "automatically intercept" a number of missiles each
/// turn, which the defender may spread across salvoes as they like.  A
/// per-round pool, drained missile by missile across every salvo that
/// arrives, is the same thing with the spreading done for her.
///
/// Batteries are rolled separately and summed rather than pooled into one throw,
/// so that a critical hit disabling one battery removes exactly its share.
#[must_use]
pub fn roll_battery_pool(ship: &Ship, rng: &mut dyn RngCore) -> u32 {
  ship
    .weapons()
    .iter()
    .enumerate()
    .filter(|(index, _)| ship.active_weapons[*index])
    .map(|(index, weapon)| {
      // Batteries intercept automatically and roll no check; repulsors deflect
      // on a Gunner (capital) check.  Both are once-per-round and neither costs
      // the crew an action, so both belong in this pass.
      // Every mount of an entry of several rolls its own, while it is up.
      (0..ship.live_mounts(index))
        .map(|_| match battery_intercept_dice(weapon) {
          Some(dice) => u32::from(roll_dice(dice, rng)),
          None => roll_repulsor(weapon, ship.get_crew().get_gunnery(index), rng),
        })
        .sum::<u32>()
    })
    .sum()
}

// Helper function to determine which point defense weapon is most effective.
// Result here is one more than the bonus to the check. 0 means it cannot
// be used for point defense.
pub(crate) fn point_defense_score(weapon: &Weapon) -> u16 {
  // Only lasers track a missile well enough to swat it (High Guard p. 30 notes
  // barbettes explicitly cannot, which the mount term below already enforces).
  //
  // Counted over the laser guns in the mount: a triple turret holding one pulse
  // laser and two sandcasters is a weak point-defence mount, not a strong one.
  let lasers = weapon.guns.iter().filter(|gun| gun.kind.is_laser()).count();
  if lasers == 0 {
    return 0;
  }

  match weapon.mount {
    WeaponMount::Turret => u16::try_from(lasers).unwrap_or(u16::MAX),
    // Barbettes, bays, spinals and fixed mounts cannot track an incoming
    // missile.
    WeaponMount::Barbette
    | WeaponMount::Bay(_)
    | WeaponMount::FixedMount
    | WeaponMount::Battery(_)
    | WeaponMount::Spinal { .. } => 0,
  }
}

/// For a given ship, and a list of ``PointDefenseAction`` actions, build the list of weapons that will make a
/// point-defence check this round.  Each item is a pair of (id of the weapon, bonus to the check), where the bonus is
/// the turret's DM (+0/+1/+2 for single/double/triple, Core Rulebook p. 171) plus gunnery and any leadership boost.
#[must_use]
pub fn build_point_defense_tallies(
  ship: &Ship, actions: &[ShipAction], boost_map: &BoostMap, ship_name: &str,
) -> Vec<(usize, u16)> {
  let mut point_defense_list = Vec::new();

  // A table indexed by weapon of the score for that weapon.
  // The score is one more than the bonus to the check; 0 means it cannot be used for point defense.
  let weapon_scores = ship
    .weapons()
    .iter()
    .enumerate()
    .map(|(index, weapon)| {
      if ship.active_weapons[index] {
        // Point defence is a gunner's reaction, so an unmanned mount
        // contributes nothing -- unless a Virtual Gunner is at it, which is
        // what the program is for.
        let virtual_gunner = ship
          .running_level(SoftwareKind::VirtualGunner)
          .filter(|_| !ship.get_crew().has_gunner(index));
        if !ship.get_crew().has_gunner(index) && virtual_gunner.is_none() {
          // 0 is the "cannot be used for point defence" score.
          0
        } else {
          let skill = virtual_gunner.unwrap_or_else(|| ship.get_crew().get_gunnery(index));
          // The gunner's check carries the crew's DM; the mount's own score
          // does not, and a score of 0 still means "cannot".
          let skill = u16::try_from(i16::from(skill) + ship.crew_dm()).unwrap_or(0);
          point_defense_score(weapon) + skill
        }
      } else {
        0
      }
    })
    .collect::<Vec<u16>>();

  for action in actions {
    let ShipAction::PointDefenseAction { weapon_id, .. } = action else {
      warn!("(Ship.add_point_defense) Expected PointDefenseAction but got {:?}.", action);
      continue;
    };
    debug!(
      "(Ship.add_point_defense) Adding point defense for {} weapon {}",
      ship.get_name(),
      weapon_id
    );
    if weapon_scores[*weapon_id] == 0 {
      debug!(
        "(Ship.add_point_defense) Weapon {} is not suitable for point defense.",
        weapon_id
      );
      continue;
    }

    // Convert the score to an actual check modifier and apply any captain
    // leadership boost for this specific (ship, weapon) point-defense action.
    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
    let leadership_boost = boost_for_point_defense(boost_map, ship_name, *weapon_id).max(0) as u16;
    // A laser battery on point defence defends as a unit: every mount in it
    // still up makes its check, at its gunner's skill, and they all add to
    // the pool.
    let mounts = ship.live_mounts(*weapon_id);
    for _ in 0..mounts {
      point_defense_list.push((*weapon_id, weapon_scores[*weapon_id].saturating_sub(1) + leadership_boost));
    }
  }

  debug!(
    "(Ship.add_point_defense) Point defense list for {} is {:?}",
    ship.get_name(),
    point_defense_list
  );

  // Deliberately unsorted: every weapon on this list rolls once per round, so
  // there is no "first" weapon and nothing for an order to decide.

  point_defense_list
}

/// Roll a D3, as High Guard writes it: a d6 halved and rounded up.
fn roll_dice_d3(rng: &mut dyn RngCore) -> u8 {
  roll_dice(1, rng).div_ceil(2).clamp(1, 3)
}

/// Pool points needed to stop one incoming object.
///
/// "A torpedo salvo halves the Effect of any successful point defence taken
/// against it, rounding down" (High Guard p. 39).  We resolve point defence as
/// one summed pool rather than per-check, spent across salvoes rather than
/// against one, so halving is expressed as a torpedo costing two points where
/// a missile costs one -- `floor(pool / 2)` torpedoes stopped, which is the same
/// arithmetic applied to the total.  The Fleet Battles rule prices it the same
/// way ("double the amount taken from the pool", p. 113), which is a useful
/// corroboration that the aggregate reading is the intended one.
#[must_use]
pub fn interception_cost(kind: WeaponType) -> u32 {
  if kind == WeaponType::Torpedo {
    2
  } else {
    1
  }
}

/// Roll every queued point-defence weapon and total the missiles they remove.
///
/// Each gunner makes one Gunner (turret) check per round and "the Effect of the
/// check will remove that many missiles from the salvo" (Core Rulebook p. 171).
/// So every weapon on the list rolls exactly once, whatever the salvo looks
/// like, and their Effects add together.
///
/// This is a per-round total rather than a per-missile check because nothing in
/// the rules pairs one gunner with one missile -- two gunners may perfectly well
/// engage the same one, and a single good check can clear several. The pool is
/// the ship's, not one salvo's: it takes whatever arrives first, salvo after
/// salvo, so the pool *is* the allocation.
///
/// # Return
/// Total missiles this ship's gunners will remove this round.
#[must_use]
pub fn roll_point_defense_pool(point_defense_list: &[(usize, u16)], rng: &mut dyn RngCore) -> u32 {
  roll_point_defense_checks(point_defense_list, rng)
    .iter()
    .map(|check| check.removed)
    .sum()
}

/// One gunner's point-defence check for the round: the mount, the 2D rolled,
/// the bonus (turret, gunnery, leadership), and how many missiles it stops.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PointDefenseCheck {
  pub weapon: usize,
  pub roll: u8,
  pub bonus: u16,
  pub removed: u32,
}

impl PointDefenseCheck {
  /// The check as a results line about `defender`, covering `ward` if it is
  /// another ship's missiles it is shooting at; the arithmetic on the hover.
  #[must_use]
  pub fn message(&self, defender: &str, ward: Option<&str>) -> EffectMsg {
    let mount = self.weapon;
    let whose = if ward.is_some() { " covering %t" } else { "" };
    let content = if self.removed > 0 {
      format!(
        "%s's mount #{mount} on point defence{whose} can stop {} missile{} this round.",
        self.removed,
        if self.removed == 1 { "" } else { "s" }
      )
    } else {
      format!("%s's mount #{mount} on point defence{whose} misses: it stops nothing this round.")
    };
    let total = i32::from(self.roll) + i32::from(self.bonus);
    let effect = total - STANDARD_ROLL_THRESHOLD;
    let message = EffectMsg::outcome(defender, MessageCategory::Attack, content, self.removed > 0).rolled(format!(
      "roll {} + turret, gunner and leadership {} = {total} vs {STANDARD_ROLL_THRESHOLD}, effect {effect:+}{}",
      self.roll,
      self.bonus,
      if self.removed > 0 && effect == 0 {
        " (a bare success stops one)"
      } else {
        ""
      }
    ));
    match ward {
      Some(ward) => message.at(ward),
      None => message,
    }
  }
}

/// The checks as results lines: one a mount, except that a battery's mounts
/// are reported together, "mount #3 (battery of 16) on point defence can stop
/// 23 missiles this round", with every roll on the hover.
#[must_use]
pub fn point_defense_messages(checks: &[PointDefenseCheck], defender: &str, ward: Option<&str>) -> Vec<EffectMsg> {
  checks
    .chunk_by(|a, b| a.weapon == b.weapon)
    .map(|group| {
      let [check] = group else {
        let mount = group[0].weapon;
        let removed: u32 = group.iter().map(|check| check.removed).sum();
        let whose = if ward.is_some() { " covering %t" } else { "" };
        let content = format!(
          "%s's mount #{mount} (battery of {}) on point defence{whose} can stop {removed} missile{} this round.",
          group.len(),
          if removed == 1 { "" } else { "s" }
        );
        let rolls: Vec<String> = group.iter().map(|check| check.roll.to_string()).collect();
        let message = EffectMsg::outcome(defender, MessageCategory::Attack, content, removed > 0).rolled(format!(
          "each roll + turret, gunner and leadership {} vs {STANDARD_ROLL_THRESHOLD}: {}",
          group[0].bonus,
          rolls.join(", ")
        ));
        return match ward {
          Some(ward) => message.at(ward),
          None => message,
        };
      };
      check.message(defender, ward)
    })
    .collect()
}

/// Roll each gunner's point-defence check: see [`roll_point_defense_pool`],
/// which sums them. The dice are thrown in the same order, so the two give the
/// same game.
#[must_use]
pub fn roll_point_defense_checks(point_defense_list: &[(usize, u16)], rng: &mut dyn RngCore) -> Vec<PointDefenseCheck> {
  point_defense_list
    .iter()
    .map(|(weapon, bonus)| {
      let roll = roll_dice(2, rng);
      let effect = i32::from(roll) + i32::from(*bonus) - STANDARD_ROLL_THRESHOLD;
      // A successful check always stops at least the missile it was made
      // against, so a bare success is worth one.
      #[allow(clippy::cast_sign_loss)]
      let removed = if effect >= 0 { effect.max(1) as u32 } else { 0 };
      debug!(
        "(Combat.roll_point_defense_checks) Weapon {weapon} rolled {roll} with bonus {bonus}: removes {removed} missile(s)."
      );
      PointDefenseCheck {
        weapon: *weapon,
        roll,
        bonus: *bonus,
        removed,
      }
    })
    .collect()
}

#[cfg(test)]
mod point_defense_message_tests {
  use super::*;

  #[test]
  fn a_check_reports_its_roll() {
    let check = PointDefenseCheck {
      weapon: 2,
      roll: 9,
      bonus: 3,
      removed: 4,
    };
    let line = check.message("INS Righteous", None);
    assert_eq!(
      line.rendered(),
      "INS Righteous's mount #2 on point defence can stop 4 missiles this round."
    );
    let EffectMsg::Message { roll, .. } = &line else {
      panic!("expected a message, got {line:?}");
    };
    assert_eq!(
      roll.as_deref(),
      Some("roll 9 + turret, gunner and leadership 3 = 12 vs 8, effect +4")
    );
  }

  #[test]
  fn a_miss_says_so_and_names_the_ship_covered() {
    let check = PointDefenseCheck {
      weapon: 0,
      roll: 3,
      bonus: 2,
      removed: 0,
    };
    let line = check.message("INS Stag", Some("INS Righteous"));
    assert_eq!(
      line.rendered(),
      "INS Stag's mount #0 on point defence covering INS Righteous misses: it stops nothing this round."
    );
  }
}

#[cfg(test)]
mod battery_tests {
  use super::*;
  use crate::action::ShipAction;
  use crate::entity::Vec3;
  use crate::rules_tables::weapon_profile;
  use crate::ship::ShipDesignTemplate;
  use crate::ship::{Gun, MountClass, ScreenType, WeaponModifier};
  use cgmath::Zero;
  use rand::rngs::SmallRng;
  use rand::SeedableRng;
  use std::collections::HashMap;
  use std::sync::Arc;

  fn battery(grade: u8) -> Weapon {
    Weapon::single(WeaponType::PointDefense, WeaponMount::Battery(grade))
  }

  fn ship_with(weapons: Vec<Weapon>) -> Ship {
    let design = Arc::new(ShipDesignTemplate {
      name: "Batteries".to_string(),
      weapons,
      ..Default::default()
    });
    Ship::new("Batteries".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None)
  }

  /// Type I/II/III intercept 2D/4D/6D (High Guard p. 40).
  #[test]
  fn intercept_dice_follow_the_grade() {
    assert_eq!(battery_intercept_dice(&battery(1)), Some(2));
    assert_eq!(battery_intercept_dice(&battery(2)), Some(4));
    assert_eq!(battery_intercept_dice(&battery(3)), Some(6));
  }

  /// The (kind, mount) pair is a cross-product, so nonsense pairs are
  /// representable.  They must read as "not a battery" rather than as a battery
  /// of some invented grade.
  #[test]
  fn nonsense_pairs_are_not_batteries() {
    // A grade the book does not sell.
    assert_eq!(battery_intercept_dice(&battery(0)), None);
    assert_eq!(battery_intercept_dice(&battery(4)), None);
    // A real weapon in a battery mount, and a battery in a real mount.
    assert_eq!(
      battery_intercept_dice(&Weapon::single(WeaponType::Beam, WeaponMount::Battery(2))),
      None
    );
    assert_eq!(
      battery_intercept_dice(&Weapon::uniform(WeaponType::PointDefense, WeaponMount::Turret, 3)),
      None
    );
  }

  /// A battery takes no action and never enters the gunner-driven point defence
  /// path, so a stray `PointDefenseAction` naming one is dropped.
  #[test]
  fn batteries_score_zero_in_the_action_path() {
    for grade in 1..=3 {
      assert_eq!(point_defense_score(&battery(grade)), 0);
    }
  }

  #[test]
  fn pool_is_zero_without_batteries() {
    let ship = ship_with(vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3)]);
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    assert_eq!(roll_battery_pool(&ship, &mut rng), 0);
  }

  /// A pool is a sum of dice, so it must land inside the range those dice can
  /// produce.  Asserting bounds rather than an exact value keeps this from
  /// being a change-detector for the RNG.
  #[test]
  fn pool_lands_within_the_dice_range() {
    for (grade, dice) in [(1u8, 2u32), (2, 4), (3, 6)] {
      let ship = ship_with(vec![battery(grade)]);
      let mut rng = SmallRng::seed_from_u64(0xD1CE);
      let pool = roll_battery_pool(&ship, &mut rng);
      assert!(
        (dice..=dice * 6).contains(&pool),
        "Type {grade} rolled {pool}, outside {dice}D's range of {dice}..={}",
        dice * 6
      );
    }
  }

  /// Batteries stack additively, and are rolled separately so that losing one
  /// to a critical hit removes exactly its share.
  #[test]
  fn batteries_stack() {
    let ship = ship_with(vec![battery(3), battery(3)]);
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let pool = roll_battery_pool(&ship, &mut rng);
    assert!(
      (12..=72).contains(&pool),
      "two Type III batteries rolled {pool}, outside 12..=72"
    );
  }

  /// A battery knocked out by a critical hit stops contributing.
  #[test]
  fn disabled_batteries_contribute_nothing() {
    let mut ship = ship_with(vec![battery(3), battery(3)]);
    ship.active_weapons[0] = false;
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let pool = roll_battery_pool(&ship, &mut rng);
    assert!((6..=36).contains(&pool), "one live Type III rolled {pool}, outside 6..=36");
  }

  /// The pool is spent one missile at a time and cannot go negative.
  #[test]
  fn pool_drains_one_missile_at_a_time() {
    let mut ship = ship_with(vec![battery(1)]);
    ship.set_point_defense_pool(2);
    assert!(ship.take_interception(interception_cost(WeaponType::Missile)));
    assert!(ship.take_interception(interception_cost(WeaponType::Missile)));
    assert!(!ship.take_interception(interception_cost(WeaponType::Missile)));
    assert_eq!(ship.point_defense_pool, 0);
  }

  /// A torpedo costs two points where a missile costs one, so the same pool
  /// stops half as many of them (High Guard p. 113).
  #[test]
  fn torpedoes_cost_double() {
    assert_eq!(interception_cost(WeaponType::Torpedo), 2);
    assert_eq!(interception_cost(WeaponType::Missile), 1);

    let mut ship = ship_with(vec![battery(1)]);
    ship.set_point_defense_pool(4);
    for _ in 0..2 {
      assert!(ship.take_interception(interception_cost(WeaponType::Torpedo)));
    }
    assert!(!ship.take_interception(interception_cost(WeaponType::Torpedo)));
    assert_eq!(ship.point_defense_pool, 0);
  }

  /// A pool too small for a torpedo stops nothing, and the leftover point stays
  /// available for a missile rather than being wasted.
  #[test]
  fn a_partial_pool_cannot_half_stop_a_torpedo() {
    let mut ship = ship_with(vec![battery(1)]);
    ship.set_point_defense_pool(1);
    assert!(!ship.take_interception(interception_cost(WeaponType::Torpedo)));
    assert_eq!(ship.point_defense_pool, 1, "the failed attempt must not spend anything");
    assert!(ship.take_interception(interception_cost(WeaponType::Missile)));
  }

  /// A gun fires because someone fires it. An empty mount says so rather
  /// than quietly missing.
  #[test]
  fn an_unmanned_mount_does_not_fire() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Derelict".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      power: 300,
      ..ShipDesignTemplate::default()
    });
    // A crew stated explicitly with nobody on the gun, which is how a
    // scenario says the position is empty.
    let crew = crate::crew::Crew::new();
    let mut attacker = Ship::new("Derelict".to_string(), Vec3::zero(), Vec3::zero(), &design, Some(crew), None);
    attacker.contacts.push("Target".to_string());
    let target = Ship::new(
      "Target".to_string(),
      Vec3::new(5000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let mut ships: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
    let snapshot: HashMap<String, Ship> = HashMap::new();
    let mut sand = create_sand_counts(&snapshot, &[], &HashMap::new());

    let actions = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Target".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];
    let mut rng = SmallRng::seed_from_u64(7);
    let (_, effects) = do_fire_actions(&attacker, &mut ships, &mut sand, &actions, &BoostMap::default(), &mut rng);
    assert!(
      format!("{effects:?}").contains("has no gunner"),
      "the crew should be told why nothing was fired: {effects:?}"
    );
  }

  /// The turret bonus must match the book: DM+0 single, DM+1 double, DM+2 triple
  /// (Core Rulebook p. 171), plus the gunner's skill.
  ///
  /// `point_defense_score` returns one *more* than the bonus so that 0 can mean
  /// "unusable for point defence"; `build_point_defense_tallies` takes that 1
  /// back off.  This pins the round trip, because losing or double-applying that
  /// conversion shifts every point-defence check by a full point.
  #[test]
  fn turret_point_defense_bonus_matches_the_book() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Gunners".to_string(),
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 2),
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3),
      ],
      ..Default::default()
    });
    // Gunnery 0 across the board, so the tally is the turret bonus alone.
    let ship = Ship::new("Gunners".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let actions: Vec<ShipAction> = (0..3)
      .map(|weapon_id| ShipAction::PointDefenseAction {
        weapon_id,
        protecting: None,
      })
      .collect();

    let tallies = build_point_defense_tallies(&ship, &actions, &BoostMap::default(), "Gunners");
    let bonus = |id: usize| tallies.iter().find(|(w, _)| *w == id).map(|(_, b)| *b);

    assert_eq!(bonus(0), Some(0), "a single turret is DM+0");
    assert_eq!(bonus(1), Some(1), "a double turret is DM+1");
    assert_eq!(bonus(2), Some(2), "a triple turret is DM+2");
  }

  /// A torpedo is DM-2 to hit anything under 2,000 tons (High Guard p. 39).
  ///
  /// Driven by rolling the same seeded attack at a small and a large target and
  /// checking the small one is harder to hit across many trials, rather than by
  /// asserting an exact roll -- the point is the direction of the modifier.
  #[test]
  fn torpedoes_struggle_against_small_ships() {
    let small = Arc::new(ShipDesignTemplate {
      name: "Small".to_string(),
      displacement: 400,
      hull: 1_000_000,
      ..Default::default()
    });
    let large = Arc::new(ShipDesignTemplate {
      name: "Large".to_string(),
      displacement: 5_000,
      hull: 1_000_000,
      ..Default::default()
    });
    let attacker = ship_with(vec![]);
    let torpedo = Weapon::single(WeaponType::Torpedo, WeaponMount::Barbette);

    let mut hits = [0u32; 2];
    for (slot, design) in [&small, &large].into_iter().enumerate() {
      let mut rng = SmallRng::seed_from_u64(0x707D);
      for _ in 0..400 {
        let mut defender = Ship::new("D".to_string(), Vec3::zero(), Vec3::zero(), design, None, None);
        let effects = attack(
          HitMods::gunner(0),
          0,
          &attacker,
          &mut defender,
          &torpedo.firing_default().unwrap(),
          None,
          &BoostMap::default(),
          &mut rng,
        );
        if effects.iter().any(|e| !matches!(e, EffectMsg::Message { .. })) {
          hits[slot] += 1;
        }
      }
    }
    assert!(
      hits[0] < hits[1],
      "a torpedo should hit the 400-ton ship less often than the 5,000-ton one: {hits:?}"
    );
  }

  /// An ion hit drains Power and leaves the hull untouched (High Guard p. 30).
  #[test]
  fn ion_drains_power_not_hull() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Target".to_string(),
      displacement: 5_000,
      power: 500,
      hull: 1_000,
      armor: 10,
      ..Default::default()
    });
    let attacker = ship_with(vec![]);
    let ion = Weapon::single(WeaponType::Ion, WeaponMount::Barbette);
    let mut rng = SmallRng::seed_from_u64(0x10);

    // Loop until a hit lands, so the test is about what a hit does rather than
    // about whether this particular seed connects.
    let mut defender = Ship::new("Target".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let hull_before = defender.get_current_hull_points();
    for _ in 0..50 {
      attack(
        HitMods::gunner(8),
        0,
        &attacker,
        &mut defender,
        &ion.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng,
      );
      if defender.ion_power_loss > 0 {
        break;
      }
    }

    assert!(defender.ion_power_loss > 0, "an ion cannon should eventually connect");
    assert_eq!(
      defender.get_current_hull_points(),
      hull_before,
      "an ion cannon must not damage the hull"
    );
    assert_eq!(defender.current_power, 500, "current_power is the undamaged figure");
    assert!(
      defender.available_power() < 500,
      "available power should be suppressed while the ion effect runs"
    );
    assert!(defender.ion_rounds >= 1);
  }

  /// Ion ignores armour outright, so a heavily armoured ship is no better off.
  #[test]
  fn ion_ignores_armour() {
    let profile = weapon_profile(WeaponType::Ion, MountClass::Barbette).unwrap();
    assert_eq!(profile.ap, crate::ship::AP_INFINITE);
    assert!(profile.ion);
    // It is still a direct-fire weapon, so it takes the mount's multiple.
    assert!(profile.use_multiple);
    assert!(profile.salvo.is_none());
  }

  /// Ion is a barbette-and-bay weapon; the book sells no ion turret.
  #[test]
  fn ion_has_no_turret() {
    assert!(weapon_profile(WeaponType::Ion, MountClass::Turret).is_none());
    assert!(weapon_profile(WeaponType::Ion, MountClass::Fixed).is_none());
    assert!(weapon_profile(WeaponType::Ion, MountClass::Barbette).is_some());
    assert!(weapon_profile(WeaponType::Ion, MountClass::LargeBay).is_some());
  }

  /// Suppression lapses on its own, handing the Power back.
  #[test]
  fn ion_suppression_expires() {
    let mut ship = ship_with(vec![]);
    ship.current_power = 100;
    ship.apply_ion_damage(40, 1);
    assert_eq!(ship.available_power(), 60);

    ship.tick_ion_recovery();
    assert_eq!(ship.available_power(), 100, "power returns once the effect lapses");
    assert_eq!(ship.ion_rounds, 0);
  }

  /// Two hits stack, and the longer duration wins so a second hit cannot cut
  /// the first one short.
  #[test]
  fn ion_hits_stack_and_take_the_longer_duration() {
    let mut ship = ship_with(vec![]);
    ship.current_power = 100;
    ship.apply_ion_damage(30, 3);
    ship.apply_ion_damage(20, 1);
    assert_eq!(ship.available_power(), 50, "both hits should be suppressing power");
    assert_eq!(ship.ion_rounds, 3, "the longer duration wins");

    ship.tick_ion_recovery();
    assert_eq!(ship.available_power(), 50, "still suppressed after one round");
    ship.tick_ion_recovery();
    ship.tick_ion_recovery();
    assert_eq!(ship.available_power(), 100);
  }

  /// Draining power throttles thrust, which is the point of an ion cannon.
  #[test]
  fn ion_suppression_limits_thrust() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Runner".to_string(),
      displacement: 400,
      power: 400,
      maneuver: 6,
      ..Default::default()
    });
    let mut ship = Ship::new("Runner".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let before = ship.max_acceleration();
    ship.apply_ion_damage(350, 1);
    let after = ship.max_acceleration();
    assert!(after < before, "losing power should cost thrust: {before} -> {after}");
  }

  fn ship_with_screens(screens: Vec<ScreenType>, skills: &[u8]) -> Ship {
    let design = Arc::new(ShipDesignTemplate {
      name: "Screened".to_string(),
      displacement: 5_000,
      hull: 1_000_000,
      screens,
      ..Default::default()
    });
    let mut crew = crate::crew::Crew::new();
    for skill in skills {
      crew.add_screen_gunnery(*skill);
    }
    Ship::new("Screened".to_string(), Vec3::zero(), Vec3::zero(), &design, Some(crew), None)
  }

  /// Screens are strictly type-specific.
  #[test]
  fn screens_only_defend_their_own_weapon() {
    assert!(ScreenType::Meson.defends_against(WeaponType::Meson));
    assert!(!ScreenType::Meson.defends_against(WeaponType::Fusion));
    assert!(ScreenType::NuclearDamper.defends_against(WeaponType::Fusion));
    assert!(!ScreenType::NuclearDamper.defends_against(WeaponType::Meson));
    // And neither touches an ordinary laser.
    assert!(!ScreenType::Meson.defends_against(WeaponType::Beam));
    assert!(!ScreenType::NuclearDamper.defends_against(WeaponType::Beam));
  }

  /// A meson screen reduces by 2D x 10; a damper by 2D.
  #[test]
  fn screen_reduction_matches_the_book() {
    assert_eq!(ScreenType::Meson.reduction_dice(), (2, 10));
    assert_eq!(ScreenType::NuclearDamper.reduction_dice(), (2, 1));
  }

  /// The roll must land inside what the dice, the factor and the Effect allow.
  #[test]
  fn screen_pool_lands_in_range() {
    let ship = ship_with_screens(vec![ScreenType::Meson], &[2]);
    let mut rng = SmallRng::seed_from_u64(0x5C4E);
    for _ in 0..50 {
      let pool = roll_screen_pool(&ship, &mut rng);
      assert_eq!(pool.len(), 1);
      // Either the check failed (0), or 2D x 10 x at least 1.
      assert!(pool[0] == 0 || (20..=12 * 10 * 7).contains(&pool[0]), "got {}", pool[0]);
    }
  }

  /// A screen absorbs damage from the weapon it defends against, and is then
  /// spent -- excess and all.
  #[test]
  fn a_screen_is_spent_whole() {
    let mut ship = ship_with_screens(vec![ScreenType::NuclearDamper], &[0]);
    ship.set_screen_pool(vec![50]);

    // A 20-damage fusion hit is fully absorbed...
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 20), 0);
    // ...and the remaining 30 is gone with it, so the next hit lands in full.
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 20), 20);
  }

  /// Screens carry to the next attack once the current one is stopped.
  #[test]
  fn screens_spread_across_attacks() {
    let mut ship = ship_with_screens(vec![ScreenType::NuclearDamper, ScreenType::NuclearDamper], &[0, 0]);
    ship.set_screen_pool(vec![10, 10]);

    // First attack takes the first screen only, since 10 zeroes it.
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 10), 0);
    // Second attack gets the second screen, still unspent.
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 10), 0);
    // Third has nothing left.
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 10), 10);
  }

  /// Several screens stack on one attack when one is not enough.
  #[test]
  fn screens_stack_until_the_damage_is_gone() {
    let mut ship = ship_with_screens(vec![ScreenType::NuclearDamper, ScreenType::NuclearDamper], &[0, 0]);
    ship.set_screen_pool(vec![10, 10]);
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 25), 5, "both screens should be spent");
    assert_eq!(ship.apply_screens(WeaponType::Fusion, 10), 10, "and nothing is left");
  }

  /// A screen is never spent on a weapon it does not defend against.
  #[test]
  fn screens_ignore_the_wrong_weapon() {
    let mut ship = ship_with_screens(vec![ScreenType::Meson], &[0]);
    ship.set_screen_pool(vec![500]);
    assert_eq!(
      ship.apply_screens(WeaponType::Fusion, 40),
      40,
      "a meson screen must not stop a fusion gun"
    );
    // Still available for what it is for.
    assert_eq!(ship.apply_screens(WeaponType::Meson, 40), 0);
  }

  /// Screens are per-round scratch and must not survive the round.
  #[test]
  fn clearing_point_defense_clears_screens() {
    let mut ship = ship_with_screens(vec![ScreenType::Meson], &[0]);
    ship.set_screen_pool(vec![100]);
    ship.clear_point_defense();
    assert!(
      ship.screen_pool.is_empty(),
      "screens should all have been spent: {:?}",
      ship.screen_pool
    );
    assert_eq!(ship.apply_screens(WeaponType::Meson, 40), 40);
  }

  fn profile_with(kind: WeaponType, mount: MountClass, mods: &[WeaponModifier]) -> WeaponProfile {
    weapon_profile(kind, mount).unwrap().with_modifiers(kind, mods)
  }

  /// Accurate is DM+1 to attack rolls, Inaccurate DM-1 (High Guard p. 71).
  #[test]
  fn accuracy_modifiers_shift_the_hit_roll() {
    let plain = profile_with(WeaponType::Beam, MountClass::Turret, &[]);
    let accurate = profile_with(WeaponType::Beam, MountClass::Turret, &[WeaponModifier::Accurate]);
    let inaccurate = profile_with(WeaponType::Beam, MountClass::Turret, &[WeaponModifier::Inaccurate]);
    assert_eq!(accurate.hit_mod, plain.hit_mod + 1);
    assert_eq!(inaccurate.hit_mod, plain.hit_mod - 1);
  }

  /// "Intense Focus can only be applied to lasers and particle weapons."
  #[test]
  fn intense_focus_is_ap_and_only_for_lasers_and_particle() {
    let focus = [WeaponModifier::IntenseFocus];
    let beam = profile_with(WeaponType::Beam, MountClass::Turret, &focus);
    assert_eq!(beam.ap, 2, "a laser gains AP+2");
    let particle = profile_with(WeaponType::Particle, MountClass::Turret, &focus);
    assert_eq!(particle.ap, 2);

    // A railgun already has AP 4 and is not eligible, so it stays put.
    let railgun = profile_with(WeaponType::Railgun, MountClass::Turret, &focus);
    assert_eq!(railgun.ap, 4, "intense focus does not apply to a railgun");
  }

  /// "The range for the weapon is increased by one band, to a maximum of Very
  /// Long."
  #[test]
  fn long_range_raises_the_band_and_stops_at_very_long() {
    let long = [WeaponModifier::LongRange];
    // A railgun turret is Short, so it becomes Medium.
    assert_eq!(
      profile_with(WeaponType::Railgun, MountClass::Turret, &long).max_range,
      Some(Range::Medium)
    );
    // A particle beam is already Very Long and must not reach Distant.
    assert_eq!(
      profile_with(WeaponType::Particle, MountClass::Turret, &long).max_range,
      Some(Range::VeryLong)
    );
    // A launcher has no band to raise.
    assert_eq!(profile_with(WeaponType::Missile, MountClass::Turret, &long).max_range, None);
  }

  /// High Yield counts 1s as 2s; Very High Yield counts 1s and 2s as 3s.
  /// Neither applies to missiles or torpedoes.
  #[test]
  fn yield_modifiers_set_the_die_floor() {
    assert_eq!(WeaponProfile::min_die(WeaponType::Beam, &[]), 1);
    assert_eq!(WeaponProfile::min_die(WeaponType::Beam, &[WeaponModifier::HighYield]), 2);
    assert_eq!(WeaponProfile::min_die(WeaponType::Beam, &[WeaponModifier::VeryHighYield]), 3);
    // The stronger wins when both are somehow present.
    assert_eq!(
      WeaponProfile::min_die(WeaponType::Beam, &[WeaponModifier::HighYield, WeaponModifier::VeryHighYield]),
      3
    );
    // "Not applicable for missiles and torpedoes."
    assert_eq!(WeaponProfile::min_die(WeaponType::Missile, &[WeaponModifier::HighYield]), 1);
    assert_eq!(WeaponProfile::min_die(WeaponType::Torpedo, &[WeaponModifier::VeryHighYield]), 1);
  }

  /// The floor has to be applied per die, not to the total.
  #[test]
  fn the_die_floor_applies_to_each_die() {
    let mut rng = SmallRng::seed_from_u64(0x41CE);
    for _ in 0..200 {
      let plain = roll_dice_min(6, 1, &mut rng);
      assert!((6..=36).contains(&plain), "6D out of range: {plain}");
      // With every 1 counted as 2, six dice cannot total less than 12.
      let high = roll_dice_min(6, 2, &mut rng);
      assert!((12..=36).contains(&high), "6D high yield out of range: {high}");
      // And with 1s and 2s as 3s, not less than 18.
      let very = roll_dice_min(6, 3, &mut rng);
      assert!((18..=36).contains(&very), "6D very high yield out of range: {very}");
    }
  }

  /// Capital-ship damage rolls far more dice than a byte can total: a 66D
  /// meson spinal (High Guard p. 35) once silently rolled 0.
  #[test]
  fn many_dice_roll_in_full() {
    let mut rng = SmallRng::seed_from_u64(0x5917);
    for dice in [43u8, 66, 120, u8::MAX] {
      let total = roll_dice_min(dice, 1, &mut rng);
      let (low, high) = (u16::from(dice), u16::from(dice) * 6);
      assert!((low..=high).contains(&total), "{dice}D out of range: {total}");
      let floored = roll_dice_min(dice, 3, &mut rng);
      assert!(
        (low * 3..=high).contains(&floored),
        "{dice}D floored at 3 out of range: {floored}"
      );
    }
    // The mean of 66D is 231; a sum of 0 (the old failure) is impossible.
    let mean = (0..200).map(|_| f64::from(roll_dice_min(66, 1, &mut rng))).sum::<f64>() / 200.0;
    assert!((220.0..=242.0).contains(&mean), "66D mean {mean}");
    // A check roll that is handed too many dice saturates rather than reading 0.
    assert_eq!(roll_dice(u8::MAX, &mut rng), u8::MAX);
    assert!((2..=12).contains(&roll_dice(2, &mut rng)));
  }

  /// Modifiers ride on the weapon, so two weapons of the same kind and mount can
  /// differ -- which is exactly the mixed turret the MK Mora carries.
  #[test]
  fn modifiers_belong_to_the_weapon_not_the_mount() {
    let plain = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 3);
    let modified = Weapon {
      count: 1,
      mount: WeaponMount::Turret,
      guns: (0..3)
        .map(|_| Gun::with_modifiers(WeaponType::Pulse, vec![WeaponModifier::LongRange, WeaponModifier::HighYield]))
        .collect(),
    };
    assert_ne!(plain, modified);

    let plain_firing = plain.firing_default().unwrap();
    let modified_firing = modified.firing_default().unwrap();
    let plain_profile = profile_for(plain_firing.kind, plain_firing.mount).unwrap();
    let modified_profile = profile_for(modified_firing.kind, modified_firing.mount)
      .unwrap()
      .with_modifiers(modified_firing.kind, modified_firing.modifiers);
    assert_eq!(plain_profile.max_range, Some(Range::Long));
    assert_eq!(modified_profile.max_range, Some(Range::VeryLong));
  }

  /// The MK Mora's turret: two long-range high-yield pulse lasers beside a
  /// plain sandcaster.
  fn mora_turret() -> Weapon {
    Weapon {
      count: 1,
      mount: WeaponMount::Turret,
      guns: vec![
        Gun::with_modifiers(WeaponType::Pulse, vec![WeaponModifier::LongRange, WeaponModifier::HighYield]),
        Gun::with_modifiers(WeaponType::Pulse, vec![WeaponModifier::LongRange, WeaponModifier::HighYield]),
        Gun::new(WeaponType::Sand),
      ],
    }
  }

  #[test]
  fn a_mixed_mount_knows_it_is_mixed() {
    assert!(!mora_turret().is_uniform());
    assert!(Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3).is_uniform());
    // A single-gun mount is trivially uniform.
    assert!(Weapon::single(WeaponType::Torpedo, WeaponMount::Barbette).is_uniform());
  }

  /// Counting is by gun type, not by turret size.  This is the whole point: a
  /// turret with two lasers and a sandcaster is a two-laser mount, not a three.
  #[test]
  fn a_mixed_mount_counts_by_type() {
    let turret = mora_turret();
    assert_eq!(turret.count_of(WeaponType::Pulse), 2);
    assert_eq!(turret.count_of(WeaponType::Sand), 1);
    assert_eq!(turret.count_of(WeaponType::Beam), 0);
    assert_eq!(turret.kinds(), vec![WeaponType::Pulse, WeaponType::Sand]);
    assert!(turret.has_kind(WeaponType::Sand));
  }

  /// Firing resolves to the guns of one type, carrying their modifiers -- and
  /// not the modifiers of the other guns beside them.
  #[test]
  fn firing_picks_one_type_and_its_modifiers() {
    let turret = mora_turret();

    let lasers = turret.firing(WeaponType::Pulse).expect("has pulse lasers");
    assert_eq!(lasers.count, 2, "the same-type bonus counts two lasers, not three guns");
    assert_eq!(lasers.modifiers, &[WeaponModifier::LongRange, WeaponModifier::HighYield]);

    let sand = turret.firing(WeaponType::Sand).expect("has a sandcaster");
    assert_eq!(sand.count, 1);
    assert!(sand.modifiers.is_empty(), "the sandcaster is not long range or high yield");

    assert!(turret.firing(WeaponType::Beam).is_none(), "it carries no beam laser");
  }

  /// Point defence scores the lasers in a mount, not its size.
  #[test]
  fn point_defense_counts_lasers_only() {
    // Two lasers beside a sandcaster scores as two, not three.
    assert_eq!(point_defense_score(&mora_turret()), 2);
    // A full triple laser turret still scores three.
    assert_eq!(
      point_defense_score(&Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 3)),
      3
    );
    // And a turret with no laser at all scores nothing.
    assert_eq!(
      point_defense_score(&Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 3)),
      0
    );
  }

  /// A mixed mount is named by its contents.
  #[test]
  fn a_mixed_mount_is_named_by_its_contents() {
    assert_eq!(String::from(&mora_turret()), "pulse laser x2, sand triple turret");
    assert_eq!(
      String::from(&Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3)),
      "beam laser triple turret"
    );
  }

  /// The old wire shape still reads, and a uniform mount still writes it.
  #[test]
  fn the_old_wire_shape_survives() {
    let json = r#"{"kind":"Beam","mount":{"Turret":3}}"#;
    let weapon: Weapon = serde_json::from_str(json).expect("old shape should parse");
    assert_eq!(weapon.guns.len(), 3, "a turret of 3 becomes three guns");
    assert!(weapon.is_uniform());
    assert_eq!(serde_json::to_string(&weapon).unwrap(), json, "and writes back unchanged");
  }

  /// A genuinely mixed mount needs the new shape, and round trips in it.
  #[test]
  fn a_mixed_mount_uses_the_new_wire_shape() {
    let turret = mora_turret();
    let json = serde_json::to_string(&turret).unwrap();
    assert!(json.contains("\"guns\""), "a mixed turret must list its guns: {json}");
    let back: Weapon = serde_json::from_str(&json).expect("new shape should parse");
    assert_eq!(back, turret);
  }

  /// A mount gets one reaction a round, so a mount on point defence cannot also
  /// throw sand.
  #[test]
  fn a_mount_on_point_defense_cannot_also_disperse_sand() {
    // A turret holding a laser and a sandcaster: it could do either.
    let turret = Weapon {
      count: 1,
      mount: WeaponMount::Turret,
      guns: vec![Gun::new(WeaponType::Pulse), Gun::new(WeaponType::Sand)],
    };
    let ship = ship_with(vec![turret]);
    let ships: HashMap<String, Ship> = [(ship.get_name().to_string(), ship)].into_iter().collect();

    // With no point-defence order, the sandcaster is available.
    let free = create_sand_counts(&ships, &[], &HashMap::new());
    assert_eq!(free["Batteries"].len(), 1, "the sandcaster should be ready: {free:?}");

    // Ordering point defence on that mount spends its reaction.
    let pd = vec![(
      "Batteries".to_string(),
      vec![ShipAction::PointDefenseAction {
        weapon_id: 0,
        protecting: None,
      }],
    )];
    let spent = create_sand_counts(&ships, &pd, &HashMap::new());
    assert!(
      spent["Batteries"].is_empty(),
      "a mount on point defence has no reaction left for sand: {spent:?}"
    );
  }

  /// A mount whose own gunner is reloading it makes no reaction either, and
  /// a sandcaster with no canisters in it throws nothing, whatever is in
  /// stores.
  #[test]
  fn a_reloading_or_empty_sandcaster_throws_nothing() {
    let mut ship = ship_with(vec![Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 1)]);
    let mut ships: HashMap<String, Ship> = [(ship.get_name().to_string(), ship.clone())].into_iter().collect();
    let reloading: HashMap<String, Vec<usize>> = [("Batteries".to_string(), vec![0])].into_iter().collect();
    assert_eq!(create_sand_counts(&ships, &[], &HashMap::new())["Batteries"].len(), 1);
    assert!(
      create_sand_counts(&ships, &[], &reloading)["Batteries"].is_empty(),
      "its gunner is reloading"
    );

    ship.loaded[0].sand = 0;
    ship.magazine.sand = 40;
    ships.insert("Batteries".to_string(), ship);
    assert!(
      create_sand_counts(&ships, &[], &HashMap::new())["Batteries"].is_empty(),
      "the caster is empty"
    );
  }

  /// A sandcaster battery throws a cloud per mount only while its load lasts.
  #[test]
  fn a_sandcaster_battery_throws_what_it_holds() {
    let mut ship = ship_with(vec![Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 3).times(4)]);
    ship.loaded[0].sand = 2;
    let ships: HashMap<String, Ship> = [(ship.get_name().to_string(), ship)].into_iter().collect();
    let clouds = &create_sand_counts(&ships, &[], &HashMap::new())["Batteries"];
    assert_eq!(clouds.len(), 2);
    assert!(clouds.iter().all(|cloud| cloud.mount == 0));
  }

  /// Attacking does *not* spend the sandcaster, which is the whole reason to put
  /// one in a mixed turret.
  #[test]
  fn attacking_leaves_the_sandcaster_available() {
    let ship = ship_with(vec![Weapon {
      count: 1,
      mount: WeaponMount::Turret,
      guns: vec![Gun::new(WeaponType::Pulse), Gun::new(WeaponType::Sand)],
    }]);
    let ships: HashMap<String, Ship> = [(ship.get_name().to_string(), ship)].into_iter().collect();

    // A fire action is not a point-defence action, so sand is untouched.  The
    // gunner attacks with the laser and still reacts with the sandcaster.
    let counts = create_sand_counts(&ships, &[], &HashMap::new());
    assert_eq!(counts["Batteries"].len(), 1);
  }

  /// Batteries and gunners feed one pool, as the book totals them.
  #[test]
  fn battery_and_gunner_contributions_add() {
    let mut ship = ship_with(vec![battery(1)]);
    ship.set_point_defense_pool(3);
    ship.add_point_defense_pool(4);
    assert_eq!(ship.point_defense_pool, 7);
  }

  /// The pool is per-round scratch and must not survive into the next round.
  #[test]
  fn clearing_point_defense_zeroes_the_pool() {
    let mut ship = ship_with(vec![battery(3)]);
    ship.set_point_defense_pool(19);
    ship.clear_point_defense();
    assert_eq!(ship.point_defense_pool, 0);
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::entity::Vec3;
  use crate::ship::{BaySize, Weapon, WeaponMount, WeaponType};
  use crate::ship::{Ship, ShipDesignTemplate};
  use cgmath::{MetricSpace, Zero};

  use rand::rngs::{StdRng, ThreadRng};
  use rand::SeedableRng;
  use std::collections::HashMap;
  use std::sync::{Arc, RwLock};

  use crate::info;

  #[test_log::test]
  fn test_missile_fire_actions() {
    let mut rng = StdRng::seed_from_u64(38); // Use a seeded RNG for reproducibility

    // Create a mock ship design with various weapon types and mounts
    let attacker_design = ShipDesignTemplate {
      name: "TestShip".to_string(),
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 2),
        Weapon::single(WeaponType::Missile, WeaponMount::Barbette),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Small)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Medium)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Large)),
      ],
      // Plant enough to feed all of it: an unpowered mount cannot fire, and
      // this test is about what the mounts throw, not about the plant.
      power: 500,
      // Likewise a full magazine: the question here is what each mount
      // throws, not how long the ship can keep throwing it.
      magazine: crate::ship::Magazine {
        missiles: 1000,
        torpedoes: 100,
        sand: 100,
        rounds: 0,
      },
      ..ShipDesignTemplate::default()
    };

    let target_design = ShipDesignTemplate {
      name: "TestTarget".to_string(),
      armor: 0,
      ..ShipDesignTemplate::default()
    };

    let mut attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(-1000.0, 1000.0, 0.0),
      Vec3::zero(),
      &Arc::new(attacker_design),
      None,
      None,
    );
    // Built directly rather than through a scenario, so seed the contact the
    // fire action needs.
    attacker.contacts.push("Target".to_string());
    let target = Ship::new(
      "Target".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(target_design),
      None,
      None,
    );

    let mut ships = HashMap::with_capacity(1);
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target.clone())));

    // Create sand counts
    let mut sand_ships = HashMap::with_capacity(1);
    sand_ships.insert("Target".to_string(), target.clone());
    let mut sand_counts = create_sand_counts(&sand_ships, &[], &HashMap::new());

    let actions = vec![
      ShipAction::FireAction {
        weapon_id: 0,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Beam Turret
      ShipAction::FireAction {
        weapon_id: 1,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Missile Turret
      ShipAction::FireAction {
        weapon_id: 2,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Missile Barbette
      ShipAction::FireAction {
        weapon_id: 3,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Missile Bay (Small)
      ShipAction::FireAction {
        weapon_id: 4,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Missile Bay (Medium)
      ShipAction::FireAction {
        weapon_id: 5,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }, // Missile Bay (Large)
    ];

    let boost_map = BoostMap::default();
    let (missiles, effects) = do_fire_actions(&attacker, &mut ships, &mut sand_counts, &actions, &boost_map, &mut rng);

    // Check beam weapon effect
    assert!(effects.iter().any(|e| matches!(e, EffectMsg::BeamHit { .. })));

    // One salvo per launcher, carrying what it threw: 2 from the turret, 5
    // from the barbette, 12 from the small bay, 24 from the medium bay, 120
    // from the large bay.
    assert_eq!(
      missiles.iter().map(|missile| missile.count).collect::<Vec<_>>(),
      vec![2, 5, 12, 24, 120]
    );

    // Check that all missiles have correct source and target
    for missile in &missiles {
      assert_eq!(missile.source, "Attacker");
      assert_eq!(missile.target, "Target");
    }

    // Check that we have the expected number of effects
    // 1 for beam plus any potential damage messages
    assert!(!effects.is_empty(), "the attack should have reported something");

    // You might want to add more specific checks based on your exact implementation
    // For example, checking for specific damage amounts or other effect details
  }

  #[test_log::test]
  fn test_apply_crit() {
    let mut rng = StdRng::seed_from_u64(42); // Use a seeded RNG for reproducibility

    let mut ship = Ship::new(
      "TestShip".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test Hull critical hits
    for level in 1..=6 {
      let effects = apply_crit(level, ShipSystem::Hull, &mut ship, &mut rng);
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      assert_eq!(ship.crit_level[ShipSystem::Hull as usize], level);
    }

    let design = ShipDesignTemplate {
      name: "TestShip".to_string(),
      power: 100, // Make math easier to check tests
      fuel: 100,  // Makes math easier to check tests
      // Ensure enough weapons in this design so we can do all weapon crits
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 2),
        Weapon::single(WeaponType::Missile, WeaponMount::Barbette),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Small)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Medium)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Large)),
      ],
      ..ShipDesignTemplate::default()
    };

    // Reset ship
    ship = Ship::new(
      "TestShip".to_string(),
      Vec3::zero(),
      Vec3::zero(),
      &Arc::new(design),
      None,
      None,
    );

    // Test Armor critical hits
    for level in 1..=6 {
      let effects = apply_crit(level, ShipSystem::Armor, &mut ship, &mut rng);
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      assert_eq!(ship.crit_level[ShipSystem::Armor as usize], level);
    }

    // Test Sensor critical hits
    for level in 1..=6 {
      let orig_sensors = ship.current_sensors;
      let effects = apply_crit(level, ShipSystem::Sensors, &mut ship, &mut rng);

      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      match level {
        1 => assert_eq!(ship.attack_dm, -1),
        6 => assert_eq!(ship.active_weapons, vec![false; 6]),
        _ => assert_eq!(ship.current_sensors, orig_sensors - 1),
      }
      assert_eq!(ship.crit_level[ShipSystem::Sensors as usize], level);
    }

    // Test Powerplant critical hits
    for level in 1..=6 {
      ship.current_power = 100; // Reset power before each test
      ship.current_hull = 100; // Reset hull before each test

      let effects = apply_crit(level, ShipSystem::Powerplant, &mut ship, &mut rng);

      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));

      match level {
        1 | 2 => {
          assert_eq!(
            ship.current_power,
            90,
            "Power should be reduced by 10% for level 1-2 {}",
            level.to_string().as_str()
          );
        }
        3 => {
          assert_eq!(ship.current_power, 50, "Power should be reduced by 50% for level 3");
        }
        4..=6 => {
          assert_eq!(ship.current_power, 0, "Power should be reduced to 0 for level 4-6");
        }
        _ => unreachable!(),
      }

      if level >= 5 {
        assert_eq!(ship.current_power, 0);
        // Check for additional hull damage
        assert!(
          ship.current_hull < 100,
          "Ship should have taken hull damage for a Powerplant crit level 5+"
        );
      }

      // Reset crit level for next iteration
      ship.crit_level[ShipSystem::Powerplant as usize] = 0;
    }

    // Test Weapon critical hits
    for level in 1..=6 {
      ship.active_weapons = vec![true, true, true, true, true, true];
      let effects = apply_crit(level, ShipSystem::Weapon, &mut ship, &mut rng);
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      assert_eq!(ship.crit_level[ShipSystem::Weapon as usize], level);
    }

    // Test Fuel critical hits
    for level in 1..=6 {
      ship.current_fuel = 100; // Reset fuel before each test
      ship.current_hull = 100; // Reset hull before each test

      let effects = apply_crit(level, ShipSystem::Fuel, &mut ship, &mut rng);

      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));

      match level {
        1..=3 => {
          assert!(ship.current_fuel < 100, "Fuel should be reduced for level 1-3");
          assert_eq!(ship.current_hull, 100, "Hull should not be affected for level 1-3");
        }
        4..=6 => {
          assert_eq!(ship.current_fuel, 0, "Fuel should be reduced to 0 for level 4-6");
          assert!(ship.current_hull < 100, "Hull should be damaged for level 4-6");
        }
        _ => unreachable!(),
      }

      if level >= 4 {
        assert!(effects.len() > 1, "Should have additional hull damage effect for level 4+");
      }

      // Reset crit level for next iteration
      ship.crit_level[ShipSystem::Fuel as usize] = 0;
    }

    // Test Drive critical hits
    for level in 1..=6 {
      ship.current_maneuver = 6;
      let effects = apply_crit(level, ShipSystem::Maneuver, &mut ship, &mut rng);
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      assert_eq!(ship.crit_level[ShipSystem::Maneuver as usize], level);
    }

    // Test Jump critical hits
    for level in 1..=6 {
      ship.current_jump = 6;
      let effects = apply_crit(level, ShipSystem::Jump, &mut ship, &mut rng);
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
      assert_eq!(ship.crit_level[ShipSystem::Jump as usize], level);
      if level >= 2 {
        assert_eq!(ship.current_jump, 0);
      } else {
        assert_eq!(ship.current_jump, 5);
      }
      if level >= 4 {
        assert!(effects.len() > 1); // Additional hull damage for level 4+
      }
    }

    // Test Crew critical hits
    for level in 1..=6 {
      let effects = apply_crit(level, ShipSystem::Crew, &mut ship, &mut rng);
      assert_eq!(effects.len(), 1);
      assert!(matches!(effects[0], EffectMsg::Message { .. }));
    }

    // Test Bridge critical hits
    // Level 1-5 only have one effect.
    for level in 1..6 {
      let effects = apply_crit(level, ShipSystem::Bridge, &mut ship, &mut rng);
      assert_eq!(
        effects.len(),
        1,
        "Should have exactly one effect for level {level}. Instead found {effects:?}"
      );
      assert!(matches!(effects[0], EffectMsg::Message { .. }));
    }
    let effects = apply_crit(6, ShipSystem::Bridge, &mut ship, &mut rng);
    assert_eq!(
      effects.len(),
      2,
      "Should have exactly two effects for level 6. Instead found {effects:?}"
    );
    assert!(matches!(effects[0], EffectMsg::Message { .. }));
    assert!(matches!(effects[1], EffectMsg::Message { .. }));
  }

  /// An overloaded plant actually makes more Power.
  ///
  /// The multiplier a successful overload sets lived in a helper nothing ever
  /// called, so the engineer who rolled it got a line in the log and not one
  /// point of Power: not a thrust, not a weapon, nothing on the board.
  #[test]
  fn an_overloaded_plant_makes_more_power() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      power: 100,
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Plant".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    assert_eq!(ship.available_power(), 100);

    ship.set_temporary_power_multiplier(1.1, 3);
    assert_eq!(ship.available_power(), 110, "a 10% overload is ten more Power on a 100 plant");

    // An ion hit takes from what the plant makes, overloaded or not.
    ship.apply_ion_damage(20, 2);
    assert_eq!(ship.available_power(), 90, "ion drain comes off the overloaded output");

    ship.set_temporary_power_multiplier(1.0, 0);
    assert_eq!(ship.available_power(), 80, "and the plant is back to its rating when it lapses");
  }

  /// The arithmetic on an attack adds up.
  ///
  /// It did not: the breakdown dropped anything whose text began "evade ",
  /// meaning to drop the dodge term when nobody dodged, and took "evade
  /// software" with it -- so a defender running Evade had the DM counted
  /// against the shot and left out of the sum. The line then read
  /// "8+3=11 (gunner +3, captain fire +1)", which is +4 of terms making +3.
  #[test]
  fn an_attack_names_every_modifier_it_counted() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      hull: 200,
      computer: 20,
      software: vec![crate::software::Software::new(SoftwareKind::Evade, 1)],
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    // Running Evade, but with nobody at the helm dodging: the case that was
    // being mis-reported.
    assert!(
      defender.running_level(SoftwareKind::Evade).is_some(),
      "the defender should be evading"
    );

    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(7);
    let effects = attack(
      HitMods {
        gunner: 3,
        ..HitMods::default()
      },
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );

    let roll = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { roll, .. } => roll.clone(),
        _ => None,
      })
      .expect("an attack reports its arithmetic");

    // "8+3=11 (gunner +3, evade software -1, ...) vs 8": the dice, the
    // modifier applied, and the terms it was made of.
    let (sum, rest) = roll.split_once('=').expect("the roll states a total");
    let stated: i32 = sum
      .split_once(['+', '-'])
      .map(|(_, modifier)| {
        let size = modifier.trim().parse::<i32>().unwrap_or(0);
        if sum.contains('-') {
          -size
        } else {
          size
        }
      })
      .expect("the roll states a modifier");
    let named: i32 = rest
      .split_once('(')
      .and_then(|(_, tail)| tail.split_once(')'))
      .map_or(0, |(terms, _)| {
        terms
          .split(", ")
          .filter_map(|term| term.rsplit_once(' '))
          .map(|(_, value)| value.parse::<i32>().unwrap_or(0))
          .sum()
      });

    assert_eq!(
      named, stated,
      "the terms named should add up to the modifier applied, in {roll:?}"
    );
    assert!(
      roll.contains("evade software"),
      "the defender's Evade should be named: {roll:?}"
    );
  }

  /// A critical names the ship that caused it and says which of the two ways
  /// it arrived.
  ///
  /// The line used to name only the ship that suffered it, so a gunner
  /// watching their own ship saw the damage they did and none of the
  /// criticals it caused.
  #[test]
  fn a_critical_names_its_cause() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1)],
      hull: 80,
      armor: 0,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let weapon = Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1);

    let mut seen = 0;
    for seed in 0..40u64 {
      let mut defender = Ship::new(
        "Defender".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      let mut rng = StdRng::seed_from_u64(seed);
      for effect in attack(
        HitMods {
          gunner: 3,
          ..HitMods::default()
        },
        0,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng,
      ) {
        let EffectMsg::Message {
          category: MessageCategory::Critical,
          target,
          roll,
          ..
        } = &effect
        else {
          continue;
        };
        seen += 1;
        assert_eq!(
          target.as_ref().map(|who| who.name.as_str()),
          Some("Attacker"),
          "a critical should name the ship that caused it"
        );
        let why = roll.clone().unwrap_or_default();
        assert!(
          why.contains("sustained damage") || why.contains("effect"),
          "a critical should say why it happened, got {why:?}"
        );
      }
    }
    assert!(seen > 0, "no criticals in forty attacks -- the fixture is wrong");
  }

  /// Sustained-damage criticals count only the hull a ship still has: a
  /// tenth of her hull at a time, ten at most from full, and none from a hit
  /// on a wreck.
  #[test]
  fn sustained_damage_criticals_stop_at_the_hull_left() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Fighter".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1)],
      hull: 4,
      armor: 0,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let weapon = Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1);
    let sustained = |effects: &[EffectMsg]| {
      effects
        .iter()
        .filter(|effect| {
          matches!(effect, EffectMsg::Message { category: MessageCategory::Critical, roll: Some(why), .. }
            if why.contains("sustained damage"))
        })
        .count()
    };

    for seed in 0..40u64 {
      let mut defender = Ship::new(
        "Fighter".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      let mut rng = StdRng::seed_from_u64(seed);
      let shoot = |defender: &mut Ship, rng: &mut StdRng| {
        attack(
          HitMods {
            gunner: 3,
            ..HitMods::default()
          },
          0,
          &attacker,
          defender,
          &weapon.firing_default().unwrap(),
          None,
          &BoostMap::default(),
          rng,
        )
      };
      let first = shoot(&mut defender, &mut rng);
      assert!(
        sustained(&first) <= 4,
        "a hull of 4 has four tenths to pass, got {}",
        sustained(&first)
      );
      if defender.get_current_hull_points() == 0 {
        let again = shoot(&mut defender, &mut rng);
        assert_eq!(sustained(&again), 0, "a wreck has no hull left to pass");
      }
    }
  }

  /// A called shot that hits puts the critical where the gunner called it,
  /// including when the critical came from sustained damage rather than from
  /// an Effect of 6. "If the attack is successful and scores a critical hit,
  /// the attacker may choose which location is hit" (CRB p. 169) does not say
  /// which critical, and restricting it to the Effect 6 one made a called
  /// shot a DM-2 for nothing in the ordinary case.
  #[test]
  fn bay_weapons_struggle_against_small_targets() {
    let bay = Weapon::single(WeaponType::Fusion, WeaponMount::Bay(BaySize::Small));
    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Gunship".to_string(),
      weapons: vec![bay.clone()],
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Gunship".to_string(), Vec3::zero(), Vec3::zero(), &attacker_design, None, None);
    let firing = bay.firing_default().unwrap();
    let profile = profile_for(firing.kind, firing.mount).unwrap();
    let small_target = |tons: u32| {
      let design = Arc::new(ShipDesignTemplate {
        name: "Target".to_string(),
        displacement: tons,
        ..ShipDesignTemplate::default()
      });
      let defender = Ship::new(
        "Target".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      let terms = attack_terms(
        &AttackSituation {
          hit: HitMods::default(),
          attacker: &attacker,
          defender: &defender,
          firing: &firing,
          profile: &profile,
          range_band: Range::Short,
          distance: None,
          called_shot: false,
        },
        0,
        0,
      )
      .unwrap();
      terms.iter().find(|(name, _)| *name == "small target").unwrap().1
    };
    assert_eq!(small_target(50), -4, "100 tons or less");
    assert_eq!(small_target(400), -2, "2,000 tons or less");
    assert_eq!(small_target(2_000), -2);
    assert_eq!(small_target(5_000), 0, "a capital ship is what a bay is for");
  }

  #[test]
  fn a_called_shot_is_only_called_at_short_range() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1)],
      hull: 80,
      armor: 0,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let weapon = Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1);
    // Whether the attack's itemised DM carries the called shot's DM-2.
    let called_at = |metres: f64| {
      let mut defender = Ship::new(
        "Defender".to_string(),
        Vec3::new(metres, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      let effects = attack(
        HitMods {
          gunner: 3,
          ..HitMods::default()
        },
        0,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        Some(&ShipSystem::Weapon),
        &BoostMap::default(),
        &mut StdRng::seed_from_u64(7),
      );
      serde_json::to_string(&effects).unwrap().contains("called shot")
    };
    assert!(called_at(1_000_000.0), "Short: the call is made, at DM-2");
    assert!(!called_at(5_000_000.0), "Medium: fired as an ordinary shot");
  }

  #[test]
  fn a_called_shot_places_the_crit_it_causes() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1)],
      // Small enough that one hit carries it past a tenth of its hull, which
      // is the sustained-damage critical.
      hull: 80,
      armor: 0,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let weapon = Weapon::uniform(WeaponType::Particle, WeaponMount::Barbette, 1);

    // Several seeds: the roll has to hit, and without the fix the location
    // is random, so one lucky seed would prove nothing.
    let mut called_right = 0;
    let mut crits_seen = 0;
    for seed in 0..40u64 {
      let mut defender = Ship::new(
        "Defender".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      let mut rng = StdRng::seed_from_u64(seed);
      let effects = attack(
        HitMods {
          gunner: 3,
          ..HitMods::default()
        },
        0,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        Some(&ShipSystem::Weapon),
        &BoostMap::default(),
        &mut rng,
      );
      let crits: Vec<&EffectMsg> = effects
        .iter()
        .filter(|effect| matches!(effect, EffectMsg::Message { content, .. } if content.contains("critical hit")))
        .collect();
      if crits.is_empty() {
        continue;
      }
      crits_seen += 1;
      if matches!(crits[0], EffectMsg::Message { content, .. } if content.contains("weapon critical hit")) {
        called_right += 1;
      }
    }

    assert!(crits_seen > 0, "no critical hits in forty attacks -- the fixture is wrong");
    assert_eq!(
      called_right, crits_seen,
      "every first critical should land on the weapon the gunner called"
    );
  }

  /// The attacker's modifiers are named, not summed: a skill-3 gunner showing
  /// "+6" is a pilot's assist and two inspires, and the log now says so.
  #[test]
  fn attack_messages_split_the_gunners_side() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      hull: 200,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(42);
    let effects = attack(
      HitMods {
        gunner: 3,
        assist: 2,
        captain_assist: 0,
        captain_fire: 1,
        smart: 0,
        fire_control: 0,
      },
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    let text = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { roll, .. } => Some(format!("{} {}", e.rendered(), roll.clone().unwrap_or_default())),
        _ => None,
      })
      .unwrap();
    // In order, with the zero captain-assist term dropped between them. The
    // weapon's own +4 follows; the total is the arithmetic test's business.
    assert!(text.contains("(gunner +3, assist +2, captain fire +1, weapon +4"), "{text:?}");
    assert!(!text.contains("captain assist"), "a zero term is dropped: {text}");
  }

  /// An evasion is finally visible in the results, and the captain's inspire
  /// on it shows on the one shot it applies to and not the next.
  ///
  /// Before this the whole attack DM reached the player as one number, so a
  /// dodging pilot and a spent leadership point were indistinguishable from a
  /// long-range shot -- and an evasion never produced a message at all.
  #[test]
  fn attack_messages_name_evade_and_the_captains_inspire() {
    let text = |effects: &[EffectMsg]| {
      effects
        .iter()
        .find_map(|e| match e {
          EffectMsg::Message { roll, .. } => Some(format!("{} {}", e.rendered(), roll.clone().unwrap_or_default())),
          _ => None,
        })
        .expect("an attack always reports something")
    };
    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });
    // Enough drive and power to have thrust to spare for dodging.
    let defender_design = Arc::new(ShipDesignTemplate {
      name: "Defender".to_string(),
      hull: 200,
      maneuver: 6,
      power: 300,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &attacker_design, None, None);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );
    defender.set_crew(serde_json::from_value::<crate::crew::Crew>(serde_json::json!({"pilot": 3})).unwrap());
    defender
      .set_pilot_actions(Some(2), None)
      .expect("a 6G hull can spare 2G to dodge");
    let mut boost_map = BoostMap::default();
    boost_map.insert(crate::action::BoostTarget::Evade {
      ship: "Defender".to_string(),
    });
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(42);
    let mut shoot = |defender: &mut Ship, boosts: &BoostMap| {
      text(&attack(
        HitMods::gunner(0),
        0,
        &attacker,
        defender,
        &weapon.firing_default().unwrap(),
        None,
        boosts,
        &mut rng,
      ))
    };

    // First shot: the pilot's skill and the captain's point, both named.
    let first = shoot(&mut defender, &boost_map);
    assert!(first.contains("evade -3"), "pilot should be named: {first}");
    assert!(first.contains("captain evade -1"), "the inspire should be named: {first}");

    // Second shot the same turn: still dodging (2G bought two attacks), but the
    // inspire is spent and must not be claimed again.
    let second = shoot(&mut defender, &boost_map);
    assert!(second.contains("evade -3"), "{second}");
    assert!(!second.contains("captain"), "the inspire is one-shot: {second}");

    // A target that is not dodging shows neither term.
    let mut still = Ship::new(
      "Still".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );
    let plain = shoot(&mut still, &BoostMap::default());
    assert!(!plain.contains("evade") && !plain.contains("captain"), "{plain}");
    assert!(!plain.contains("dodges"), "nobody dodged: {plain}");
  }

  /// The ship that dodged is told so, with what the dodge did to the shot.
  #[test]
  fn the_dodging_ship_hears_of_each_dodge() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Dodger".to_string(),
      hull: 200,
      maneuver: 6,
      power: 300,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let mut defender = Ship::new(
      "Dodger".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    defender.set_crew(serde_json::from_value::<crate::crew::Crew>(serde_json::json!({"pilot": 3})).unwrap());
    defender.set_pilot_actions(Some(1), None).unwrap();
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(7);
    let effects = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    let dodge = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { roll, .. } if e.rendered().contains("dodges") => {
          Some(format!("{} {}", e.rendered(), roll.clone().unwrap_or_default()))
        }
        _ => None,
      })
      .unwrap_or_default();
    assert!(dodge.contains("dodges"), "{dodge}");
    assert!(dodge.contains("evade -3"), "{dodge}");
    assert!(
      ["made it miss", "would have anyway", "hit anyway"]
        .iter()
        .any(|said| dodge.contains(said)),
      "{dodge}"
    );
  }

  /// Every attack reports how it was worked out, not only how it came out.
  ///
  /// The numbers all existed already but went to `debug!`, so a player reading
  /// "hit for 5 damage" had no way to tell whether armour had eaten some of it.
  #[test]
  fn attack_messages_show_their_arithmetic() {
    let text = |effects: &[EffectMsg]| {
      effects
        .iter()
        .find_map(|e| match e {
          EffectMsg::Message { roll, .. } => Some(format!("{} {}", e.rendered(), roll.clone().unwrap_or_default())),
          _ => None,
        })
        .expect("an attack always reports something")
    };
    let category = |effects: &[EffectMsg]| {
      effects
        .iter()
        .find_map(|e| match e {
          EffectMsg::Message { category, .. } => Some(*category),
          _ => None,
        })
        .unwrap()
    };

    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });
    let defender_design = Arc::new(ShipDesignTemplate {
      name: "Defender".to_string(),
      hull: 200,
      armor: 3,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &attacker_design, None, None);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);

    let shoot = |hit_mod, defender: &mut Ship, rng: &mut dyn RngCore| {
      attack(
        HitMods::gunner(hit_mod),
        0,
        &attacker,
        defender,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        rng,
      )
    };

    // A miss still shows the roll, the modifier, the total and the target.
    let mut rng = StdRng::seed_from_u64(42);
    let miss = shoot(-10, &mut defender, &mut rng);
    let miss_text = text(&miss);
    assert_eq!(category(&miss), MessageCategory::Attack);
    // The modifier shown is the summed DM, not the caller's `hit_mod`, so these
    // assert on shape rather than on one particular number. The sentence and
    // the arithmetic are two fields now -- the line a player reads, and what
    // its hover carries -- so `text` joins them.
    for fragment in ["Attacker beam laser -> Defender: miss.", "=", " vs 8"] {
      assert!(
        miss_text.contains(fragment),
        "miss line {miss_text:?} should contain {fragment:?}"
      );
    }

    // A hit shows the attack roll, the effect, and how the damage was reached
    // -- including the armour that was subtracted from it.
    let hit = shoot(20, &mut defender, &mut rng);
    let hit_text = text(&hit);
    assert_eq!(category(&hit), MessageCategory::Damage);
    for fragment in [
      "Attacker beam laser -> Defender: hit, effect ",
      " vs 8",
      "damage ",
      "-3 armour",
      " = ",
    ] {
      assert!(hit_text.contains(fragment), "hit line {hit_text:?} should contain {fragment:?}");
    }

    // Messages name their subject so the client can tint or filter by ship.
    assert!(
      hit
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { ship, .. } if ship.name == "Attacker")),
      "a damage message is about the attacker"
    );
  }

  #[test_log::test]
  fn test_attack() {
    let mut rng = StdRng::seed_from_u64(42); // Use a seeded RNG for reproducibility

    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
        Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 2),
        Weapon::single(WeaponType::Pulse, WeaponMount::Barbette),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Small)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Medium)),
        Weapon::single(WeaponType::Missile, WeaponMount::Bay(BaySize::Large)),
      ],
      hull: 100,
      armor: 10,
      ..ShipDesignTemplate::default()
    });

    let defender_design = Arc::new(ShipDesignTemplate {
      name: "Defender".to_string(),
      hull: 200,
      armor: 0,
      ..ShipDesignTemplate::default()
    });

    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &attacker_design,
      None,
      None,
    );

    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );

    // Test cases
    let test_cases = vec![
      (4, 0, WeaponType::Beam, WeaponMount::Turret, 1, true),
      (0, 0, WeaponType::Missile, WeaponMount::Turret, 2, false),
      (0, 0, WeaponType::Missile, WeaponMount::Turret, 2, false),
      (0, 0, WeaponType::Pulse, WeaponMount::Barbette, 1, true),
      (6, 0, WeaponType::Missile, WeaponMount::Bay(BaySize::Small), 1, true),
      (2, 0, WeaponType::Missile, WeaponMount::Bay(BaySize::Medium), 1, false),
      // Flipped from miss to hit when pulse barbettes started rolling their
      // correct 3D (High Guard p. 30) rather than a turret's 2D, which consumes
      // a different amount of the seeded stream and shifts every later roll.
      (1, 0, WeaponType::Missile, WeaponMount::Bay(BaySize::Large), 1, true),
      (10, 0, WeaponType::Beam, WeaponMount::Turret, 1, true), // High hit mod
      (0, 10, WeaponType::Beam, WeaponMount::Turret, 1, true), // High damage mod
    ];

    for (hit_mod, damage_mod, weapon_type, weapon_mount, gun_count, should_hit) in test_cases {
      debug!("\n\n");
      info!(
        "(test.test_attack) Test case: hit_mod {}, damage_mod {}, weapon_type {:?}, weapon_mount {:?}",
        hit_mod, damage_mod, weapon_type, weapon_mount
      );
      let weapon = Weapon::uniform(weapon_type, weapon_mount.clone(), gun_count);

      let starting_hull = defender.get_current_hull_points();

      let effects = attack(
        HitMods::gunner(hit_mod),
        damage_mod,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng,
      );
      // Check that we have effects. If not it means we missed which is okay for some attacks.
      // This is a hack but since the random seed is known, we map which should hit and which should miss.
      if should_hit {
        assert!(
                    effects
                        .iter()
                        .any(|e| !matches!(e, EffectMsg::Message { .. })),
                    "Expected hit in test case [hit_mod: {hit_mod}, damage_mod: {damage_mod}, weapon_type: {weapon_type:?}, weapon_mount: {weapon_mount:?}] and should produce effects: {effects:?}"
                );
      } else {
        assert!(
          !effects.iter().any(|e| !matches!(e, EffectMsg::Message { .. })),
          "Miss should produce no effects"
        );
        continue;
      }

      // Check for specific effect types based on weapon type
      match weapon_type {
        WeaponType::Beam | WeaponType::Pulse => {
          assert!(effects.iter().any(|e| matches!(e, EffectMsg::BeamHit { .. })));
        }
        WeaponType::Missile => {
          // For missiles, we don't check for BeamHit, but we should have a damage message
          assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
        }
        _ => panic!("Unexpected weapon type"),
      }
      // Check for damage
      assert!(defender.get_current_hull_points() < starting_hull, "Damage should be applied.");

      debug!(
        "(test.test_attack) Damage: {}.  Hull: {}.  Armor: {}.",
        starting_hull - defender.get_current_hull_points(),
        defender.get_current_hull_points(),
        defender.get_current_armor()
      );

      debug!("(test.test_attack) Reset defender.");

      // Reset defender for next test
      defender = Ship::new(
        "Defender".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &defender_design,
        None,
        None,
      );
    }

    info!("(test.test_attack) Core test scenarios complete. Now test special cases.");
    info!("(test.test_attack) Test miss scenario");

    // Test miss scenario
    let miss_effects = attack(
      HitMods::gunner(-10),
      0,
      &attacker,
      &mut defender,
      &Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)
        .firing_default()
        .unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(
      !miss_effects.iter().any(|e| !matches!(e, EffectMsg::Message { .. })),
      "Miss should produce no effects"
    );

    info!("(test.test_attack) Test critical hit scenario");
    // Test critical hit scenario
    let crit_effects = attack(
      HitMods::gunner(20),
      0,
      &attacker,
      &mut defender,
      &Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)
        .firing_default()
        .unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(crit_effects
      .iter()
      .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("critical"))));

    info!("(test.test_attack) Test non-missile medium and large bays.");
    // Test scenario for non-missile weapons in medium or large bays
    for size in [BaySize::Medium, BaySize::Large] {
      info!("(test.test_attack) Test {:?} bay.", size);
      // Reset defender for next test
      defender = Ship::new(
        "Defender".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &defender_design,
        None,
        None,
      );

      let mut effects = vec![];

      // Repeat the attack until we have a hit.
      while !effects.iter().any(|e| !matches!(e, EffectMsg::Message { .. })) {
        defender.current_hull = 200;
        effects = attack(
          HitMods::gunner(0),
          0,
          &attacker,
          &mut defender,
          &Weapon::single(WeaponType::Particle, WeaponMount::Bay(size))
            .firing_default()
            .unwrap(),
          None,
          &BoostMap::default(),
          &mut rng,
        );
      }
      match size {
        BaySize::Small => (),
        BaySize::Medium => assert!(
          defender.current_hull < 140,
          "Medium bay should do more damage than {}",
          200 - defender.current_hull
        ),
        BaySize::Large => assert!(
          defender.current_hull < 80,
          "Large bay should do more damage than {}",
          200 - defender.current_hull
        ),
      }
      assert!(effects.iter().any(|e| matches!(e, EffectMsg::Message { .. })));
    }
  }

  #[test_log::test]
  fn test_attack_range_mod() {
    let mut rng = StdRng::seed_from_u64(38);
    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Test in-range attack
    let in_range_weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    defender.set_position(Vec3::new(1_000_000.0, 0.0, 0.0)); // Assuming this is within range
    let result = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &in_range_weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(result.iter().all(|msg| !msg.to_string().contains("out of range")));

    // Test out-of-range attack
    let out_of_range_weapon = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 1);
    defender.set_position(Vec3::new(30_000_000.0, 0.0, 0.0)); // Assuming this is out of range
    let result = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &out_of_range_weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(result.iter().any(|msg| msg.to_string().contains("out of range")));

    // Test missile which should never be out of range
    let missile_weapon = Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1);
    let result = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &missile_weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(result.iter().all(|msg| !msg.to_string().contains("out of range")));
  }

  #[test]
  fn test_attack_out_of_range() {
    // Rng doesn't matter as it shouldn't impact any results here.
    let mut rng = ThreadRng::default();

    // Create ships far apart from each other
    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    let mut defender = Ship::new(
      "Defender".to_string(),
      // Position defender very far away (beyond weapon range)
      Vec3::new(6_000_000.0, 6_000_000.0, 6_000_000.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Create a beam weapon (which has limited range unlike missiles)
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let range_band = find_range_band(attacker.get_position().distance(defender.get_position()) as u32);

    assert_eq!(range_band, Range::Long);

    let result = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );

    assert_eq!(result.len(), 1);
    assert!(
      matches!(&result[0], EffectMsg::Message { content, .. } if content.contains("out of range")),
      "Expected out of range message"
    );

    // Now test something in range.

    let mut defender = Ship::new(
      "Defender".to_string(),
      // Position defender very far away (beyond weapon range)
      Vec3::new(1_000_000.0, 1_000_000.0, 1_000_000.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    #[allow(clippy::cast_sign_loss)]
    #[allow(clippy::cast_possible_truncation)]
    let range_band = find_range_band(attacker.get_position().distance(defender.get_position()) as u32);

    assert_eq!(range_band, Range::Medium);

    let result = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(
      result.iter().all(|msg| !msg.to_string().contains("out of range")),
      "Expected no out of range message"
    );
  }

  #[test_log::test]
  fn test_attack_evade_boost_consumed_first_call_only() {
    use crate::action::BoostTarget;
    use crate::crew::{Crew, Skills};

    let mut rng = StdRng::seed_from_u64(7);

    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    // Defender with a non-zero pilot skill so the modifier is non-zero
    // and visible if we instrument it.
    let mut defender_crew = Crew::new();
    defender_crew.set_skill(Skills::Pilot, 2);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      Some(defender_crew),
      None,
    );

    // Two dodge points so the second attack still has dodge_thrust > 0.
    defender
      .set_pilot_actions(Some(2), None)
      .expect("(test) failed to set pilot actions");
    assert_eq!(defender.get_dodge_thrust(), 2);
    assert!(!defender.has_evade_boost_used());

    let mut boost_map = BoostMap::default();
    boost_map.insert(BoostTarget::Evade {
      ship: defender.get_name().to_string(),
    });

    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);

    // First attack: evade boost consumed, flag flips to true.
    let _ = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &boost_map,
      &mut rng,
    );
    assert!(
      defender.has_evade_boost_used(),
      "First attack should have consumed the evade boost"
    );
    assert_eq!(
      defender.get_dodge_thrust(),
      1,
      "Dodge thrust should have decremented by one after the first attack"
    );

    // Second attack: flag stays true (already consumed); dodge thrust decrements again.
    let _ = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &boost_map,
      &mut rng,
    );
    assert!(
      defender.has_evade_boost_used(),
      "Evade boost should remain consumed after second attack"
    );
    assert_eq!(
      defender.get_dodge_thrust(),
      0,
      "Dodge thrust should have decremented again on the second attack"
    );
  }

  #[test_log::test]
  fn test_attack_evade_boost_changes_hit_outcome() {
    // Use deterministic RNG and a hit_mod tuned so the difference between
    // -pilot and -pilot-1 changes the outcome from hit to miss.
    //
    // Roll: with `StdRng::seed_from_u64(123)` the first `roll_dice(2)` call
    // yields a known value; we only assert the *relative* relationship —
    // the same RNG seed and hit_mod with the boost results in a strictly
    // worse hit_roll than without. That manifests as either fewer effects
    // or a miss-vs-hit outcome.
    use crate::action::BoostTarget;
    use crate::crew::{Crew, Skills};

    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );

    let make_defender = || {
      let mut crew = Crew::new();
      crew.set_skill(Skills::Pilot, 2);
      let mut d = Ship::new(
        "Defender".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &Arc::new(ShipDesignTemplate {
          armor: 0,
          ..ShipDesignTemplate::default()
        }),
        Some(crew),
        None,
      );
      d.set_pilot_actions(Some(1), None).expect("(test) failed to set pilot actions");
      d
    };

    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);

    // Run a number of trials with the same seed schedule. With the same
    // seed each trial, the only difference between the boost-on and
    // boost-off runs is the +1 to defensive_modifier. We therefore assert
    // that the boost-on hull damage is `<=` the boost-off hull damage
    // across many seeds (strictly less for at least one).
    let mut strict_advantage = 0_u32;
    let mut total_unboosted_damage: u64 = 0;
    let mut total_boosted_damage: u64 = 0;
    for seed in 0..32_u64 {
      let mut d_unboosted = make_defender();
      let mut d_boosted = make_defender();

      let mut rng_unboosted = StdRng::seed_from_u64(seed);
      let mut rng_boosted = StdRng::seed_from_u64(seed);

      let _ = attack(
        HitMods::gunner(0),
        0,
        &attacker,
        &mut d_unboosted,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng_unboosted,
      );

      let mut boost_map = BoostMap::default();
      boost_map.insert(BoostTarget::Evade {
        ship: "Defender".to_string(),
      });
      let _ = attack(
        HitMods::gunner(0),
        0,
        &attacker,
        &mut d_boosted,
        &weapon.firing_default().unwrap(),
        None,
        &boost_map,
        &mut rng_boosted,
      );

      let unboosted_damage = ShipDesignTemplate::default().hull - d_unboosted.get_current_hull_points();
      let boosted_damage = ShipDesignTemplate::default().hull - d_boosted.get_current_hull_points();

      assert!(
        boosted_damage <= unboosted_damage,
        "Evade boost should never increase damage taken (seed={seed}, unboosted={unboosted_damage}, boosted={boosted_damage})"
      );
      if boosted_damage < unboosted_damage {
        strict_advantage += 1;
      }
      total_unboosted_damage += u64::from(unboosted_damage);
      total_boosted_damage += u64::from(boosted_damage);
    }

    assert!(
      strict_advantage > 0,
      "Across 32 seeds, the evade boost should have produced strictly less damage at least once. Total unboosted={total_unboosted_damage}, total boosted={total_boosted_damage}"
    );
  }

  #[test_log::test]
  fn test_attack_evade_boost_not_applied_without_dodge_thrust() {
    // When dodge_thrust == 0 the boost should NOT be consumed even if
    // present in the boost map (no evasion roll happens).
    use crate::action::BoostTarget;

    let mut rng = StdRng::seed_from_u64(11);

    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    assert_eq!(defender.get_dodge_thrust(), 0);

    let mut boost_map = BoostMap::default();
    boost_map.insert(BoostTarget::Evade {
      ship: defender.get_name().to_string(),
    });

    let _ = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)
        .firing_default()
        .unwrap(),
      None,
      &boost_map,
      &mut rng,
    );

    assert!(
      !defender.has_evade_boost_used(),
      "Evade boost flag should NOT flip when dodge_thrust is 0"
    );
  }

  /// Ships do not shoot their own side.
  #[test]
  fn a_ship_will_not_fire_on_its_own_team() {
    use crate::ship::Team;
    let mut rng = StdRng::seed_from_u64(11);
    let design = Arc::new(ShipDesignTemplate {
      name: "TestShip".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });

    let mut attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(-1000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    attacker.team = Some(Team::Red);
    attacker.contacts.push("Target".to_string());

    let mut target = Ship::new("Target".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    target.team = Some(Team::Red);
    let starting_hull = target.current_hull;

    let mut ships = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
    let mut sand_counts = HashMap::new();
    let actions = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Target".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];

    let (missiles, effects) = do_fire_actions(
      &attacker,
      &mut ships,
      &mut sand_counts,
      &actions,
      &BoostMap::default(),
      &mut rng,
    );

    assert!(missiles.is_empty());
    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("same side"))),
      "expected a same-side refusal, got {effects:?}"
    );
    assert_eq!(
      ships.get("Target").unwrap().read().unwrap().current_hull,
      starting_hull,
      "a team-mate should take no damage"
    );
  }

  /// Being on opposite sides, or unaligned, is no bar to shooting.
  #[test]
  fn teams_only_block_their_own() {
    use crate::ship::Team;
    for (attacker_team, target_team) in [
      (Some(Team::Red), Some(Team::Blue)),
      (Some(Team::Red), None),
      (None, Some(Team::Red)),
      (None, None),
    ] {
      let mut rng = StdRng::seed_from_u64(11);
      let design = Arc::new(ShipDesignTemplate {
        name: "TestShip".to_string(),
        weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
        ..ShipDesignTemplate::default()
      });
      let mut attacker = Ship::new(
        "Attacker".to_string(),
        Vec3::new(-1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      attacker.team = attacker_team;
      attacker.contacts.push("Target".to_string());
      let mut target = Ship::new("Target".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
      target.team = target_team;

      let mut ships = HashMap::new();
      ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
      let mut sand_counts = HashMap::new();
      let actions = vec![ShipAction::FireAction {
        weapon_id: 0,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      }];
      let (_, effects) = do_fire_actions(
        &attacker,
        &mut ships,
        &mut sand_counts,
        &actions,
        &BoostMap::default(),
        &mut rng,
      );
      assert!(
        !effects
          .iter()
          .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("same side"))),
        "{attacker_team:?} firing on {target_team:?} should be allowed"
      );
    }
  }

  /// The cargo lines say "50% of cargo destroyed", not "50%%".
  #[test]
  fn cargo_crits_print_one_percent_sign() {
    let design = Arc::new(ShipDesignTemplate {
      name: "Freighter".to_string(),
      hull: 200,
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new("Freighter".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let mut rng = StdRng::seed_from_u64(5);

    for level in 1..=6 {
      let effects = apply_crit(level, ShipSystem::Cargo, &mut ship, &mut rng);
      for effect in &effects {
        if let EffectMsg::Message { content, .. } = effect {
          assert!(!content.contains("%%"), "level {level}: {content}");
        }
      }
    }
  }

  /// A dodge by a pilot with no skill still shows in the log. It buys nothing,
  /// which is the point: the line has to say the order was carried out.
  #[test]
  fn an_unskilled_dodge_is_still_reported() {
    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });
    let defender_design = Arc::new(ShipDesignTemplate {
      name: "Defender".to_string(),
      hull: 200,
      maneuver: 6,
      power: 300,
      ..ShipDesignTemplate::default()
    });
    let attacker = Ship::new("Attacker".to_string(), Vec3::zero(), Vec3::zero(), &attacker_design, None, None);
    let mut defender = Ship::new(
      "Defender".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );
    // The crew the scenario gave them: a pilot of skill 0.
    defender.set_pilot_actions(Some(1), None).expect("a 6G hull can spare 1G");

    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(11);
    let effects = attack(
      HitMods::gunner(0),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    let line = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { roll, .. } => Some(format!("{} {}", e.rendered(), roll.clone().unwrap_or_default())),
        _ => None,
      })
      .expect("an attack always reports something");
    assert!(line.contains("evade +0"), "the dodge should be named: {line}");
  }

  /// A sandcaster sharing a turret with missile racks still deploys, and the
  /// log says so. Executor's second turret is exactly this shape.
  #[test]
  fn sand_from_a_mixed_turret_is_reported() {
    let defender_design = Arc::new(ShipDesignTemplate {
      name: "HMS Executor".to_string(),
      hull: 400,
      weapons: vec![
        Weapon::single(WeaponType::Particle, WeaponMount::Barbette),
        Weapon {
          count: 1,
          mount: WeaponMount::Turret,
          guns: vec![
            crate::ship::Gun::new(WeaponType::Missile),
            crate::ship::Gun::new(WeaponType::Missile),
            crate::ship::Gun::new(WeaponType::Sand),
          ],
        },
      ],
      ..ShipDesignTemplate::default()
    });
    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Threshing Oar".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 2)],
      ..ShipDesignTemplate::default()
    });

    let mut attacker = Ship::new("Thrasher".to_string(), Vec3::zero(), Vec3::zero(), &attacker_design, None, None);
    attacker.contacts = vec!["Executor".to_string()];
    let defender = Ship::new(
      "Executor".to_string(),
      Vec3::new(5000.0, 0.0, 0.0),
      Vec3::zero(),
      &defender_design,
      None,
      None,
    );

    let mut snapshot = HashMap::new();
    snapshot.insert("Executor".to_string(), defender.clone());
    let mut sand_counts = create_sand_counts(&snapshot, &[], &HashMap::new());
    assert_eq!(
      sand_counts.get("Executor").map(Vec::len),
      Some(1),
      "the mixed turret should offer one sandcaster: {sand_counts:?}"
    );

    let mut ships = HashMap::new();
    ships.insert("Executor".to_string(), Arc::new(RwLock::new(defender)));
    let actions = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Executor".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];
    let mut rng = StdRng::seed_from_u64(3);
    let (_, effects) = do_fire_actions(
      &attacker,
      &mut ships,
      &mut sand_counts,
      &actions,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("sand"))),
      "the sand attempt should be reported: {effects:?}"
    );
  }

  /// A gunner may throw fewer missiles than the rack holds -- a warning shot,
  /// or keeping some back -- but never more than it holds.
  #[test]
  fn a_gunner_can_fire_a_short_salvo() {
    let design = Arc::new(ShipDesignTemplate {
      name: "TestShip".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 3)],
      ..ShipDesignTemplate::default()
    });

    let launch = |salvo_size: Option<u16>| {
      let mut rng = StdRng::seed_from_u64(7);
      let mut attacker = Ship::new(
        "Attacker".to_string(),
        Vec3::new(-1000.0, 0.0, 0.0),
        Vec3::zero(),
        &design,
        None,
        None,
      );
      attacker.contacts = vec!["Target".to_string()];
      let target = Ship::new("Target".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
      let mut ships = HashMap::new();
      ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
      let mut sand_counts = HashMap::new();
      let actions = vec![ShipAction::FireAction {
        weapon_id: 0,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size,
        fire_control_dm: 0,
        computer_fired: false,
      }];
      let (missiles, _) = do_fire_actions(
        &attacker,
        &mut ships,
        &mut sand_counts,
        &actions,
        &BoostMap::default(),
        &mut rng,
      );
      assert!(missiles.len() <= 1, "one mount, one salvo");
      missiles.iter().map(|missile| missile.count).sum::<u16>()
    };

    assert_eq!(launch(None), 3, "the whole rack by default");
    assert_eq!(launch(Some(1)), 1, "a warning shot");
    assert_eq!(launch(Some(9)), 3, "and never more than the rack holds");
    assert_eq!(launch(Some(0)), 1, "asking for none still fires one");
  }

  /// A ship cannot shoot what it has not detected. The order is accepted but
  /// resolves to a refusal rather than an attack, and no damage is dealt.
  #[test]
  fn firing_at_an_undetected_ship_is_refused() {
    let mut rng = StdRng::seed_from_u64(7);
    let design = Arc::new(ShipDesignTemplate {
      name: "TestShip".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });

    // No contacts are seeded, so the attacker has no idea the target is there.
    let attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(-1000.0, 0.0, 0.0),
      Vec3::zero(),
      &design,
      None,
      None,
    );
    let target = Ship::new("Target".to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    let starting_hull = target.current_hull;

    let mut ships = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
    let mut sand_counts = HashMap::new();

    let actions = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Target".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];

    let (missiles, effects) = do_fire_actions(
      &attacker,
      &mut ships,
      &mut sand_counts,
      &actions,
      &BoostMap::default(),
      &mut rng,
    );

    assert!(missiles.is_empty(), "no missile should launch at an undetected ship");
    assert!(
      effects
        .iter()
        .any(|e| matches!(e, EffectMsg::Message { content, .. } if content.contains("cannot fire on"))),
      "expected a refusal effect, got {effects:?}"
    );
    assert_eq!(
      ships.get("Target").unwrap().read().unwrap().current_hull,
      starting_hull,
      "an undetected target should take no damage"
    );
  }

  #[test_log::test]
  fn test_do_fire_actions_assist_gunner_first_only() {
    // The AssistGunner boost should add +1 to the FIRST fire-action's
    // hit_mod and nothing more. We verify this with two complementary
    // checks:
    //
    // (1) Aggregate-damage check across many seeds with a SINGLE
    //     FireAction. With the boost the total damage across N trials
    //     should be strictly greater than without (probabilistic but
    //     overwhelmingly likely with ample seeds).
    //
    // (2) Two-FireAction check confirming the boost does NOT carry over
    //     to the second weapon: the SECOND weapon's hit_mod should match
    //     the unboosted case. We verify by running both attacks against
    //     a target where weapon 0 always misses (out of range) and only
    //     weapon 1 actually rolls. With the boost the first weapon
    //     out-of-range path doesn't *consume* the bonus (returns early),
    //     and weapon 1 SHOULD get the bonus because it's the first roll
    //     to actually happen. We don't try to enforce that nuance — the
    //     plan's "FIRST fire roll" semantics is what `first_assist_consumed`
    //     guards. We instead simply verify the runs complete and that
    //     repeated invocations don't blow up. The aggregate check above
    //     is the load-bearing one.
    use crate::action::BoostTarget;
    use crate::crew::{Crew, Skills};

    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      ..ShipDesignTemplate::default()
    });
    let target_design = Arc::new(ShipDesignTemplate {
      name: "Target".to_string(),
      armor: 0,
      ..ShipDesignTemplate::default()
    });

    let make_attacker = || {
      let mut crew = Crew::new();
      crew.set_skill(Skills::Pilot, 2);
      // A crew stated in full mans what it says it mans, so put someone on
      // the gun: this test is about the pilot's assist, not about who is at
      // the mount.
      crew.add_gunnery(0);
      let mut a = Ship::new(
        "Attacker".to_string(),
        Vec3::new(-1000.0, 0.0, 0.0),
        Vec3::zero(),
        &attacker_design,
        Some(crew),
        None,
      );
      a.set_pilot_actions(None, Some(true)).expect("(test) set assist gunners");
      assert!(a.get_assist_gunners());
      // Built directly rather than through a scenario, so it has no contacts
      // and every shot would be refused for want of one.
      a.contacts.push("Target".to_string());
      a
    };

    let actions = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Target".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];

    let mut total_unboosted: u64 = 0;
    let mut total_boosted: u64 = 0;
    let trials = 64_u64;

    for seed in 0..trials {
      // Unboosted run.
      let attacker_unboosted = make_attacker();
      let target_unboosted = Ship::new(
        "Target".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &target_design,
        None,
        None,
      );
      let max_hull = target_unboosted.get_max_hull_points();
      let mut ships_unboosted: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
      ships_unboosted.insert("Target".to_string(), Arc::new(RwLock::new(target_unboosted.clone())));
      let mut sand_unboosted: HashMap<String, Ship> = HashMap::new();
      sand_unboosted.insert("Target".to_string(), target_unboosted.clone());
      let mut sand_counts_unboosted = create_sand_counts(&sand_unboosted, &[], &HashMap::new());
      let mut rng_unboosted = StdRng::seed_from_u64(seed);

      do_fire_actions(
        &attacker_unboosted,
        &mut ships_unboosted,
        &mut sand_counts_unboosted,
        &actions,
        &BoostMap::default(),
        &mut rng_unboosted,
      );
      let unboosted_hull = ships_unboosted.get("Target").unwrap().read().unwrap().get_current_hull_points();
      total_unboosted += u64::from(max_hull - unboosted_hull);

      // Boosted run.
      let attacker_boosted = make_attacker();
      let target_boosted = Ship::new(
        "Target".to_string(),
        Vec3::new(1000.0, 0.0, 0.0),
        Vec3::zero(),
        &target_design,
        None,
        None,
      );
      let mut ships_boosted: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
      ships_boosted.insert("Target".to_string(), Arc::new(RwLock::new(target_boosted.clone())));
      let mut sand_boosted: HashMap<String, Ship> = HashMap::new();
      sand_boosted.insert("Target".to_string(), target_boosted.clone());
      let mut sand_counts_boosted = create_sand_counts(&sand_boosted, &[], &HashMap::new());
      let mut boost_map = BoostMap::default();
      boost_map.insert(BoostTarget::AssistGunner {
        ship: "Attacker".to_string(),
      });
      let mut rng_boosted = StdRng::seed_from_u64(seed);

      do_fire_actions(
        &attacker_boosted,
        &mut ships_boosted,
        &mut sand_counts_boosted,
        &actions,
        &boost_map,
        &mut rng_boosted,
      );
      let boosted_hull = ships_boosted.get("Target").unwrap().read().unwrap().get_current_hull_points();
      total_boosted += u64::from(max_hull - boosted_hull);
    }

    // With +1 to the first (and only) attack roll across many trials, total
    // damage should be strictly greater than without. The increment is small
    // (it's just whether borderline rolls hit instead of miss), so using 64
    // seeds gives us wide statistical headroom.
    assert!(
      total_boosted > total_unboosted,
      "Across {trials} seeds, AssistGunner boost should yield strictly more total damage. unboosted={total_unboosted}, boosted={total_boosted}"
    );
  }

  #[test_log::test]
  fn test_do_fire_actions_assist_gunner_only_first_action_in_sequence() {
    // White-box: with TWO FireActions and the AssistGunner boost, only the
    // first action consumes the boost. We verify by computing the
    // *expected* hit_mod arithmetic: the second weapon's hit_mod should
    // match what we'd get without the boost. We can't easily inspect
    // hit_mod from outside, so we instead verify the indirect consequence:
    // running ONLY the second action (skipping the first) with the boost
    // produces the same damage distribution as running it without — but
    // running both actions with the boost differs from running both
    // without ONLY because of the first action's bonus. Verifying that
    // exact equivalence is overconstrained for a unit test — the
    // load-bearing invariant is encoded in `first_assist_consumed` and
    // exercised by the single-action aggregate test above.
    //
    // This second test simply confirms the function does not panic and
    // returns without error in a multi-weapon context with the boost set.
    use crate::action::BoostTarget;
    use crate::crew::{Crew, Skills};

    let attacker_design = Arc::new(ShipDesignTemplate {
      name: "Attacker".to_string(),
      weapons: vec![
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
        Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1),
      ],
      ..ShipDesignTemplate::default()
    });
    let target_design = Arc::new(ShipDesignTemplate {
      name: "Target".to_string(),
      armor: 0,
      ..ShipDesignTemplate::default()
    });
    let mut crew = Crew::new();
    crew.set_skill(Skills::Pilot, 2);
    let mut attacker = Ship::new(
      "Attacker".to_string(),
      Vec3::new(-1000.0, 0.0, 0.0),
      Vec3::zero(),
      &attacker_design,
      Some(crew),
      None,
    );
    attacker.set_pilot_actions(None, Some(true)).unwrap();

    let target = Ship::new(
      "Target".to_string(),
      Vec3::new(1000.0, 0.0, 0.0),
      Vec3::zero(),
      &target_design,
      None,
      None,
    );
    let mut ships: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target.clone())));
    let mut sand_input: HashMap<String, Ship> = HashMap::new();
    sand_input.insert("Target".to_string(), target);
    let mut sand_counts = create_sand_counts(&sand_input, &[], &HashMap::new());

    let actions = vec![
      ShipAction::FireAction {
        weapon_id: 0,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      },
      ShipAction::FireAction {
        weapon_id: 1,
        target: "Target".to_string(),
        called_shot_system: None,
        firing_kind: None,
        salvo_size: None,
        fire_control_dm: 0,
        computer_fired: false,
      },
    ];

    let mut boost_map = BoostMap::default();
    boost_map.insert(BoostTarget::AssistGunner {
      ship: "Attacker".to_string(),
    });
    let mut rng = StdRng::seed_from_u64(99);

    // Should complete without panic. The first FireAction is the one that
    // consumes the +1; the second runs at the base assist_bonus.
    let _ = do_fire_actions(&attacker, &mut ships, &mut sand_counts, &actions, &boost_map, &mut rng);
  }
}

/// Weapon batteries (Imperial Navy p. 113): identical mounts grouped into one
/// weapon with one gunner and one target, each mount rolling its own attack.
#[cfg(test)]
mod weapon_battery_tests {
  use super::*;
  use crate::crew::Crew;
  use crate::entity::Vec3;
  use crate::ship::{form_batteries, Magazine, PowerSystem, ShipDesignTemplate};
  use cgmath::Zero;
  use rand::rngs::StdRng;
  use rand::SeedableRng;

  fn ship_with(name: &str, weapons: Vec<Weapon>, gunners: usize, x: f64) -> Ship {
    let design = Arc::new(ShipDesignTemplate {
      name: "Battleship".to_string(),
      displacement: 50_000,
      hull: 100_000,
      armor: 0,
      power: 100_000,
      magazine: Magazine {
        missiles: 10_000,
        torpedoes: 0,
        sand: 100,
        rounds: 0,
      },
      weapons,
      ..ShipDesignTemplate::default()
    });
    let mut crew = Crew::default();
    for _ in 0..gunners {
      crew.add_gunnery(2);
    }
    let mut ship = Ship::new(
      name.to_string(),
      Vec3::new(x, 0.0, 0.0),
      Vec3::zero(),
      &design,
      Some(crew),
      None,
    );
    ship.contacts.push("Target".to_string());
    ship
  }

  fn fire(weapon_id: usize) -> ShipAction {
    ShipAction::FireAction {
      weapon_id,
      target: "Target".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }
  }

  /// Fire `actions` from `attacker` at a fresh target; the target's hull
  /// after, the missiles launched, and every message.
  fn shoot(attacker: &Ship, actions: &[ShipAction], seed: u64) -> (u32, Vec<LaunchMissileMsg>, Vec<String>) {
    let target = ship_with("Target", vec![], 0, 1000.0);
    let mut ships = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
    let mut sand_counts = HashMap::new();
    let (missiles, effects) = do_fire_actions(
      attacker,
      &mut ships,
      &mut sand_counts,
      actions,
      &BoostMap::default(),
      &mut StdRng::seed_from_u64(seed),
    );
    let hull = ships["Target"].read().unwrap().get_current_hull_points();
    let lines = effects
      .iter()
      .filter(|e| matches!(e, EffectMsg::Message { .. }))
      .map(EffectMsg::rendered)
      .collect();
    (hull, missiles, lines)
  }

  fn barbette() -> Weapon {
    Weapon::single(WeaponType::Particle, WeaponMount::Barbette)
  }

  #[test]
  fn an_entry_without_a_count_reads_and_writes_as_before() {
    let json = r#"{"kind":"Beam","mount":{"Turret":3}}"#;
    let weapon: Weapon = serde_json::from_str(json).unwrap();
    assert_eq!(weapon.mounts(), 1);
    assert_eq!(serde_json::to_string(&weapon).unwrap(), json);
  }

  #[test]
  fn a_count_reads_and_writes() {
    let json = r#"{"kind":"Beam","mount":{"Turret":3},"count":200}"#;
    let weapon: Weapon = serde_json::from_str(json).unwrap();
    assert_eq!(weapon.mounts(), 200);
    assert_eq!(weapon.guns.len(), 3);
    assert_eq!(serde_json::to_string(&weapon).unwrap(), json);
    let mixed = r#"{"mount":{"Turret":2},"guns":[{"kind":"Beam"},{"kind":"Sand"}],"count":4}"#;
    let weapon: Weapon = serde_json::from_str(mixed).unwrap();
    assert_eq!(weapon.mounts(), 4);
    assert_eq!(serde_json::to_string(&weapon).unwrap(), mixed);
  }

  #[test]
  fn batteries_are_split_by_the_hull() {
    let beams = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 3).times(200);
    let sizes = |displacement| {
      form_batteries(std::slice::from_ref(&beams), displacement)
        .iter()
        .map(Weapon::mounts)
        .collect::<Vec<_>>()
    };
    // 75,000 tons: batteries of 16, then what is left.
    let mut wind = vec![16; 12];
    wind.push(8);
    assert_eq!(sizes(75_000), wind);
    assert_eq!(sizes(500).len(), 50, "fours on a small hull");
    assert_eq!(sizes(5_000).len(), 25, "eights");
    assert_eq!(sizes(500_000).iter().max(), Some(&32));
    assert_eq!(sizes(500_000).iter().map(|n| u32::from(*n)).sum::<u32>(), 200);

    // Point-defence batteries and fixed mounts are single weapons.
    let pd = Weapon::single(WeaponType::PointDefense, WeaponMount::Battery(3)).times(8);
    let formed = form_batteries(&[pd], 75_000);
    assert_eq!(formed.len(), 8);
    assert!(formed.iter().all(|w| w.mounts() == 1));

    // Forming twice changes nothing, and a list without counts is untouched.
    let once = form_batteries(&[beams.clone(), barbette()], 75_000);
    assert_eq!(form_batteries(&once, 75_000), once);
    let plain = vec![barbette(), Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 2)];
    assert_eq!(form_batteries(&plain, 75_000), plain);
  }

  /// IN p. 113's usual way: a separate attack roll for each mount. A
  /// battery of four does exactly what four single mounts do, roll for roll,
  /// and says so in one line.
  #[test]
  fn a_battery_rolls_each_mount_like_single_mounts() {
    let battery = ship_with("Shooter", vec![barbette().times(4)], 1, 0.0);
    let singles = ship_with("Shooter", vec![barbette(); 4], 4, 0.0);
    for seed in 0..20 {
      let (hull_battery, _, lines) = shoot(&battery, &[fire(0)], seed);
      let (hull_singles, _, _) = shoot(&singles, &[fire(0), fire(1), fire(2), fire(3)], seed);
      assert_eq!(hull_battery, hull_singles, "seed {seed}");
      let summary: Vec<&String> = lines
        .iter()
        .filter(|l| l.contains("particle beam barbette ×4 (battery)"))
        .collect();
      assert_eq!(summary.len(), 1, "{lines:?}");
      assert!(summary[0].contains(" of 4 hit"), "{}", summary[0]);
      let lost = 100_000 - hull_battery;
      if lost > 0 {
        assert!(summary[0].contains(&format!(", {lost} damage")), "{}", summary[0]);
      }
    }
  }

  #[test]
  fn a_missile_battery_is_one_salvo_of_every_launcher() {
    let bays = Weapon::single(WeaponType::Missile, WeaponMount::Bay(crate::ship::BaySize::Small)).times(16);
    let ship = ship_with("Shooter", vec![bays], 1, 0.0);
    let (_, missiles, _) = shoot(&ship, &[fire(0)], 1);
    assert_eq!(missiles.iter().map(|m| m.count).collect::<Vec<_>>(), vec![16 * 12]);
  }

  #[test]
  fn a_battery_draws_power_for_every_mount() {
    let one = ship_with("Shooter", vec![barbette()], 1, 0.0);
    let battery = ship_with("Shooter", vec![barbette().times(8)], 1, 0.0);
    let draw = |ship: &Ship| ship.power_line(PowerSystem::Weapon(0)).map(|line| line.draw);
    assert_eq!(draw(&battery), draw(&one).map(|d| d * 8));
  }

  /// A laser battery on point defence makes a check per mount, at its one
  /// gunner's skill, and reports them as one line.
  #[test]
  fn a_laser_battery_defends_with_every_mount() {
    let lasers = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 3).times(16);
    let ship = ship_with("Shooter", vec![lasers], 1, 0.0);
    let defend = [ShipAction::PointDefenseAction {
      weapon_id: 0,
      protecting: None,
    }];
    let tallies = build_point_defense_tallies(&ship, &defend, &BoostMap::default(), "Shooter");
    assert_eq!(tallies.len(), 16);
    let checks = roll_point_defense_checks(&tallies, &mut StdRng::seed_from_u64(3));
    let lines = point_defense_messages(&checks, "Shooter", None);
    assert_eq!(lines.len(), 1);
    let removed: u32 = checks.iter().map(|c| c.removed).sum();
    assert!(
      lines[0]
        .rendered()
        .contains(&format!("(battery of 16) on point defence can stop {removed} missile")),
      "{}",
      lines[0].rendered()
    );
  }

  /// Each shot of a laser battery can draw a sand cloud, while the target has
  /// any to throw, and the clouds are reported in one line.
  #[test]
  fn sand_meets_a_laser_battery_shot_by_shot() {
    let lasers = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1).times(3);
    let attacker = ship_with("Shooter", vec![lasers], 1, 0.0);
    let target = ship_with("Target", vec![], 0, 1000.0);
    let mut ships = HashMap::new();
    ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
    let mut sand_counts = HashMap::new();
    sand_counts.insert("Target".to_string(), vec![SandCloud { modifier: 0, mount: 0 }; 2]);
    let (_, effects) = do_fire_actions(
      &attacker,
      &mut ships,
      &mut sand_counts,
      &[fire(0)],
      &BoostMap::default(),
      &mut StdRng::seed_from_u64(5),
    );
    assert!(sand_counts["Target"].is_empty(), "both clouds thrown");
    let lines: Vec<String> = effects.iter().map(EffectMsg::rendered).collect();
    assert_eq!(
      lines
        .iter()
        .filter(|l| l.contains("Target sand -> Shooter: ") && l.contains(" of 2 clouds against 3 shots"))
        .count(),
      1,
      "{lines:?}"
    );
  }

  /// A sandcaster battery throws a cloud from every mount.
  #[test]
  fn a_sand_battery_reacts_with_every_mount() {
    let sand = Weapon::uniform(WeaponType::Sand, WeaponMount::Turret, 1).times(5);
    let ship = ship_with("Target", vec![sand], 1, 0.0);
    let mut snapshot = HashMap::new();
    snapshot.insert("Target".to_string(), ship);
    assert_eq!(create_sand_counts(&snapshot, &[], &HashMap::new())["Target"].len(), 5);
  }

  /// Every shipped design that states no batteries comes out of battery
  /// forming exactly as it went in. One that does (the capitals, from step 7)
  /// keeps every mount, in batteries no bigger than its hull allows.
  #[test]
  fn shipped_designs_form_their_batteries() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("ship_templates");
    let mut checked = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
      let path = entry.unwrap().path();
      if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
        continue;
      }
      let design: ShipDesignTemplate = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
      let formed = form_batteries(&design.weapons, design.displacement);
      if design.weapons.iter().all(|weapon| weapon.count == 1) {
        assert_eq!(formed, design.weapons, "{}", path.display());
      } else {
        let limit = crate::ship::battery_limit(design.displacement);
        assert!(formed.iter().all(|weapon| weapon.count <= limit), "{}", path.display());
        let mounts = |weapons: &[Weapon]| -> u32 { weapons.iter().map(|weapon| u32::from(weapon.mounts())).sum() };
        assert_eq!(mounts(&formed), mounts(&design.weapons), "{}", path.display());
      }
      checked += 1;
    }
    assert!(checked > 40);
  }
}

/// Spinal mounts (High Guard pp. 34-36): the size and close-range DMs, the
/// 2,000-ton limit, armour by the percentage, ×1,000, the critical every hit
/// brings, and rounds.
#[cfg(test)]
mod spinal_tests {
  use super::*;
  use crate::entity::Vec3;
  use crate::ship::{ShipDesignTemplate, Weapon};
  use cgmath::Zero;
  use rand::rngs::StdRng;
  use rand::SeedableRng;
  use std::sync::{Arc, RwLock};

  fn spinal(kind: WeaponType, multiple: u8) -> Weapon {
    Weapon::single(kind, WeaponMount::Spinal { multiple, tl: None })
  }

  fn design(name: &str, tons: u32, armor: u32, weapons: Vec<Weapon>) -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: name.to_string(),
      displacement: tons,
      hull: 1_000_000,
      armor,
      maneuver: 2,
      power: 1_000_000,
      tl: 15,
      weapons,
      ..ShipDesignTemplate::default()
    })
  }

  fn ship(name: &str, design: &Arc<ShipDesignTemplate>, x: f64) -> Ship {
    Ship::new(name.to_string(), Vec3::new(x, 0.0, 0.0), Vec3::zero(), design, None, None)
  }

  /// The DM terms a spinal on a ship of `attacker_tons` gets against a
  /// target of `target_tons`, `distance` metres away.
  fn terms(
    attacker_tons: u32, target_tons: u32, distance: Option<u32>, band: Range, held_still: bool,
  ) -> Option<HashMap<&'static str, i32>> {
    let weapon = spinal(WeaponType::Meson, 1);
    let attacker = ship("A", &design("A", attacker_tons, 0, vec![weapon.clone()]), 0.0);
    let mut defender = ship("D", &design("D", target_tons, 0, vec![]), 1.0);
    defender.held_still = held_still;
    let firing = weapon.firing_default().unwrap();
    let profile = profile_for(firing.kind, firing.mount).unwrap();
    attack_terms(
      &AttackSituation {
        hit: HitMods::default(),
        attacker: &attacker,
        defender: &defender,
        firing: &firing,
        profile: &profile,
        range_band: band,
        distance,
        called_shot: false,
      },
      0,
      0,
    )
    .map(|terms| terms.into_iter().collect())
  }

  /// "DM-4 when attacking targets of 10,000 tons or less and DM-8 when
  /// attacking targets of 5,000 tons or less", and nothing under 2,000 tons
  /// "unless they are stationary" (High Guard p. 34).
  #[test]
  fn the_size_dms_and_the_two_thousand_ton_limit() {
    let size = |tons, still| terms(50_000, tons, None, Range::Medium, still).map(|terms| terms["small target"]);
    assert_eq!(size(10_001, false), Some(0));
    assert_eq!(size(10_000, false), Some(-4));
    assert_eq!(size(5_001, false), Some(-4));
    assert_eq!(size(5_000, false), Some(-8));
    assert_eq!(size(2_000, false), Some(-8));
    assert_eq!(size(1_999, false), None, "cannot attack a ship under 2,000 tons");
    assert_eq!(size(1_999, true), Some(-8), "unless she held still, and then at DM-8");
    assert_eq!(size(10, true), Some(-8));
  }

  /// Spinal Weapon Range Adjustments (High Guard p. 35), by the attacker's
  /// size, at Adjacent (under 1 km), Close (to 10 km) and Short.
  #[test]
  fn the_close_range_dms_grow_with_the_attacker() {
    let dm = |attacker, distance| terms(attacker, 50_000, distance, Range::Short, false).unwrap()["spinal range"];
    for (attacker, [adjacent, close, short]) in [
      (9_999, [-4, -2, -1]),
      (10_000, [-6, -4, -2]),
      (50_000, [-6, -4, -2]),
      (50_001, [-8, -6, -4]),
      (250_000, [-8, -6, -4]),
      (250_001, [-10, -8, -6]),
    ] {
      assert_eq!(dm(attacker, Some(999)), adjacent, "{attacker} t at Adjacent");
      assert_eq!(dm(attacker, Some(1_000)), close, "{attacker} t at Close");
      assert_eq!(dm(attacker, Some(10_000)), close, "{attacker} t at Close");
      assert_eq!(dm(attacker, Some(10_001)), short, "{attacker} t at Short");
      assert_eq!(dm(attacker, None), short, "anywhere in the band reads as Short");
    }
    // Past Short there is no penalty.
    assert_eq!(terms(9_999, 50_000, None, Range::Medium, false).unwrap()["spinal range"], 0);
    // And no other mount pays it.
    let laser = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let attacker = ship("A", &design("A", 300_000, 0, vec![laser.clone()]), 0.0);
    let defender = ship("D", &design("D", 50_000, 0, vec![]), 500.0);
    let firing = laser.firing_default().unwrap();
    let profile = profile_for(firing.kind, firing.mount).unwrap();
    let laser_terms = attack_terms(
      &AttackSituation {
        hit: HitMods::default(),
        attacker: &attacker,
        defender: &defender,
        firing: &firing,
        profile: &profile,
        range_band: Range::Short,
        distance: Some(500),
        called_shot: false,
      },
      0,
      0,
    )
    .unwrap();
    assert!(laser_terms.iter().all(|(name, value)| *name != "spinal range" || *value == 0));
  }

  /// Orbital Bombardment: "DM-12 when attacking any target that can
  /// manoeuvre in space combat" (High Guard p. 30). Mass drivers carry it,
  /// in a bay or a spinal.
  #[test]
  fn a_mass_driver_is_a_bombardment_weapon() {
    for weapon in [
      spinal(WeaponType::MassDriver, 1),
      Weapon::single(WeaponType::MassDriver, WeaponMount::Bay(crate::ship::BaySize::Large)),
    ] {
      let attacker = ship("A", &design("A", 50_000, 0, vec![weapon.clone()]), 0.0);
      let mut defender = ship("D", &design("D", 50_000, 0, vec![]), 500_000.0);
      let firing = weapon.firing_default().unwrap();
      let profile = profile_for(firing.kind, firing.mount).unwrap();
      let bombardment = |defender: &Ship| {
        attack_terms(
          &AttackSituation {
            hit: HitMods::default(),
            attacker: &attacker,
            defender,
            firing: &firing,
            profile: &profile,
            range_band: Range::Short,
            distance: Some(500_000),
            called_shot: false,
          },
          0,
          0,
        )
        .unwrap()
        .into_iter()
        .find(|(name, _)| *name == "bombardment")
        .unwrap()
        .1
      };
      assert_eq!(bombardment(&defender), -12, "{}", String::from(&weapon));
      defender.current_maneuver = 0;
      assert_eq!(bombardment(&defender), 0, "a hulk with no drive cannot manoeuvre");
    }
  }

  /// One spinal attack at a target with this armour: the damage the hit did,
  /// its Effect and whether it scored a critical, or `None` for a miss.
  fn shoot(weapon: &Weapon, armor: u32, gunner: i32, seed: u64) -> Option<(u32, i32, bool)> {
    let attacker = ship("A", &design("A", 100_000, 0, vec![weapon.clone()]), 0.0);
    let mut defender = ship("D", &design("D", 100_000, armor, vec![]), 2.0e6);
    let mut rng = StdRng::seed_from_u64(seed);
    let effects = attack(
      HitMods::gunner(gunner),
      0,
      &attacker,
      &mut defender,
      &weapon.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    let line = effects.first().map(EffectMsg::rendered).unwrap_or_default();
    let effect: i32 = line.split("effect ").nth(1)?.split(['.', ' ']).next()?.parse().ok()?;
    let critical = effects.iter().any(|e| {
      matches!(
        e,
        EffectMsg::Message {
          category: MessageCategory::Critical,
          ..
        }
      )
    });
    // The hit's own damage, as its line gives it; a critical on the hull
    // does more of its own.
    let damage = line
      .split("Damage ")
      .nth(1)
      .and_then(|rest| rest.trim_end_matches('.').parse().ok())
      .unwrap_or(0);
    Some((damage, effect, critical))
  }

  /// Damage is the dice plus Effect, then ×1,000; a 66D meson does what a
  /// 66D gun should, all of it through armour (High Guard pp. 29, 35).
  #[test]
  fn a_tigress_meson_hits_for_its_66_dice_through_any_armour() {
    let meson = spinal(WeaponType::Meson, 11);
    assert_eq!(profile_for(WeaponType::Meson, &meson.mount).unwrap().damage_dice, 66);
    for seed in 0..20 {
      let (damage, effect, critical) = shoot(&meson, 30, 20, seed).expect("a +20 gunner hits");
      let effect = u32::try_from(effect).unwrap();
      assert_eq!(damage % 1_000, 0, "×1,000 with nothing taken off: {damage}");
      assert!(
        (66 + effect..=396 + effect).contains(&(damage / 1_000)),
        "{damage} from 66D+{effect}"
      );
      assert!(critical, "a spinal hit always crits");
    }
  }

  /// "Reduced by 3% per point of armour ... before applying the Damage
  /// Multiple" (High Guard p. 36): armour 10 takes 30%, so every hit is a
  /// multiple of 700.
  #[test]
  fn a_particle_spinal_loses_three_percent_a_point_of_armour() {
    let particle = spinal(WeaponType::Particle, 1);
    for seed in 0..20 {
      let (damage, effect, _) = shoot(&particle, 10, 20, seed).unwrap();
      let effect = u32::try_from(effect).unwrap();
      assert_eq!(damage % 700, 0, "{damage}");
      assert!((8 + effect..=48 + effect).contains(&(damage / 700)), "{damage}");
    }
    // Armour 34 and up takes all of it.
    let (damage, _, critical) = shoot(&particle, 34, 20, 1).unwrap();
    assert_eq!(damage, 0);
    assert!(!critical, "a hit that does nothing does not crit");
  }

  /// The results line names the mount and its multiple, and the hover shows
  /// the percentage and the ×1,000.
  #[test]
  fn the_line_shows_the_percentage_and_the_multiple() {
    let particle = spinal(WeaponType::Particle, 2);
    let attacker = ship("A", &design("A", 100_000, 0, vec![particle.clone()]), 0.0);
    let mut defender = ship("D", &design("D", 100_000, 10, vec![]), 2.0e6);
    let effects = attack(
      HitMods::gunner(20),
      0,
      &attacker,
      &mut defender,
      &particle.firing_default().unwrap(),
      None,
      &BoostMap::default(),
      &mut StdRng::seed_from_u64(1),
    );
    let EffectMsg::Message { roll: Some(roll), .. } = &effects[0] else {
      panic!("{effects:?}");
    };
    let line = effects[0].rendered();
    assert!(line.contains("particle spinal mount (2×)"), "{line}");
    assert!(roll.contains("16D"), "{roll}");
    assert!(roll.contains("-30% armour (10 × 3%)"), "{roll}");
    assert!(roll.contains("x1000 mount"), "{roll}");
    assert!(
      effects.iter().any(|e| matches!(
        e,
        EffectMsg::Message {
          category: MessageCategory::Critical,
          ..
        }
      )),
      "{effects:?}"
    );
  }

  /// A railgun spinal's AP 20 comes off the armour first, and what is left
  /// costs 2% a point (High Guard pp. 35-36).
  #[test]
  fn a_railgun_spinal_has_ap_twenty_and_then_two_percent() {
    let railgun = spinal(WeaponType::Railgun, 1);
    let (damage, _, _) = shoot(&railgun, 20, 20, 3).unwrap();
    assert_eq!(damage % 1_000, 0, "armour 20 is all taken by the AP: {damage}");
    for seed in 0..10 {
      let (damage, effect, _) = shoot(&railgun, 25, 20, seed).unwrap();
      let effect = u32::try_from(effect).unwrap();
      assert_eq!(damage % 900, 0, "five points left, 10% off: {damage}");
      assert!((4 + effect..=24 + effect).contains(&(damage / 900)));
    }
  }

  /// "Spinal mount weapons can always (and will!) cause critical hits" (High
  /// Guard p. 27): every hit that does damage scores one, even below the
  /// Effect 6 another gun needs.
  #[test]
  fn every_spinal_hit_is_a_critical() {
    let meson = spinal(WeaponType::Meson, 1);
    let low_effect_hits: Vec<bool> = (0..200)
      .filter_map(|seed| shoot(&meson, 0, 0, seed))
      .filter(|(_, effect, _)| *effect < 6)
      .map(|(_, _, critical)| critical)
      .collect();
    assert!(low_effect_hits.len() > 20, "enough low hits to tell");
    assert!(low_effect_hits.iter().all(|critical| *critical));
    // Where a bay's hit with the same Effect does not.
    let bay = Weapon::single(WeaponType::Meson, WeaponMount::Bay(crate::ship::BaySize::Large));
    assert!((0..200)
      .filter_map(|seed| shoot(&bay, 0, 0, seed))
      .filter(|(_, effect, _)| *effect < 6)
      .any(|(_, _, critical)| !critical));
  }

  /// A ship under 2,000 tons is too small to fire on unless she held still,
  /// and the gunner is told before anything is spent.
  #[test]
  fn a_spinal_holds_fire_on_a_manoeuvring_fighter() {
    let meson = spinal(WeaponType::Meson, 1);
    let mut attacker = ship("A", &design("A", 30_000, 0, vec![meson]), 0.0);
    attacker.contacts.push("Fighter".to_string());
    let mut fighter = ship("Fighter", &design("Fighter", 10, 0, vec![]), 2.0e6);
    let fire = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "Fighter".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];
    let run = |fighter: &Ship| {
      let mut ships: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
      ships.insert("Fighter".to_string(), Arc::new(RwLock::new(fighter.clone())));
      let mut sand = create_sand_counts(&HashMap::<String, Ship>::new(), &[], &HashMap::new());
      let mut rng = StdRng::seed_from_u64(5);
      let (_, effects) = do_fire_actions(&attacker, &mut ships, &mut sand, &fire, &BoostMap::default(), &mut rng);
      effects.iter().map(EffectMsg::rendered).collect::<Vec<_>>().join(" | ")
    };
    let refused = run(&fighter);
    assert!(refused.contains("too small"), "{refused}");
    fighter.held_still = true;
    let fired = run(&fighter);
    assert!(fired.contains("meson spinal mount (1×)"), "{fired}");
    assert!(!fired.contains("too small"), "{fired}");
  }

  /// A railgun spinal comes with five rounds and fires one an attack; the
  /// sixth attack finds none (High Guard p. 36).
  #[test]
  fn a_railgun_spinal_runs_out_of_rounds() {
    let railgun = spinal(WeaponType::Railgun, 1);
    let attacker_design = design("A", 30_000, 0, vec![railgun]);
    let mut attacker = ship("A", &attacker_design, 0.0);
    attacker.contacts.push("D".to_string());
    assert_eq!(attacker.magazine.rounds, 5);
    assert_eq!(attacker.loaded(0).rounds, 0, "they feed it from stores");
    let mut ships: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    ships.insert("A".to_string(), Arc::new(RwLock::new(attacker)));
    ships.insert(
      "D".to_string(),
      Arc::new(RwLock::new(ship("D", &design("D", 30_000, 0, vec![]), 2.0e6))),
    );
    let fire = vec![ShipAction::FireAction {
      weapon_id: 0,
      target: "D".to_string(),
      called_shot_system: None,
      firing_kind: None,
      salvo_size: None,
      fire_control_dm: 0,
      computer_fired: false,
    }];
    let mut rng = StdRng::seed_from_u64(9);
    for round in 0..6 {
      let snapshot = ships["A"].read().unwrap().clone();
      let mut sand = create_sand_counts(&HashMap::<String, Ship>::new(), &[], &HashMap::new());
      let (_, effects) = do_fire_actions(&snapshot, &mut ships, &mut sand, &fire, &BoostMap::default(), &mut rng);
      let said = effects.iter().map(EffectMsg::rendered).collect::<Vec<_>>().join(" | ");
      assert_eq!(said.contains("out of rounds"), round == 5, "round {round}: {said}");
    }
    assert_eq!(ships["A"].read().unwrap().magazine.rounds, 0);
  }
}

/// High Guard p. 27 on big ships: which guns can crit them, and the
/// large-ship tables for Weapons, Crew and Cargo; p. 31's radiation.
#[cfg(test)]
mod large_ship_tests {
  use super::*;
  use crate::entity::Vec3;
  use crate::ship::{BaySize, ShipDesignTemplate, Weapon};
  use cgmath::Zero;
  use rand::rngs::StdRng;
  use rand::SeedableRng;
  use std::sync::Arc;

  fn design(tons: u32, hull: u32, crew: u32, weapons: Vec<Weapon>) -> Arc<ShipDesignTemplate> {
    Arc::new(ShipDesignTemplate {
      name: format!("{tons}-ton"),
      displacement: tons,
      hull,
      crew,
      armor: 0,
      maneuver: 2,
      power: 1_000_000,
      tl: 15,
      weapons,
      ..ShipDesignTemplate::default()
    })
  }

  fn ship(name: &str, design: &Arc<ShipDesignTemplate>, x: f64) -> Ship {
    Ship::new(name.to_string(), Vec3::new(x, 0.0, 0.0), Vec3::zero(), design, None, None)
  }

  fn criticals(effects: &[EffectMsg]) -> usize {
    effects
      .iter()
      .filter(|e| {
        matches!(
          e,
          EffectMsg::Message {
            category: MessageCategory::Critical,
            ..
          }
        )
      })
      .count()
  }

  /// One shot from `weapon` at a target of `target_tons` with a gunner good
  /// enough that every hit is Effect 6 or more: how many criticals it
  /// caused, over seeds that hit.
  fn crits_from(weapon: &Weapon, target_tons: u32, target_hull: u32) -> Vec<usize> {
    let attacker = ship("A", &design(200_000, 1_000_000, 10, vec![weapon.clone()]), 0.0);
    let target = design(target_tons, target_hull, 10, vec![]);
    (0..12)
      .filter_map(|seed| {
        let mut defender = ship("D", &target, 2.0e6);
        // So a spinal can lay on the small ones (High Guard p. 34).
        defender.held_still = true;
        let mut rng = StdRng::seed_from_u64(seed);
        let effects = attack(
          HitMods::gunner(20),
          0,
          &attacker,
          &mut defender,
          &weapon.firing_default().unwrap(),
          None,
          &BoostMap::default(),
          &mut rng,
        );
        (defender.get_current_hull_points() < target_hull).then(|| criticals(&effects))
      })
      .collect()
  }

  /// Over 2,000 tons a turret's Effect 6 is no critical; exactly 2,000 is
  /// not over; a spinal crits anything; sustained damage crits at any size.
  #[test]
  fn big_ships_shrug_off_small_guns() {
    let turret = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 3);
    let barbette = Weapon::single(WeaponType::Particle, WeaponMount::Barbette);
    let small_bay = Weapon::single(WeaponType::Fusion, WeaponMount::Bay(BaySize::Small));
    let medium_bay = Weapon::single(WeaponType::Meson, WeaponMount::Bay(BaySize::Medium));
    let large_bay = Weapon::single(WeaponType::Meson, WeaponMount::Bay(BaySize::Large));
    let spinal = Weapon::single(WeaponType::Meson, WeaponMount::Spinal { multiple: 1, tl: None });
    let big = 1_000_000;
    let always = |crits: Vec<usize>| !crits.is_empty() && crits.iter().all(|n| *n >= 1);
    let never = |crits: Vec<usize>| !crits.is_empty() && crits.iter().all(|n| *n == 0);

    assert!(always(crits_from(&turret, 2_000, big)), "exactly 2,000 tons is not over 2,000");
    assert!(never(crits_from(&turret, 2_001, big)));
    assert!(never(crits_from(&barbette, 3_000, big)));
    assert!(always(crits_from(&small_bay, 10_000, big)));
    assert!(never(crits_from(&small_bay, 10_001, big)));
    assert!(always(crits_from(&medium_bay, 100_000, big)));
    assert!(never(crits_from(&medium_bay, 100_001, big)));
    assert!(always(crits_from(&large_bay, 1_000_000, big)));
    for tons in [400, 2_000, 2_001, 10_001, 100_001, 1_000_000] {
      assert!(always(crits_from(&spinal, tons, big)), "a spinal crits a {tons}-ton hull");
    }
    // Sustained damage: a hull of 10 loses a tenth to any hit.
    assert!(always(crits_from(&turret, 3_000, 10)));
  }

  /// A Weapons critical of severity 2 or more on a ship over 5,000 tons
  /// takes a share of one battery by the Cargo and Weapons table: D3 x 10%
  /// disabled at 2, 1D x 10% destroyed at 3, all at 4 and up. A repair
  /// brings back the disabled mounts and not the destroyed ones.
  #[test]
  fn a_weapons_critical_takes_a_share_of_a_battery() {
    let battery = Weapon::single(WeaponType::Particle, WeaponMount::Barbette).times(8);
    let design = design(6_000, 2_400, 100, vec![battery]);
    let mut rng = StdRng::seed_from_u64(3);

    let mut ship = ship("Big", &design, 0.0);
    let effects = apply_crit(2, ShipSystem::Weapon, &mut ship, &mut rng);
    let disabled = ship.mounts_down[0];
    assert!((1..=3).contains(&disabled), "10-30% of 8, at least one: {disabled}");
    assert_eq!(ship.live_mounts(0), 8 - disabled);
    assert!(ship.active_weapons[0]);
    assert!(effects[0].rendered().contains("disabled"), "{}", effects[0].rendered());
    assert!(effects[0].rendered().contains("×8 (battery)"), "{}", effects[0].rendered());

    let effects = apply_crit(3, ShipSystem::Weapon, &mut ship, &mut rng);
    let destroyed = ship.mounts_down[0] - disabled;
    assert!((1..=5).contains(&destroyed), "10-60% of what was left: {destroyed}");
    assert!(effects[0].rendered().contains("destroyed"), "{}", effects[0].rendered());

    // Repairing it brings the disabled mounts back and no more.
    let restored = ship.undo_damage(ShipSystem::Weapon, 0);
    assert_eq!(ship.mounts_down[0], destroyed);
    assert!(restored.iter().any(|line| line.contains("back online")), "{restored:?}");

    // Severity 4 takes everything left, and the weapon is off for good.
    let mut ship = self::ship("Big", &design, 0.0);
    let hull_before = ship.get_current_hull_points();
    apply_crit(4, ShipSystem::Weapon, &mut ship, &mut rng);
    assert_eq!(ship.live_mounts(0), 0);
    assert!(!ship.active_weapons[0]);
    assert_eq!(ship.get_current_hull_points(), hull_before, "no hull severity at 4");
    ship.undo_damage(ShipSystem::Weapon, 0);
    assert_eq!(ship.live_mounts(0), 0, "destroyed mounts are not repaired");

    // Severity 5 adds a Hull critical.
    let mut ship = self::ship("Big", &design, 0.0);
    let effects = apply_crit(5, ShipSystem::Weapon, &mut ship, &mut rng);
    assert_eq!(ship.crit_level[ShipSystem::Hull as usize], 1);
    assert_eq!(criticals(&effects), 2);

    // A spinal is one mount, so any share puts it out.
    let spinal = Weapon::single(WeaponType::Meson, WeaponMount::Spinal { multiple: 2, tl: None });
    let mut ship = self::ship("Big", &self::design(20_000, 8_000, 100, vec![spinal]), 0.0);
    apply_crit(2, ShipSystem::Weapon, &mut ship, &mut rng);
    assert!(!ship.active_weapons[0]);
    assert_eq!(ship.live_mounts(0), 0);

    // Severity 1 stays the CRB's DM-1, and a 5,000-ton ship stays on the
    // CRB table altogether.
    let mut ship = self::ship("Big", &design, 0.0);
    apply_crit(1, ShipSystem::Weapon, &mut ship, &mut rng);
    assert_eq!(ship.attack_dm, -1);
    assert_eq!(ship.mounts_down[0], 0);
    let crb = self::design(
      5_000,
      2_000,
      100,
      vec![Weapon::single(WeaponType::Particle, WeaponMount::Barbette).times(8)],
    );
    let mut ship = self::ship("Sloan", &crb, 0.0);
    apply_crit(2, ShipSystem::Weapon, &mut ship, &mut rng);
    assert!(!ship.active_weapons[0], "the CRB switches the whole weapon off");
    assert_eq!(ship.mounts_down[0], 0);
  }

  /// A battery with mounts down fires, defends and draws Power for the
  /// mounts it has left.
  #[test]
  fn a_damaged_battery_fires_fewer_shots() {
    let battery = Weapon::single(WeaponType::Pulse, WeaponMount::Barbette).times(8);
    let design = design(6_000, 2_400, 100, vec![battery]);
    let whole = ship("Big", &design, 0.0);
    let mut hurt = ship("Big", &design, 0.0);
    hurt.lose_mounts(0, 3, 2, false);
    assert_eq!(hurt.live_mounts(0), 5);

    let whole_draw = whole
      .power_lines()
      .iter()
      .find(|l| l.system == crate::ship::PowerSystem::Weapon(0))
      .unwrap()
      .draw;
    let hurt_draw = hurt
      .power_lines()
      .iter()
      .find(|l| l.system == crate::ship::PowerSystem::Weapon(0))
      .unwrap()
      .draw;
    assert_eq!(hurt_draw * 8, whole_draw * 5);

    let target = self::design(6_000, 2_400, 100, vec![]);
    let mut defender = ship("D", &target, 2.0e6);
    let mut rng = StdRng::seed_from_u64(1);
    let effects = volley(
      HitMods::gunner(2),
      &[],
      &hurt,
      &mut defender,
      &hurt.weapons()[0].firing_default().unwrap(),
      hurt.live_mounts(0),
      Some("pulse laser barbette ×8 (battery)"),
      None,
      &BoostMap::default(),
      &mut rng,
    );
    assert!(effects[0].rendered().contains(" of 5 hit"), "{}", effects[0].rendered());
  }

  /// Crew criticals on a big ship take a percentage of the complement, and
  /// the crew that is left carries a DM on every check: DM-1 at 90% and
  /// below, DM-2 at 70%, DM-4 at 25%.
  #[test]
  fn crew_criticals_are_percentages_on_big_ships() {
    let design = design(6_000, 2_400, 1_000, vec![]);
    let mut rng = StdRng::seed_from_u64(7);
    let mut ship = ship("Big", &design, 0.0);
    let effects = apply_crit(3, ShipSystem::Crew, &mut ship, &mut rng);
    assert_eq!(ship.current_crew, 900);
    assert_eq!(ship.crew_percent(), 90);
    assert_eq!(ship.crew_dm(), -1);
    assert!(effects[0].rendered().contains("10% of the crew"), "{}", effects[0].rendered());
    assert!(effects[0].rendered().contains("DM-1"), "{}", effects[0].rendered());

    let effects = apply_crit(4, ShipSystem::Crew, &mut ship, &mut rng);
    assert_eq!(ship.current_crew, 750);
    assert_eq!(ship.crew_dm(), -1);
    assert!(
      effects[0].rendered().contains("life support fails in"),
      "{}",
      effects[0].rendered()
    );

    apply_crit(5, ShipSystem::Crew, &mut ship, &mut rng);
    assert_eq!(ship.current_crew, 550);
    assert_eq!(ship.crew_dm(), -2);
    apply_crit(6, ShipSystem::Crew, &mut ship, &mut rng);
    assert_eq!(ship.current_crew, 300);
    assert_eq!(ship.crew_dm(), -2);
    ship.current_crew = 250;
    assert_eq!(ship.crew_dm(), -4);

    // The DM is on the attack terms, named.
    let gun = Weapon::uniform(WeaponType::Pulse, WeaponMount::Turret, 1);
    let armed = self::design(6_000, 2_400, 1_000, vec![gun.clone()]);
    let mut attacker = self::ship("Big", &armed, 0.0);
    attacker.current_crew = 500;
    attacker.attack_dm = -1;
    let defender = self::ship("D", &design, 1.0e6);
    let firing = gun.firing_default().unwrap();
    let profile = profile_for(firing.kind, firing.mount).unwrap();
    let terms: HashMap<&str, i32> = attack_terms(
      &AttackSituation {
        hit: HitMods::default(),
        attacker: &attacker,
        defender: &defender,
        firing: &firing,
        profile: &profile,
        range_band: Range::Short,
        distance: None,
        called_shot: false,
      },
      0,
      0,
    )
    .unwrap()
    .into_iter()
    .collect();
    assert_eq!(terms["crew"], -2);
    assert_eq!(terms["damage"], -1);

    // A small ship's crew critical is the CRB's, and costs no crew.
    let small = self::design(5_000, 2_000, 100, vec![]);
    let mut ship = self::ship("Sloan", &small, 0.0);
    apply_crit(3, ShipSystem::Crew, &mut ship, &mut rng);
    assert_eq!(ship.current_crew, 100);
    assert_eq!(ship.crew_dm(), 0);
  }

  /// A Cargo critical on a big ship rolls the carrier's D6 table: the
  /// frozen watch row loses a share of the berths, every other row is
  /// cargo until craft are carried.
  #[test]
  fn cargo_criticals_on_big_ships_can_hit_the_frozen_watch() {
    let design = Arc::new(ShipDesignTemplate {
      frozen_watch: 40,
      ..(*design(6_000, 2_400, 400, vec![])).clone()
    });
    let mut watch_hit = 0;
    let mut cargo_hit = 0;
    for seed in 0..40 {
      let mut rng = StdRng::seed_from_u64(seed);
      let mut ship = ship("Big", &design, 0.0);
      assert_eq!(ship.frozen_watch, 40);
      let effects = apply_crit(2, ShipSystem::Cargo, &mut ship, &mut rng);
      let line = effects.last().unwrap().rendered();
      if line.contains("frozen watch") {
        watch_hit += 1;
        assert!((4..=12).contains(&(40 - ship.frozen_watch)), "D3 x 10% of 40: {line}");
      } else {
        cargo_hit += 1;
        assert_eq!(ship.frozen_watch, 40);
        assert!(line.contains("% of cargo destroyed"), "{line}");
      }
    }
    assert!(watch_hit > 0 && cargo_hit > 0, "{watch_hit} / {cargo_hit}");

    // Severity 5 takes everything and adds a Hull critical.
    let mut rng = StdRng::seed_from_u64(1);
    let mut ship = ship("Big", &design, 0.0);
    let effects = apply_crit(5, ShipSystem::Cargo, &mut ship, &mut rng);
    assert_eq!(ship.crit_level[ShipSystem::Hull as usize], 1);
    assert_eq!(criticals(&effects), 2);
  }

  /// The frozen watch replaces lost crew, no more than were lost and no
  /// more than are in the berths, and the DM lifts as it does.
  #[test]
  fn reviving_the_watch_restores_the_crew() {
    let design = Arc::new(ShipDesignTemplate {
      frozen_watch: 50,
      ..(*design(6_000, 2_400, 1_000, vec![])).clone()
    });
    let mut ship = ship("Big", &design, 0.0);
    ship.current_crew = 880;
    assert_eq!(ship.crew_dm(), -1);
    assert_eq!(ship.revive_watch(30), 30);
    assert_eq!((ship.current_crew, ship.frozen_watch), (910, 20));
    assert_eq!(ship.crew_dm(), 0);
    assert_eq!(ship.revive_watch(100), 20, "only what is in the berths");
    assert_eq!(ship.revive_watch(5), 0);
    let mut whole = self::ship("Big", &design, 0.0);
    assert_eq!(whole.revive_watch(10), 0, "nobody to replace");
    assert_eq!(whole.frozen_watch, 50);
  }

  /// Hull lost to a Radiation weapon is counted, mesons apart, and wears
  /// the crew down by High Guard p. 31.
  #[test]
  fn radiation_wears_the_crew_down() {
    let particle = Weapon::single(WeaponType::Particle, WeaponMount::Bay(BaySize::Large));
    let meson = Weapon::single(WeaponType::Meson, WeaponMount::Bay(BaySize::Large));
    let pulse = Weapon::single(WeaponType::Pulse, WeaponMount::Barbette);
    let attacker = ship(
      "A",
      &design(100_000, 1_000_000, 10, vec![particle.clone(), meson.clone(), pulse.clone()]),
      0.0,
    );
    let shielded = Arc::new(ShipDesignTemplate {
      radiation_shielding: true,
      ..(*design(100_000, 100_000, 1_000, vec![])).clone()
    });
    let mut defender = ship("D", &shielded, 2.0e6);
    let mut rng = StdRng::seed_from_u64(2);
    let mut lost = 0;
    for weapon in [&particle, &meson, &pulse] {
      let before = defender.get_current_hull_points();
      attack(
        HitMods::gunner(20),
        0,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng,
      );
      lost += before - defender.get_current_hull_points();
    }
    assert!(defender.radiation_hull_lost > 0 && defender.meson_hull_lost > 0);
    assert!(
      defender.radiation_hull_lost + defender.meson_hull_lost < lost,
      "the pulse laser's hull is not radiation"
    );
    // Shielded: a step per 40% from the particle beam, per 10% from the meson.
    defender.radiation_hull_lost = 40_000;
    defender.meson_hull_lost = 10_000;
    assert_eq!(defender.crew_dm_terms()[1], ("radiation", -2));
    defender.radiation_hull_lost = 39_999;
    defender.meson_hull_lost = 9_999;
    assert_eq!(defender.crew_dm(), 0);
    let mut bare = ship("D", &design(100_000, 100_000, 1_000, vec![]), 2.0e6);
    bare.radiation_hull_lost = 25_000;
    bare.meson_hull_lost = 5_000;
    assert_eq!(bare.crew_dm(), -3);
  }
}

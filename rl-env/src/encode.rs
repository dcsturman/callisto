//! What one ship sees, as a fixed-length vector of numbers, and which of the
//! actions are open to her.
//!
//! The vector is in the deciding ship's frame: everyone else's position and
//! velocity are relative to hers. Three blocks, in order:
//!
//! - herself, her side's goal, and what a withdrawal, a hand-off, the
//!   captain and jamming depend on ([`SELF_LEN`] numbers);
//! - the rest of her side, nearest first, [`FRIENDS`] slots of [`FRIEND_LEN`];
//! - every enemy on the plot, contacts and blips alike, nearest first,
//!   [`ENEMIES`] slots of [`ENEMY_LEN`].
//!
//! An empty slot is all zeros, its first number (present) included. Every
//! number is scaled to roughly [-1, 1]. Distances and speeds span six orders
//! of magnitude, so they go in as a direction and a logarithm.
//!
//! The slot an enemy or friend is in is also its index in the action heads
//! ([`crate::action`]), so the order is kept, as a [`Picture`], until the
//! actions chosen from it come back.

use std::collections::HashMap;

use ai_ships::observation::{Contact, Observation};
use callisto::entity::{Entity, Vec3};
use callisto::ship::{Goal, Ship};
use callisto::software::SoftwareKind;
use callisto::tactics::{self, BandOdds, BANDS};
use cgmath::InnerSpace;

use crate::action::{self, Head, MASK_LEN};
use crate::orders;

/// The most ships a side may have.
pub const MAX_SHIPS: usize = 20;
/// Slots for the rest of a ship's side.
pub const FRIENDS: usize = MAX_SHIPS - 1;
/// Slots for enemies.
pub const ENEMIES: usize = MAX_SHIPS;

pub const SELF_LEN: usize = 53;
pub const FRIEND_LEN: usize = 16;
pub const ENEMY_LEN: usize = 42;
/// One ship's observation.
pub const OBS_LEN: usize = SELF_LEN + FRIENDS * FRIEND_LEN + ENEMIES * ENEMY_LEN;

/// What a side last knew of an enemy it has lost contact with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
  pub round: u32,
  pub hull_share: f32,
  pub design_hull: u32,
  pub thrust: u8,
}

/// A side's memory of the enemies it has held, by name.
pub type Memory = HashMap<String, Seen>;

/// Which ship is in which slot: the enemies and friends, by name, in the
/// order the observation gave them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Picture {
  pub enemies: Vec<String>,
  pub friends: Vec<String>,
}

/// The side-wide facts every ship's observation carries.
pub struct Context<'a> {
  pub view: &'a Observation,
  pub memory: &'a Memory,
  pub round: u32,
  pub max_rounds: u32,
  /// The side's hull left, as a share of what it started with.
  pub own_hull_share: f32,
  /// The enemy's hull left as the side believes it: what it last saw.
  pub enemy_hull_share: f32,
  /// What the side is trying to do.
  pub goal: Goal,
}

/// The goals, in the order the observation flags them.
pub const GOALS: [Goal; 4] = [Goal::Destroy, Goal::Hold, Goal::Raid, Goal::Escape];

/// One enemy on the plot, contact or blip.
struct Enemy<'a> {
  name: &'a str,
  position: Vec3,
  velocity: Vec3,
  contact: Option<&'a Contact>,
}

/// Distance in metres, from 1 km to 1,000,000 km, onto [0, 1].
fn log_distance(metres: f64) -> f32 {
  ((1.0 + metres / 1_000.0).log10() / 6.0) as f32
}

/// Speed in m/s, up to about 100 km/s, onto [0, 1]; signed for a rate.
fn log_speed(metres_per_second: f64) -> f32 {
  (metres_per_second.signum() * (1.0 + metres_per_second.abs() / 10.0).log10() / 4.0) as f32
}

fn direction(v: Vec3) -> [f32; 3] {
  let length = v.magnitude();
  if length < 1e-9 {
    [0.0; 3]
  } else {
    [(v.x / length) as f32, (v.y / length) as f32, (v.z / length) as f32]
  }
}

fn share(part: u32, whole: u32) -> f32 {
  (part as f32 / whole.max(1) as f32).min(1.0)
}

/// Where another ship is and how she moves, relative to us: direction (3), log
/// distance, velocity direction (3), log speed, closing rate. Nine numbers.
fn relative(out: &mut [f32], me: &Ship, position: Vec3, velocity: Vec3) {
  let offset = position - me.get_position();
  let drift = velocity - me.get_velocity();
  out[0..3].copy_from_slice(&direction(offset));
  out[3] = log_distance(offset.magnitude());
  out[4..7].copy_from_slice(&direction(drift));
  out[7] = log_speed(drift.magnitude());
  // Positive when closing.
  let closing = if offset.magnitude() > 1e-9 {
    -drift.dot(offset) / offset.magnitude()
  } else {
    0.0
  };
  out[8] = log_speed(closing);
}

fn band_index(me: &Ship, position: Vec3) -> f32 {
  tactics::band_at((position - me.get_position()).magnitude()) as usize as f32 / 4.0
}

/// The posture a ship is in now, as the posture head numbers it.
#[must_use]
pub fn posture_of(ship: &Ship) -> usize {
  if ship.active_sensors {
    2
  } else {
    usize::from(ship.transmitting)
  }
}

/// The odds by band, best chance and expected damage as a share of `hull`:
/// ten numbers.
fn odds(out: &mut [f32], table: &[BandOdds], hull: u32) {
  for odds in table {
    let band = odds.band as usize;
    out[band] = odds.best_chance as f32;
    out[5 + band] = (odds.expected_damage / f64::from(hull.max(1))).min(1.0) as f32;
  }
}

/// Encode an absent ship -- destroyed, or a slot no ship fills: all zeros,
/// and only the first choice of every head open, so whatever is chosen does
/// nothing.
pub fn encode_absent(out: &mut [f32], mask: &mut [bool]) {
  out.fill(0.0);
  mask.fill(false);
  for head in Head::ALL {
    mask[head.offset()] = true;
  }
}

/// Encode `ship`'s view into `out` ([`OBS_LEN`]) and her open actions into
/// `mask` ([`MASK_LEN`]), and say which ship is in which slot.
///
/// # Panics
/// Panics if `out` or `mask` is the wrong length.
#[must_use]
pub fn encode_ship(ctx: &Context, me: &Ship, out: &mut [f32], mask: &mut [bool]) -> Picture {
  assert_eq!(out.len(), OBS_LEN);
  assert_eq!(mask.len(), MASK_LEN);
  out.fill(0.0);
  let view = ctx.view;
  let name = me.get_name();

  // The rest of the side, nearest first.
  let mut friends: Vec<&Ship> = view.own.iter().filter(|ship| ship.get_name() != name).collect();
  friends.sort_by(|a, b| {
    let from_me = |ship: &Ship| (ship.get_position() - me.get_position()).magnitude();
    from_me(a).total_cmp(&from_me(b)).then_with(|| a.get_name().cmp(b.get_name()))
  });
  friends.truncate(FRIENDS);

  // Every enemy on the plot, nearest first.
  let mut enemies: Vec<Enemy> = view
    .enemies()
    .into_iter()
    .map(|contact| Enemy {
      name: &contact.name,
      position: contact.position,
      velocity: contact.velocity,
      contact: Some(contact),
    })
    .chain(view.blips.iter().map(|blip| Enemy {
      name: &blip.name,
      position: blip.position,
      velocity: blip.velocity,
      contact: None,
    }))
    .collect();
  enemies.sort_by(|a, b| {
    let from_me = |enemy: &Enemy| (enemy.position - me.get_position()).magnitude();
    from_me(a).total_cmp(&from_me(b)).then_with(|| a.name.cmp(b.name))
  });
  enemies.truncate(ENEMIES);

  encode_self(ctx, me, &mut out[..SELF_LEN], friends.len(), enemies.len());

  for (slot, friend) in friends.iter().enumerate() {
    let at = SELF_LEN + slot * FRIEND_LEN;
    let row = &mut out[at..at + FRIEND_LEN];
    row[0] = 1.0;
    relative(&mut row[1..10], me, friend.get_position(), friend.get_velocity());
    row[10] = share(friend.current_hull, friend.design.hull);
    row[11] = f32::from(friend.max_acceleration()) / 10.0;
    row[12] = (f64::from(friend.design.hull.max(1)).log10() / 4.0) as f32;
    row[13] = band_index(me, friend.get_position());
    row[14] = friend.contacts.len().min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
    row[15] = f32::from(friend.get_dodge_thrust()) / 3.0;
  }

  for (slot, enemy) in enemies.iter().enumerate() {
    let at = SELF_LEN + FRIENDS * FRIEND_LEN + slot * ENEMY_LEN;
    encode_enemy(ctx, me, enemy, &mut out[at..at + ENEMY_LEN]);
  }

  masks(me, view, &friends, &enemies, mask);

  Picture {
    enemies: enemies.iter().map(|enemy| enemy.name.to_string()).collect(),
    friends: friends.iter().map(|ship| ship.get_name().to_string()).collect(),
  }
}

fn encode_self(ctx: &Context, me: &Ship, row: &mut [f32], friends: usize, enemies: usize) {
  let name = me.get_name();
  let view = ctx.view;
  row[0] = 1.0;
  row[1] = share(me.current_hull, me.design.hull);
  row[2] = me.get_current_armor().min(20) as f32 / 20.0;
  row[3] = f32::from(me.max_acceleration()) / 10.0;
  row[4] = f32::from(me.design.maneuver) / 10.0;
  row[5] = f32::from(me.get_dodge_thrust()) / 3.0;
  row[6] = f32::from(u8::from(me.get_assist_gunners()));
  row[7 + posture_of(me)] = 1.0;
  row[10] = me.contacts.len().min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  row[11] = me.sensor_locks.len().min(4) as f32 / 4.0;
  let velocity = me.get_velocity();
  row[12] = log_speed(velocity.magnitude());
  row[13..16].copy_from_slice(&direction(velocity));
  for (band_slot, band) in BANDS.iter().enumerate() {
    row[16 + band_slot] = f32::from(u8::from(!tactics::weapons_that_bear(me, *band).is_empty()));
  }
  // 21 is spare.
  for (system, level) in me.crit_level.iter().enumerate() {
    row[22 + system] = f32::from(*level).min(6.0) / 6.0;
  }
  row[33] = ctx.round as f32 / ctx.max_rounds.max(1) as f32;
  row[34] = ctx.own_hull_share;
  row[35] = ctx.enemy_hull_share;
  row[36] = (friends + 1) as f32 / MAX_SHIPS as f32;
  row[37] = enemies as f32 / MAX_SHIPS as f32;
  let watchers = view.watchers.iter().filter(|watcher| watcher.ours == name);
  let (watching, locking) =
    watchers.fold((0, 0), |(seen, locked), watcher| (seen + 1, locked + usize::from(watcher.lock)));
  row[38] = watching.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  row[39] = locking.min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
  let goal = GOALS.iter().position(|goal| *goal == ctx.goal).unwrap_or(0);
  row[40 + goal] = 1.0;
  // What a withdrawal, a hand-off, the captain and jamming turn on.
  row[44] = f32::from(u8::from(me.running_level(SoftwareKind::JumpControl).is_some()));
  row[45] = f32::from(u8::from(me.jump_powered()));
  let well = orders::well(me, view);
  row[46] = f32::from(u8::from(well.is_none()));
  row[47] = well.map_or(0.0, |(short, _)| log_distance(short));
  row[48] = f32::from(u8::from(me.bandwidth_spare() > 0));
  row[49] = f32::from(u8::from(me.comms_jammed));
  row[50] = f32::from(u8::from(me.get_crew().sensor_operators() >= 2));
  row[51] = f32::from(me.get_crew().get_leadership()).min(6.0) / 6.0;
  row[52] = (orders::missiles_inbound(view) as f32 / 10.0).min(1.0);
}

fn encode_enemy(ctx: &Context, me: &Ship, enemy: &Enemy, row: &mut [f32]) {
  let view = ctx.view;
  let name = me.get_name();
  row[0] = 1.0;
  row[1] = f32::from(u8::from(enemy.contact.is_some()));
  relative(&mut row[2..11], me, enemy.position, enemy.velocity);
  row[11] = band_index(me, enemy.position);
  let seen = ctx.memory.get(enemy.name);
  match enemy.contact {
    Some(contact) => {
      row[12] = share(contact.current_hull, contact.design_hull);
      row[13] = 0.0;
      row[14] = (f64::from(contact.design_hull.max(1)).log10() / 4.0) as f32;
      row[15] = f32::from(contact.current_maneuver) / 10.0;
      row[20] = contact.crit_level.iter().map(|level| f32::from(*level)).sum::<f32>().min(20.0) / 20.0;
      if let Some(table) = view.engagement(name, &contact.name) {
        odds(&mut row[21..31], &table.we_hit, contact.current_hull);
        odds(&mut row[31..41], &table.they_hit, me.current_hull);
      }
      row[41] = contact.held_by.len().min(MAX_SHIPS) as f32 / MAX_SHIPS as f32;
    }
    None => {
      // A blip: only what was known of her when last held, if ever.
      if let Some(seen) = seen {
        row[12] = seen.hull_share;
        row[13] = (ctx.round.saturating_sub(seen.round).min(20) as f32) / 20.0;
        row[14] = (f64::from(seen.design_hull.max(1)).log10() / 4.0) as f32;
        row[15] = f32::from(seen.thrust) / 10.0;
      } else {
        row[13] = 1.0;
      }
    }
  }
  row[16] = f32::from(u8::from(me.contacts.iter().any(|held| held == enemy.name)));
  row[17] = f32::from(u8::from(me.sensor_locks.iter().any(|locked| locked == enemy.name)));
  let watcher = view
    .watchers
    .iter()
    .find(|watcher| watcher.ours == name && watcher.by == enemy.name);
  row[18] = f32::from(u8::from(watcher.is_some()));
  row[19] = f32::from(u8::from(watcher.is_some_and(|watcher| watcher.lock)));
}

/// Which choices of each head are open to `me`.
fn masks(me: &Ship, view: &Observation, friends: &[&Ship], enemies: &[Enemy], mask: &mut [bool]) {
  mask.fill(false);
  let moves = me.max_acceleration() > 0;
  let any_enemy = !enemies.is_empty();
  let guides: Vec<bool> = friends.iter().map(|ship| ship.max_acceleration() > 0).collect();

  let verbs = action::Verb::ALL.map(|verb| match verb {
    action::Verb::Hold => true,
    action::Verb::Intercept | action::Verb::HoldRange | action::Verb::OpenRange => moves && any_enemy,
    action::Verb::Follow => moves && guides.iter().any(|open| *open),
  });
  set(mask, Head::Verb, &verbs);

  let targets: Vec<bool> = (0..ENEMIES).map(|slot| slot < enemies.len()).collect();
  set(mask, Head::HelmTarget, &targets);
  set(mask, Head::Band, &[true; action::HELM_BANDS.len()]);
  let guide_slots: Vec<bool> = (0..FRIENDS).map(|slot| guides.get(slot).copied().unwrap_or(false)).collect();
  set(mask, Head::Guide, &guide_slots);

  // Fire and lock: none, or a ship in contact.
  let contacts: Vec<bool> = std::iter::once(true)
    .chain((0..ENEMIES).map(|slot| enemies.get(slot).is_some_and(|enemy| enemy.contact.is_some())))
    .collect();
  set(mask, Head::Fire, &contacts);
  set(mask, Head::Lock, &contacts);
  set(mask, Head::Posture, &[true; 3]);
  let dodges: Vec<bool> = (0..action::DODGES).map(|g| g <= usize::from(me.max_acceleration())).collect();
  set(mask, Head::Dodge, &dodges);
  set(mask, Head::Assist, &[true, true]);
  set(mask, Head::Jump, &[true, orders::can_withdraw(me)]);

  // Sensor orders: lock and jam comms need a contact, jamming missiles needs
  // missiles coming in. A second operator's heads only if there is one.
  let any_contact = enemies.iter().any(|enemy| enemy.contact.is_some());
  let missiles = orders::missiles_inbound(view) > 0;
  let operators = me.get_crew().sensor_operators();
  let verbs = orders::SENSOR_ORDERS.map(|order| match order {
    orders::SensorOrder::Lock | orders::SensorOrder::JamComms => any_contact,
    orders::SensorOrder::JamMissiles => missiles,
  });
  set(mask, Head::Sensor1, if operators >= 1 { &verbs } else { &[false; 3] });
  set(mask, Head::Sensor2, if operators >= 2 { &verbs } else { &[false; 3] });
  let none_only: Vec<bool> = std::iter::once(true).chain(std::iter::repeat_n(false, ENEMIES)).collect();
  set(mask, Head::Target2, if operators >= 2 { &contacts } else { &none_only });
  set(mask, Head::Captain, &[true; orders::RANKINGS.len()]);
  set(mask, Head::Computer, &[true, true]);
}

/// Open the choices `open` says in `head`, and the first one whatever, so a
/// head always has a choice.
fn set(mask: &mut [bool], head: Head, open: &[bool]) {
  let at = head.offset();
  mask[at..at + head.size()].copy_from_slice(open);
  if !mask[at..at + head.size()].iter().any(|open| *open) {
    mask[at] = true;
  }
}

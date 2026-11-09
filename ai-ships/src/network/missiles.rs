//! Where the side's missiles go: shared out across the whole side, against
//! each target's point defence.
//!
//! A point-defence pool is rolled once a round, and every missile landing
//! that round drains it. So a few missiles at a defended ship are wasted,
//! but enough of them, arriving together, overwhelm it -- and everything past
//! the pool gets through. No one ship can see that, so it is decided here,
//! for the side, after the guns are shared out:
//!
//! 1. Each launcher's options: every enemy it holds, with what one of its
//!    missiles would do there before point defence, and the round it would
//!    arrive.
//! 2. Packages first. For each defended target and arrival round, every
//!    launcher that could arrive then, with the missiles already in flight
//!    that arrive then too. A package is sent only if what it is expected to
//!    get through does more than its launchers would against undefended
//!    targets.
//! 3. Then the rest, one launcher at a time, wherever its missiles do most --
//!    the share expected to get through, up to what the target has left --
//!    and held, not fired, where they would do nothing.
//!
//! Only ships the network has chosen to fire take part: one told to hold
//! fire keeps her launchers quiet too.

use std::collections::HashMap;

use callisto::combat::battery_intercept_dice;
use callisto::entity::{Entity, Vec3, DELTA_TIME_F64, G};
use callisto::ship::{get_ship_template, Ship};
use callisto::tactics::{self, Engage, Intent};
use cgmath::InnerSpace;

use crate::network::action::ENOUGH;
use crate::observation::{Contact, Observation};

/// A missile's acceleration.
const MISSILE_ACCELERATION: f64 = 10.0 * G;
/// Rounds a missile burns before it is spent.
const MISSILE_BURNS: f64 = 10.0;

/// One launcher on one of our ships.
struct Launcher {
  ship: String,
  mount: usize,
  /// Missiles one launch puts in flight.
  missiles: f64,
  /// Pool points each costs.
  cost: f64,
  options: Vec<Choice>,
}

/// Where a launcher could send its salvo.
struct Choice {
  target: String,
  /// Expected damage from the whole salvo before point defence.
  damage: f64,
  round: u32,
}

/// The chance of each total a point-defence pool can roll.
#[derive(Clone)]
struct Pool(Vec<f64>);

impl Pool {
  /// No point defence.
  fn none() -> Self {
    Pool(vec![1.0])
  }

  /// The pool rolled by `dice` six-sided dice.
  fn of_dice(dice: u32) -> Self {
    let chances = (0..dice).fold(vec![1.0], |sums, _| {
      let mut next = vec![0.0; sums.len() + 6];
      for (sum, chance) in sums.iter().enumerate() {
        for face in 1..=6 {
          next[sum + face] += chance / 6.0;
        }
      }
      next
    });
    Pool(chances)
  }

  fn defended(&self) -> bool {
    self.0.len() > 1
  }

  /// Pool points expected to get through when `points` arrive in one round.
  fn leak(&self, points: f64) -> f64 {
    self
      .0
      .iter()
      .enumerate()
      .map(|(pool, chance)| {
        #[allow(clippy::cast_precision_loss)]
        let stopped = pool as f64;
        chance * (points - stopped).max(0.0)
      })
      .sum()
  }
}

/// The point defence a contact's design carries: its batteries, which fire
/// every round with no one at them. Turrets on point defence are not
/// counted -- a turret doing that is not attacking, and the side cannot know
/// which will.
fn pool_of(contact: &Contact) -> Pool {
  let dice: u32 = get_ship_template(&contact.design).map_or(0, |design| {
    design.weapons.iter().filter_map(battery_intercept_dice).map(u32::from).sum()
  });
  if dice == 0 {
    Pool::none()
  } else {
    Pool::of_dice(dice)
  }
}

/// The round, counting this one as 1, that something at `from` moving at
/// `velocity` and burning at missile thrust reaches a target at `to` moving
/// at `target_velocity`. `None` if it would burn out first.
fn arrival(from: Vec3, velocity: Vec3, to: Vec3, target_velocity: Vec3) -> Option<u32> {
  let offset = to - from;
  let distance = offset.magnitude();
  if distance < 1.0 {
    return Some(1);
  }
  let closing = (velocity - target_velocity).dot(offset / distance);
  // distance = closing t + a t^2 / 2
  let time = (-closing + (closing * closing + 2.0 * MISSILE_ACCELERATION * distance).sqrt()) / MISSILE_ACCELERATION;
  let rounds = (time / DELTA_TIME_F64).ceil().max(1.0);
  #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
  (rounds <= MISSILE_BURNS).then_some(rounds as u32)
}

/// What a target has room for: [`ENOUGH`] times its hull, less what the
/// side's guns are already expected to do to it this round.
fn rooms(
  intents: &[(String, Intent)], own: &[&Ship], view: &Observation, launchers: &[Launcher],
) -> HashMap<String, f64> {
  let mut rooms: HashMap<String, f64> = view
    .enemies()
    .iter()
    .map(|contact| (contact.name.clone(), ENOUGH * f64::from(contact.current_hull.max(1))))
    .collect();
  for (name, intent) in intents {
    let Some(ship) = own.iter().find(|ship| ship.get_name() == name) else {
      continue;
    };
    for engage in &intent.fire {
      let Some(contact) = view.enemies().into_iter().find(|contact| contact.name == engage.target) else {
        continue;
      };
      let band = tactics::band_at((contact.position - ship.get_position()).magnitude());
      let guns: f64 = view
        .engagement(name, &engage.target)
        .and_then(|table| table.we_hit.iter().find(|odds| odds.band == band))
        .map_or(0.0, |odds| {
          odds
            .by_mount
            .iter()
            .filter(|(mount, _)| engage.weapons.is_empty() || engage.weapons.contains(mount))
            .filter(|(mount, _)| !launchers.iter().any(|l| l.ship == *name && l.mount == *mount))
            .map(|(_, damage)| damage)
            .sum()
        });
      if let Some(room) = rooms.get_mut(&engage.target) {
        *room = (*room - guns).max(0.0);
      }
    }
  }
  rooms
}

/// The side's launchers, on the ships that are firing, and where each could
/// send its salvo.
fn launchers(intents: &[(String, Intent)], own: &[&Ship], view: &Observation) -> Vec<Launcher> {
  let enemies = view.enemies();
  intents
    .iter()
    .filter(|(_, intent)| !intent.fire.is_empty() && intent.fire.iter().all(|engage| engage.called.is_none()))
    .filter_map(|(name, _)| own.iter().find(|ship| ship.get_name() == name))
    .flat_map(|ship| {
      let enemies = &enemies;
      // Launchers fire at any range, so any band finds them.
      tactics::weapons_that_bear(ship, callisto::ship::Range::Distant)
        .into_iter()
        .filter_map(move |(mount, kind)| {
          let (missiles, cost) = tactics::launch_size(&ship.weapons()[mount], kind)?;
          let options = enemies
            .iter()
            .filter(|contact| ship.contacts.contains(&contact.name))
            .filter_map(|contact| {
              let band = tactics::band_at((contact.position - ship.get_position()).magnitude());
              let damage = view
                .engagement(ship.get_name(), &contact.name)
                .and_then(|table| table.we_hit.iter().find(|odds| odds.band == band))
                .and_then(|odds| odds.by_mount.iter().find(|(m, _)| *m == mount).map(|(_, damage)| *damage))
                .filter(|damage| *damage > 0.0)?;
              let round = arrival(ship.get_position(), ship.get_velocity(), contact.position, contact.velocity)?;
              Some(Choice {
                target: contact.name.clone(),
                damage,
                round,
              })
            })
            .collect::<Vec<_>>();
          Some(Launcher {
            ship: ship.get_name().to_string(),
            mount,
            missiles: f64::from(missiles),
            cost: f64::from(cost),
            options,
          })
        })
    })
    .collect()
}

/// Pool points already on their way to each target, by the round they land.
fn in_flight(own: &[&Ship], view: &Observation) -> HashMap<(String, u32), f64> {
  let ours: Vec<&str> = own.iter().map(|ship| ship.get_name()).collect();
  let mut arriving: HashMap<(String, u32), f64> = HashMap::new();
  for missile in &view.missiles {
    if !ours.contains(&missile.source.as_str()) {
      continue;
    }
    let Some(target) = view.enemies().into_iter().find(|contact| contact.name == missile.target) else {
      continue;
    };
    if let Some(round) = arrival(missile.get_position(), missile.get_velocity(), target.position, target.velocity) {
      let cost = if missile.weapon.has_kind(callisto::ship::WeaponType::Torpedo) {
        2.0
      } else {
        1.0
      };
      // A salvo is one object with its missiles counted in it; each costs
      // the pool as it always did.
      *arriving.entry((target.name.clone(), round)).or_default() += cost * f64::from(missile.count);
    }
  }
  arriving
}

/// Rewrite each firing ship's orders: her guns as they were, her launchers
/// where they were assigned, or silent.
fn rewrite(
  intents: &mut [(String, Intent)], own: &[&Ship], launchers: &[Launcher], assigned: &HashMap<(String, usize), String>,
) {
  for (name, intent) in intents.iter_mut() {
    let mounts: Vec<usize> = launchers.iter().filter(|l| l.ship == *name).map(|l| l.mount).collect();
    if mounts.is_empty() {
      continue;
    }
    let Some(ship) = own.iter().find(|ship| ship.get_name() == name) else {
      continue;
    };
    let guns: Vec<usize> = (0..ship.weapons().len()).filter(|mount| !mounts.contains(mount)).collect();
    let mut fire: Vec<Engage> = intent
      .fire
      .drain(..)
      .filter_map(|mut engage| {
        engage.weapons = if engage.weapons.is_empty() {
          guns.clone()
        } else {
          engage.weapons.into_iter().filter(|mount| guns.contains(mount)).collect()
        };
        (!engage.weapons.is_empty()).then_some(engage)
      })
      .collect();
    for mount in mounts {
      let Some(target) = assigned.get(&(name.clone(), mount)) else {
        continue;
      };
      match fire.iter_mut().find(|engage| engage.target == *target) {
        Some(engage) => engage.weapons.push(mount),
        None => fire.push(Engage {
          target: target.clone(),
          weapons: vec![mount],
          ..Engage::default()
        }),
      }
    }
    intent.fire = fire;
  }
}

/// Share the side's launchers out across its targets: see the module notes.
pub fn share_missiles(intents: &mut [(String, Intent)], own: &[&Ship], view: &Observation) {
  let launchers = launchers(intents, own, view);
  if launchers.is_empty() {
    return;
  }
  let pools: HashMap<String, Pool> = view.enemies().into_iter().map(|c| (c.name.clone(), pool_of(c))).collect();
  let pool = |target: &str| pools.get(target).cloned().unwrap_or_else(Pool::none);
  let mut rooms = rooms(intents, own, view, &launchers);
  let mut arriving = in_flight(own, view);

  // What a salvo is expected to do, sent to `choice` with `points` already
  // landing that round: its share of what gets through, up to the target's
  // room.
  let worth = |launcher: &Launcher, choice: &Choice, points: f64, rooms: &HashMap<String, f64>| {
    let added = launcher.missiles * launcher.cost;
    let pool = pool(&choice.target);
    let through = (pool.leak(points + added) - pool.leak(points)) / added;
    (choice.damage * through).min(rooms.get(&choice.target).copied().unwrap_or(0.0))
  };
  let mut assigned: HashMap<(String, usize), String> = HashMap::new();
  let mut free: Vec<usize> = (0..launchers.len()).collect();

  // Packages: commit every launcher that can arrive together at a defended
  // target, when that beats sending each to her best undefended one.
  loop {
    let undefended = |index: usize, rooms: &HashMap<String, f64>| {
      launchers[index]
        .options
        .iter()
        .filter(|choice| !pool(&choice.target).defended())
        .map(|choice| worth(&launchers[index], choice, 0.0, rooms))
        .fold(0.0, f64::max)
    };
    let mut best: Option<(f64, String, u32, Vec<usize>, f64)> = None;
    let mut targets: Vec<(String, u32)> = free
      .iter()
      .flat_map(|index| launchers[*index].options.iter())
      .filter(|choice| pool(&choice.target).defended())
      .map(|choice| (choice.target.clone(), choice.round))
      .collect();
    targets.sort();
    targets.dedup();
    for (target, round) in targets {
      let members: Vec<usize> = free
        .iter()
        .copied()
        .filter(|index| {
          launchers[*index]
            .options
            .iter()
            .any(|choice| choice.target == target && choice.round == round)
        })
        .collect();
      let landing = arriving.get(&(target.clone(), round)).copied().unwrap_or(0.0);
      let added: f64 = members.iter().map(|i| launchers[*i].missiles * launchers[*i].cost).sum();
      let damage: f64 = members
        .iter()
        .filter_map(|i| launchers[*i].options.iter().find(|c| c.target == target).map(|c| c.damage))
        .sum();
      let pool = pool(&target);
      let through = if added > 0.0 {
        (pool.leak(landing + added) - pool.leak(landing)) / added
      } else {
        0.0
      };
      let value = (damage * through).min(rooms.get(&target).copied().unwrap_or(0.0));
      let instead: f64 = members.iter().map(|i| undefended(*i, &rooms)).sum();
      let gain = value - instead;
      if gain > 0.0 && best.as_ref().is_none_or(|b| gain > b.0) {
        best = Some((gain, target, round, members, value));
      }
    }
    let Some((_, target, round, members, value)) = best else {
      break;
    };
    for index in &members {
      let launcher = &launchers[*index];
      assigned.insert((launcher.ship.clone(), launcher.mount), target.clone());
      *arriving.entry((target.clone(), round)).or_default() += launcher.missiles * launcher.cost;
    }
    if let Some(room) = rooms.get_mut(&target) {
      *room = (*room - value).max(0.0);
    }
    free.retain(|index| !members.contains(index));
  }

  // The rest, one at a time, wherever each does most; held where nothing is
  // worth a salvo.
  for index in free {
    let launcher = &launchers[index];
    let best = launcher
      .options
      .iter()
      .map(|choice| {
        let landing = arriving.get(&(choice.target.clone(), choice.round)).copied().unwrap_or(0.0);
        (worth(launcher, choice, landing, &rooms), choice)
      })
      .filter(|(value, _)| *value > 0.05)
      .max_by(|a, b| a.0.total_cmp(&b.0));
    if let Some((value, choice)) = best {
      assigned.insert((launcher.ship.clone(), launcher.mount), choice.target.clone());
      *arriving.entry((choice.target.clone(), choice.round)).or_default() += launcher.missiles * launcher.cost;
      if let Some(room) = rooms.get_mut(&choice.target) {
        *room = (*room - value).max(0.0);
      }
    }
  }

  rewrite(intents, own, &launchers, &assigned);
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn with_no_point_defence_everything_gets_through() {
    assert!((Pool::none().leak(6.0) - 6.0).abs() < 1e-9);
  }

  #[test]
  fn a_pool_stops_what_it_rolls() {
    // One die against six: 6 - 3.5 get through on average.
    assert!((Pool::of_dice(1).leak(6.0) - 2.5).abs() < 1e-9);
    // Six dice (a Type III battery) against six: hardly any.
    assert!(Pool::of_dice(6).leak(6.0) < 1e-3);
    // Against thirty-six, most of what is over the pool's average of 21.
    assert!((Pool::of_dice(6).leak(36.0) - 15.0).abs() < 0.1);
  }

  #[test]
  fn a_close_target_is_reached_this_round_and_a_far_one_later() {
    let at_rest = Vec3::new(0.0, 0.0, 0.0);
    assert_eq!(arrival(at_rest, at_rest, Vec3::new(1_000_000.0, 0.0, 0.0), at_rest), Some(1));
    assert_eq!(arrival(at_rest, at_rest, Vec3::new(20_000_000.0, 0.0, 0.0), at_rest), Some(2));
    // Past what ten rounds of burn can reach.
    assert_eq!(arrival(at_rest, at_rest, Vec3::new(1e12, 0.0, 0.0), at_rest), None);
  }
}

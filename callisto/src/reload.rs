//! Reloading a launcher or sandcaster mount from the ship's stores.
//!
//! "It takes one round to reload a missile rack" (Core Rulebook p. 168), and a
//! sandcaster the same. A turret "may be reloaded by any Traveller with the
//! Gunner skill but it will be unable to make any attacks in that round"
//! (p. 172). Callisto lets a gunner beyond the mounts do it for free: see
//! [`assign`].

use std::fmt::Write as _;

use crate::action::ShipAction;
use crate::entity::Entity;
use crate::payloads::{EffectMsg, MessageCategory};
use crate::ship::Ship;

/// A Reload the crew will carry out this round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reload {
  pub weapon_id: usize,
  /// Done by a spare gunner, so the mount still fires (or defends) with its
  /// own. `false` when the mount's own gunner does it instead, which costs
  /// the mount its attack and its reaction this round.
  pub spare: bool,
}

/// The Reloads `ship`'s crew will carry out, in the order they were chosen.
///
/// Each takes a spare gunner while any are free; the first that finds none
/// left, and every one after it, is done by the mount's own gunner. A Reload
/// that would put nothing in the mount -- one with no launcher or sandcaster,
/// one already full, or one whose stores are out of what it is missing -- is
/// not carried out and takes no one, and gets a line saying why.
#[must_use]
pub fn assign(ship: &Ship, actions: &[ShipAction]) -> (Vec<Reload>, Vec<EffectMsg>) {
  let mut spares = ship.spare_gunners();
  let mut reloads: Vec<Reload> = Vec::new();
  let mut refused = Vec::new();
  for weapon_id in actions.iter().filter_map(|action| match action {
    ShipAction::ReloadAction { weapon_id } => Some(*weapon_id),
    _ => None,
  }) {
    if weapon_id >= ship.weapons().len() || reloads.iter().any(|reload| reload.weapon_id == weapon_id) {
      continue;
    }
    if !ship.can_reload(weapon_id) {
      let why = if ship.capacity(weapon_id).is_empty() {
        "holds nothing to reload"
      } else if ship.capacity(weapon_id) == ship.loaded(weapon_id) {
        "is already full"
      } else {
        "cannot be reloaded: stores are empty"
      };
      refused.push(EffectMsg::about(
        ship.get_name(),
        MessageCategory::Attack,
        format!("%s's {} {why}.", mount_name(ship, weapon_id)),
      ));
      continue;
    }
    let spare = spares > 0;
    spares -= usize::from(spare);
    reloads.push(Reload { weapon_id, spare });
  }
  (reloads, refused)
}

/// The mounts whose own gunners are reloading, and so do nothing else.
#[must_use]
pub fn busy(reloads: &[Reload]) -> Vec<usize> {
  reloads
    .iter()
    .filter(|reload| !reload.spare)
    .map(|reload| reload.weapon_id)
    .collect()
}

/// Refill each mount from stores, up to what it holds, in order: an earlier
/// Reload gets what stores have first. A line per Reload.
pub fn apply(ship: &mut Ship, reloads: &[Reload]) -> Vec<EffectMsg> {
  reloads
    .iter()
    .map(|reload| {
      let weapon_id = reload.weapon_id;
      let capacity = ship.capacity(weapon_id);
      let before = ship.loaded(weapon_id);
      let taken = capacity.saturating_sub(before).min(ship.magazine);
      let after = before.plus(taken);
      ship.magazine = ship.magazine.saturating_sub(taken);
      if let Some(load) = ship.loaded.get_mut(weapon_id) {
        *load = after;
      }
      let mut line = format!("%s reloads {}", mount_name(ship, weapon_id));
      if reload.spare {
        line.push_str(" with a spare gunner");
      }
      let counts: Vec<String> = [
        ("missiles", capacity.missiles, after.missiles, ship.magazine.missiles),
        ("torpedoes", capacity.torpedoes, after.torpedoes, ship.magazine.torpedoes),
        ("sand canisters", capacity.sand, after.sand, ship.magazine.sand),
      ]
      .into_iter()
      .filter(|(_, holds, ..)| *holds > 0)
      .map(|(what, holds, now, stores)| format!("{now} of {holds} {what} ({stores} left in stores)"))
      .collect();
      let _ = write!(line, ": {}.", counts.join("; "));
      if !reload.spare {
        line
          .push_str(" No spare gunner was free, so its own gunner reloaded and the mount made no attack or reaction.");
      }
      EffectMsg::about(ship.get_name(), MessageCategory::Attack, line)
    })
    .collect()
}

/// "mount 2, missile triple turret", as the results log names a mount.
fn mount_name(ship: &Ship, weapon_id: usize) -> String {
  format!("mount {} ({})", weapon_id + 1, String::from(ship.get_weapon(weapon_id)))
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use cgmath::Zero;

  use super::*;
  use crate::crew::Crew;
  use crate::entity::Vec3;
  use crate::ship::{Magazine, ShipDesignTemplate, Weapon, WeaponMount, WeaponType};

  fn rack() -> Weapon {
    Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 1)
  }

  /// A 400-ton ship with these mounts, `stores` missiles beyond what they
  /// hold, and `spares` gunners more than it has mounts.
  fn ship(weapons: Vec<Weapon>, stores: u32, spares: usize) -> Ship {
    let mounts = weapons.len();
    let design = Arc::new(ShipDesignTemplate {
      name: "Reloader".to_string(),
      displacement: 400,
      weapons,
      ..ShipDesignTemplate::default()
    });
    let mut crew = Crew::new();
    (0..mounts + spares).for_each(|_| crew.add_gunnery(1));
    let mut ship = Ship::new("Reloader".to_string(), Vec3::zero(), Vec3::zero(), &design, Some(crew), None);
    ship.magazine.missiles = stores;
    ship
  }

  fn reloads(weapon_ids: &[usize]) -> Vec<ShipAction> {
    weapon_ids
      .iter()
      .map(|weapon_id| ShipAction::ReloadAction { weapon_id: *weapon_id })
      .collect()
  }

  /// A rack fires twelve and is empty; one round's reload fills it.
  #[test]
  fn an_empty_rack_refills_in_one_reload() {
    let mut ship = ship(vec![rack()], 30, 0);
    ship.loaded[0].missiles = 0;
    let (plan, refused) = assign(&ship, &reloads(&[0]));
    assert!(refused.is_empty(), "{refused:?}");
    assert_eq!(
      plan,
      vec![Reload {
        weapon_id: 0,
        spare: false
      }]
    );
    let lines = apply(&mut ship, &plan);
    assert_eq!(ship.loaded(0).missiles, 12);
    assert_eq!(ship.magazine.missiles, 18);
    let line = lines[0].rendered();
    assert!(line.contains("12 of 12 missiles (18 left in stores)"), "{line}");
    assert!(line.contains("made no attack"), "{line}");
  }

  /// Spares go to Reloads in the order they were chosen; the rest are done
  /// by the mounts' own gunners.
  #[test]
  fn spares_go_to_reloads_in_the_order_chosen() {
    let mut ship = ship(vec![rack(), rack(), rack()], 100, 2);
    ship.loaded.iter_mut().for_each(|load| load.missiles = 1);
    let (plan, _) = assign(&ship, &reloads(&[2, 0, 1]));
    assert_eq!(
      plan,
      vec![
        Reload {
          weapon_id: 2,
          spare: true
        },
        Reload {
          weapon_id: 0,
          spare: true
        },
        Reload {
          weapon_id: 1,
          spare: false
        },
      ]
    );
    assert_eq!(busy(&plan), vec![1]);
    // Withdrawn, the first Reload gives its spare back to the next.
    let (plan, _) = assign(&ship, &reloads(&[0, 1]));
    assert!(plan.iter().all(|reload| reload.spare));
  }

  /// A Reload that would put nothing in the mount is not carried out and
  /// takes no one: a full mount, a laser, or stores out of what it needs.
  #[test]
  fn a_reload_with_nothing_to_do_takes_no_one() {
    let laser = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut ship = ship(vec![rack(), laser, rack()], 5, 1);
    ship.loaded[2].missiles = 0;
    let (plan, refused) = assign(&ship, &reloads(&[0, 1, 2]));
    assert_eq!(
      plan,
      vec![Reload {
        weapon_id: 2,
        spare: true
      }],
      "the spare is still free for mount 3"
    );
    let lines: Vec<String> = refused.iter().map(EffectMsg::rendered).collect();
    assert!(lines[0].contains("already full"), "{lines:?}");
    assert!(lines[1].contains("holds nothing to reload"), "{lines:?}");

    // Stores give what they have, and are then out.
    apply(&mut ship, &plan);
    assert_eq!((ship.loaded(2).missiles, ship.magazine.missiles), (5, 0));
    ship.loaded[2].missiles = 0;
    let (plan, refused) = assign(&ship, &reloads(&[2]));
    assert!(plan.is_empty(), "{plan:?}");
    assert!(refused[0].rendered().contains("stores are empty"));
  }

  /// A battery is one mount to its gunner: one Reload refills all of it.
  #[test]
  fn a_battery_reloads_as_one() {
    let mut ship = ship(
      vec![Weapon::uniform(WeaponType::Missile, WeaponMount::Turret, 3).times(4)],
      100,
      0,
    );
    assert_eq!(ship.loaded(0).missiles, 48, "four turrets' twelve");
    ship.loaded[0].missiles = 10;
    let (plan, _) = assign(&ship, &reloads(&[0]));
    apply(&mut ship, &plan);
    assert_eq!((ship.loaded(0).missiles, ship.magazine.missiles), (48, 62));
  }

  /// A mixed turret's rack and sandcaster are refilled together.
  #[test]
  fn a_mixed_turret_refills_every_kind() {
    let mixed = Weapon {
      mount: WeaponMount::Turret,
      guns: vec![
        crate::ship::Gun::new(WeaponType::Missile),
        crate::ship::Gun::new(WeaponType::Sand),
      ],
      count: 1,
    };
    let mut ship = ship(vec![mixed], 20, 1);
    ship.magazine.sand = 30;
    ship.loaded[0] = Magazine {
      missiles: 2,
      torpedoes: 0,
      sand: 5,
      rounds: 0,
    };
    let (plan, _) = assign(&ship, &reloads(&[0]));
    let line = apply(&mut ship, &plan)[0].rendered();
    assert_eq!(
      ship.loaded(0),
      Magazine {
        missiles: 12,
        torpedoes: 0,
        sand: 20,
        rounds: 0
      }
    );
    assert_eq!(
      ship.magazine,
      Magazine {
        missiles: 10,
        torpedoes: 0,
        sand: 15,
        rounds: 0
      }
    );
    assert!(
      line.contains("12 of 12 missiles") && line.contains("20 of 20 sand canisters"),
      "{line}"
    );
    assert!(line.contains("spare gunner"), "{line}");
  }
}

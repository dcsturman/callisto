//! A round's results added up.
//!
//! A fight between two fleets puts hundreds of lines in a round's results,
//! and what a player needs from them first is short: what was lost and to
//! whom, who is hurt and how badly, and how each side's fire went. Each ship
//! notes what was done to her as it happens (`Ship::taken`); this reads those
//! notes against the ships as the round began.

use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::entity::{Entities, Entity};
use crate::payloads::{Damaged, Loss, RoundSummary, ShipRef, SideFire};
use crate::ship::{Ship, Taken, Team};

/// The round just played, from `snapshot` (the ships as it began) and
/// `entities` (the world after it, with this round's `wrecks`).
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn round_summary<S: BuildHasher>(snapshot: &HashMap<String, Ship, S>, entities: &Entities) -> RoundSummary {
  let named = |name: &str| ShipRef {
    name: name.to_string(),
    team: snapshot.get(name).and_then(|ship| ship.team),
  };
  // Who took the most hull off a ship, criticals breaking a tie, and then the
  // name, so the same round always reads the same.
  let ranked = |ship: &Ship| {
    let mut by = ship.taken.iter().filter(|(_, taken)| taken.hits > 0).collect::<Vec<_>>();
    by.sort_by(|(a_name, a), (b_name, b)| {
      (b.hull, b.criticals)
        .cmp(&(a.hull, a.criticals))
        .then_with(|| a_name.cmp(b_name))
    });
    by.into_iter().map(|(name, _)| named(name)).collect::<Vec<_>>()
  };

  let mut losses = Vec::new();
  for wreck in &entities.wrecks {
    let by = ranked(wreck).into_iter().next();
    for craft in &wreck.embarked {
      losses.push(Loss {
        ship: ShipRef {
          name: craft.get_name().to_string(),
          team: craft.team.or(wreck.team),
        },
        by: by.clone(),
        aboard: Some(wreck.get_name().to_string()),
        squadron: craft.squadron.clone(),
        tons: craft.design.displacement,
      });
    }
    losses.push(Loss {
      ship: named(wreck.get_name()),
      by,
      aboard: None,
      squadron: wreck.squadron.clone(),
      tons: wreck.design.displacement,
    });
  }
  losses.sort_by(|a, b| a.ship.name.cmp(&b.ship.name));

  let mut damaged = entities
    .ships
    .values()
    .filter_map(|ship| {
      let ship = ship.read().unwrap();
      let before = snapshot.get(ship.get_name())?;
      let hull = ship.get_current_hull_points();
      let hull_lost = before.get_current_hull_points().saturating_sub(hull);
      let criticals = ship.taken.values().map(|taken| taken.criticals).sum();
      (hull_lost > 0 || criticals > 0).then(|| Damaged {
        ship: named(ship.get_name()),
        hull_lost,
        hull,
        max_hull: ship.get_max_hull_points(),
        criticals,
        by: ranked(&ship),
        squadron: ship.squadron.clone(),
        tons: ship.design.displacement,
      })
    })
    .collect::<Vec<_>>();
  // The worst hurt first: the share of her hull each lost this round.
  damaged.sort_by(|a, b| {
    (u64::from(b.hull_lost) * u64::from(a.max_hull.max(1)))
      .cmp(&(u64::from(a.hull_lost) * u64::from(b.max_hull.max(1))))
      .then_with(|| b.criticals.cmp(&a.criticals))
      .then_with(|| a.ship.name.cmp(&b.ship.name))
  });

  // Every note on every ship, live or lost, is one attacker's fire at her.
  let mut fire: Vec<SideFire> = Vec::new();
  let live = entities
    .ships
    .values()
    .map(|ship| ship.read().unwrap().taken.clone())
    .collect::<Vec<_>>();
  for (attacker, taken) in live.iter().chain(entities.wrecks.iter().map(|wreck| &wreck.taken)).flatten() {
    let Taken {
      attacks,
      hits,
      hull,
      criticals,
      intercepted,
    } = taken;
    let side = side_of(&mut fire, snapshot.get(attacker).and_then(|ship| ship.team));
    side.attacks += attacks;
    side.hits += hits;
    side.hull += hull;
    side.criticals += criticals;
    side.intercepted += intercepted;
  }
  for loss in losses.iter().filter(|loss| loss.aboard.is_none()) {
    if let Some(by) = &loss.by {
      side_of(&mut fire, by.team).kills += 1;
    }
  }
  fire.sort_by_key(|side| side.team.map(|team| format!("{team:?}")));

  RoundSummary { losses, damaged, fire }
}

/// A side's line in `fire`, added when it has none yet.
fn side_of(fire: &mut Vec<SideFire>, team: Option<Team>) -> &mut SideFire {
  let index = fire.iter().position(|side| side.team == team).unwrap_or_else(|| {
    fire.push(SideFire {
      team,
      ..SideFire::default()
    });
    fire.len() - 1
  });
  &mut fire[index]
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::action::BoostMap;
  use crate::combat::{attack, HitMods};
  use crate::entity::Vec3;
  use crate::ship::{ShipDesignTemplate, Weapon, WeaponMount, WeaponType};
  use cgmath::Zero;
  use rand::rngs::StdRng;
  use rand::SeedableRng;
  use std::sync::{Arc, RwLock};

  fn ship(name: &str, team: Team, hull: u32) -> Ship {
    let design = Arc::new(ShipDesignTemplate {
      name: "Design".to_string(),
      weapons: vec![Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1)],
      hull,
      displacement: 400,
      ..ShipDesignTemplate::default()
    });
    let mut ship = Ship::new(name.to_string(), Vec3::zero(), Vec3::zero(), &design, None, None);
    ship.team = Some(team);
    ship
  }

  /// Every attack that rolls is noted on the ship it was at, and every hit
  /// with the hull she lost to it.
  #[test]
  fn attacks_are_noted_on_the_ship_they_were_at() {
    let attacker = ship("Hunter", Team::Blue, 200);
    let mut defender = ship("Prey", Team::Red, 1_000);
    defender.set_position(Vec3::new(1_000.0, 0.0, 0.0));
    let weapon = Weapon::uniform(WeaponType::Beam, WeaponMount::Turret, 1);
    let mut rng = StdRng::seed_from_u64(3);
    for _ in 0..30 {
      attack(
        HitMods::default(),
        0,
        &attacker,
        &mut defender,
        &weapon.firing_default().unwrap(),
        None,
        &BoostMap::default(),
        &mut rng,
      );
    }
    let taken = &defender.taken["Hunter"];
    assert_eq!(taken.attacks, 30);
    assert!(taken.hits > 0 && taken.hits < 30, "{taken:?}");
    assert_eq!(taken.hull, 1_000 - defender.get_current_hull_points());
  }

  /// A round adds up to who was lost and to whom (a carrier's craft going
  /// with her), who was hurt, and each side's fire.
  #[test]
  fn a_round_adds_up() {
    let hunter = ship("Hunter", Team::Blue, 200);
    let mut prey = ship("Prey", Team::Red, 100);
    let mut carrier = ship("Carrier", Team::Red, 100);
    carrier.embarked.push(ship("Fighter", Team::Red, 4));
    let snapshot: HashMap<String, Ship> = [&hunter, &prey, &carrier]
      .into_iter()
      .map(|ship| (ship.get_name().to_string(), ship.clone()))
      .collect();

    prey.set_hull_points(70);
    prey.taken.insert(
      "Hunter".to_string(),
      Taken {
        attacks: 4,
        hits: 2,
        hull: 30,
        criticals: 1,
        intercepted: 3,
      },
    );
    carrier.set_hull_points(0);
    for (by, hull) in [("Hunter", 80), ("Prey", 20)] {
      carrier.taken.insert(
        by.to_string(),
        Taken {
          attacks: 1,
          hits: 1,
          hull,
          ..Taken::default()
        },
      );
    }
    let mut entities = Entities::new();
    for ship in [hunter, prey] {
      entities.ships.insert(ship.get_name().to_string(), Arc::new(RwLock::new(ship)));
    }
    entities.wrecks.push(carrier);

    let summary = round_summary(&snapshot, &entities);
    let lost = summary
      .losses
      .iter()
      .map(|loss| (loss.ship.name.as_str(), loss.aboard.as_deref()))
      .collect::<Vec<_>>();
    assert_eq!(lost, vec![("Carrier", None), ("Fighter", Some("Carrier"))]);
    assert!(summary
      .losses
      .iter()
      .all(|loss| loss.by.as_ref().map(|by| by.name.as_str()) == Some("Hunter")));

    assert_eq!(summary.damaged.len(), 1);
    let hurt = &summary.damaged[0];
    assert_eq!(
      (hurt.ship.name.as_str(), hurt.hull_lost, hurt.hull, hurt.criticals),
      ("Prey", 30, 70, 1)
    );
    assert_eq!(hurt.by.iter().map(|by| by.name.as_str()).collect::<Vec<_>>(), vec!["Hunter"]);

    let blue = summary.fire.iter().find(|side| side.team == Some(Team::Blue)).unwrap();
    assert_eq!(
      (blue.attacks, blue.hits, blue.hull, blue.criticals, blue.intercepted, blue.kills),
      (5, 3, 110, 1, 3, 1)
    );
    let red = summary.fire.iter().find(|side| side.team == Some(Team::Red)).unwrap();
    assert_eq!((red.attacks, red.hits, red.hull, red.kills), (1, 1, 20, 0));
  }
}

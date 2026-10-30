//! A ship's choices as numbers, one per head, and how they become orders.
//!
//! Each head is an independent discrete choice. Targets and guides are slots
//! in the [`Picture`] the ship's observation was encoded with, so slot 3 is
//! whichever enemy was fourth nearest when she looked.
//!
//! The network's choices are the decisions; how each is carried out, when
//! it takes more than one step, is [`crate::network::orders`]: a withdrawal by jump,
//! the captain's boosts from a ranking, the computer's loadout, the sensor
//! operators' verbs. Engineers repair in [`REPAIR_PRIORITY`] when nothing
//! else needs them; point defence is left to the tactics layer.

use std::collections::{BTreeMap, HashMap};

use crate::decision::{straggler_margin, STRAGGLER_STEP};
use crate::observation::Observation;
use callisto::entity::Entity;
use callisto::ship::{Range, Ship};
use callisto::tactics::{self, Engage, EngineerVerb, Flight, Helm, Intent, Posture, SensorVerb};

use crate::network::orders;
use cgmath::InnerSpace;

use crate::network::encode::{posture_of, Picture, ENEMIES, FRIENDS};

/// The helm's verbs, in the order of the verb head.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
  Hold,
  Intercept,
  HoldRange,
  OpenRange,
  Follow,
}

impl Verb {
  pub const ALL: [Verb; 5] = [
    Verb::Hold,
    Verb::Intercept,
    Verb::HoldRange,
    Verb::OpenRange,
    Verb::Follow,
  ];
}

/// The bands a helm may aim for. Distant is not one: nothing reaches it.
pub const HELM_BANDS: [Range; 4] = [Range::Short, Range::Medium, Range::Long, Range::VeryLong];

/// Dodge reserve choices: 0, 1 or 2 G.
pub const DODGES: usize = 3;

const POSTURES: [Posture; 3] = [Posture::Silent, Posture::Passive, Posture::Active];

/// The action heads, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Head {
  Verb,
  HelmTarget,
  Band,
  Guide,
  Fire,
  Lock,
  Posture,
  Dodge,
  Assist,
  Jump,
  /// The first sensor operator's verb; her target is the lock head's.
  Sensor1,
  /// The second sensor operator's verb and target, if there is one.
  Sensor2,
  Target2,
  /// The captain's ranking of guns, defence and detection.
  Captain,
  /// Fight or link.
  Computer,
  /// Flight operations: none, launch (a carrier sends out her ready craft,
  /// the tubes' worth a round) or land (a craft goes home to her carrier).
  Flight,
}

/// The flight head's choices, in order.
pub const FLIGHTS: usize = 3;

impl Head {
  pub const ALL: [Head; 16] = [
    Head::Verb,
    Head::HelmTarget,
    Head::Band,
    Head::Guide,
    Head::Fire,
    Head::Lock,
    Head::Posture,
    Head::Dodge,
    Head::Assist,
    Head::Jump,
    Head::Sensor1,
    Head::Sensor2,
    Head::Target2,
    Head::Captain,
    Head::Computer,
    Head::Flight,
  ];

  /// How many choices the head has.
  #[must_use]
  pub const fn size(self) -> usize {
    match self {
      Head::Verb => Verb::ALL.len(),
      Head::HelmTarget => ENEMIES,
      Head::Band => HELM_BANDS.len(),
      Head::Guide => FRIENDS,
      Head::Fire | Head::Lock | Head::Target2 => ENEMIES + 1,
      Head::Posture => POSTURES.len(),
      Head::Dodge => DODGES,
      Head::Assist | Head::Jump | Head::Computer => 2,
      Head::Sensor1 | Head::Sensor2 => orders::SENSOR_ORDERS.len(),
      Head::Captain => orders::RANKINGS.len(),
      Head::Flight => FLIGHTS,
    }
  }

  /// Where the head's choices start in a ship's mask.
  #[must_use]
  pub const fn offset(self) -> usize {
    let mut at = 0;
    let mut i = 0;
    while i < self as usize {
      at += Head::ALL[i].size();
      i += 1;
    }
    at
  }
}

/// The number of heads.
pub const HEADS: usize = Head::ALL.len();
/// One ship's mask: every head's choices, end to end.
pub const MASK_LEN: usize = Head::Flight.offset() + Head::Flight.size();
/// Each head's size, for the Python side's action space.
pub const HEAD_SIZES: [usize; HEADS] = {
  let mut sizes = [0; HEADS];
  let mut i = 0;
  while i < HEADS {
    sizes[i] = Head::ALL[i].size();
    i += 1;
  }
  sizes
};

/// One ship's choices, one number per head.
pub type Choice = [i64; HEADS];

fn pick(names: &[String], slot: i64) -> Option<&str> {
  usize::try_from(slot).ok().and_then(|slot| names.get(slot)).map(String::as_str)
}

/// A slot for "none, or one of these": 0 is none, `n + 1` is slot `n`.
fn pick_or_none(names: &[String], choice: i64) -> Option<&str> {
  pick(names, choice - 1)
}

/// The helm a ship's choice asks for, and the guide she would follow.
fn helm_of(choice: &Choice, picture: &Picture) -> (Helm, Option<String>) {
  let verb = usize::try_from(choice[Head::Verb as usize])
    .ok()
    .and_then(|verb| Verb::ALL.get(verb))
    .copied()
    .unwrap_or(Verb::Hold);
  let target = pick(&picture.enemies, choice[Head::HelmTarget as usize]).map(str::to_string);
  let band = usize::try_from(choice[Head::Band as usize])
    .ok()
    .and_then(|band| HELM_BANDS.get(band))
    .copied()
    .unwrap_or(Range::Long);
  match (verb, target) {
    (Verb::Intercept, Some(target)) => (Helm::Intercept { target, band }, None),
    (Verb::HoldRange, Some(target)) => (Helm::HoldRange { target, band }, None),
    (Verb::OpenRange, Some(target)) => (Helm::OpenRange { target }, None),
    (Verb::Follow, _) => (
      Helm::Hold,
      pick(&picture.friends, choice[Head::Guide as usize]).map(str::to_string),
    ),
    _ => (Helm::Hold, None),
  }
}

/// A ship's orders from her choice, the helm aside: fire, lock, posture,
/// dodge, assist, and the standing repair order.
fn intent_of(choice: &Choice, picture: &Picture, helm: Helm, ship: &Ship, view: &Observation) -> Intent {
  let fire = pick_or_none(&picture.enemies, choice[Head::Fire as usize]);
  let index = |head: Head| usize::try_from(choice[head as usize]).unwrap_or(0).min(head.size() - 1);
  let leaving = index(Head::Jump) == 1 && orders::can_withdraw(ship);
  let evade = u8::try_from(index(Head::Dodge)).ok();

  // One verb per sensor operator: the first's target is the lock head's,
  // the second's its own.
  let operators = ship.get_crew().sensor_operators();
  let sensors: Vec<SensorVerb> = [
    (Head::Sensor1, pick_or_none(&picture.enemies, choice[Head::Lock as usize])),
    (Head::Sensor2, pick_or_none(&picture.enemies, choice[Head::Target2 as usize])),
  ]
  .into_iter()
  .take(operators)
  .filter_map(|(head, target)| orders::sensor_verb(orders::SENSOR_ORDERS[index(head)], target))
  .collect();

  let computer = if index(Head::Computer) == 1 {
    orders::Computer::Link
  } else {
    orders::Computer::Fight
  };
  let (helm, engineering) = if leaving {
    let withdrawal = orders::withdraw(ship, view);
    (withdrawal.helm.unwrap_or(helm), withdrawal.engineering)
  } else {
    (helm, orders::restore(ship))
  };
  let ranking = orders::RANKINGS[index(Head::Captain)];
  Intent {
    posture: Some(POSTURES[index(Head::Posture)]),
    flight: flight_of(index(Head::Flight), ship),
    helm,
    evade,
    assist_gunners: Some(index(Head::Assist) == 1),
    fire: fire
      .map(|target| Engage {
        target: target.to_string(),
        ..Engage::default()
      })
      .into_iter()
      .collect(),
    sensors,
    engineering,
    software: orders::software_orders(ship, computer, leaving),
    boosts: orders::captain_boosts(ship, view, ranking, evade.is_some_and(|g| g > 0)),
    ..Intent::default()
  }
}

/// The flight operation a ship's choice asks for. "none" leaves the deck to
/// the crews' own rules (`tactics`: launch when an enemy is in reach, come
/// home when spent), so a network widened with nothing learned for this head
/// plays exactly as the crews do; "launch" and "land" are what training
/// learns to choose over them. Where a choice does not apply -- launch with
/// nothing aboard, land with no carrier -- it is "none".
fn flight_of(choice: usize, ship: &Ship) -> Option<Flight> {
  match choice {
    1 if !ship.embarked.is_empty() => Some(Flight::Launch {
      craft: Vec::new(),
      count: None,
    }),
    2 => ship.carrier.clone().map(|carrier| Flight::Land { carrier }),
    _ => None,
  }
}

/// The ship at the head of a chain of follows, or `None` for a loop.
fn root_of<'a>(ship: &'a str, guides: &'a HashMap<String, String>) -> Option<&'a str> {
  let mut at = ship;
  for _ in 0..=guides.len() {
    match guides.get(at) {
      Some(next) => at = next,
      None => return Some(at),
    }
  }
  None
}

/// Every ship of a side's orders, from their choices: `own` is the side's
/// ships still in play, each with her choice and the picture it was made
/// from.
///
/// A ship that follows joins her guide's squadron, and a guide who herself
/// follows hands her followers on to her own guide, so every squadron is
/// led by a ship flying her own helm. The guide then flies at the slowest
/// member's thrust, eased off while anyone is out of station, exactly as a
/// model commander's squadrons fly. Follows that go round in a loop hold.
#[must_use]
pub fn side_intents(own: &[(&Ship, &Choice, &Picture)], view: &Observation) -> Vec<(String, Intent)> {
  side_intents_with(own, view, true)
}

/// [`side_intents`], with the side-wide missile sharing on or off. Off, the
/// launchers go with the guns, as they did before point defence was
/// counted: kept as a baseline to measure against.
#[must_use]
pub fn side_intents_with(
  own: &[(&Ship, &Choice, &Picture)], view: &Observation, missile_sharing: bool,
) -> Vec<(String, Intent)> {
  let mut helms: BTreeMap<String, Helm> = BTreeMap::new();
  let mut guides: HashMap<String, String> = HashMap::new();
  for (ship, choice, picture) in own {
    let (helm, guide) = helm_of(choice, picture);
    let name = ship.get_name().to_string();
    if let Some(guide) = guide.filter(|guide| own.iter().any(|(other, _, _)| other.get_name() == guide)) {
      guides.insert(name.clone(), guide);
    }
    helms.insert(name, helm);
  }

  // Squadrons: each root and the ships that end up following her.
  let mut squadrons: BTreeMap<String, Vec<String>> = BTreeMap::new();
  for ship in guides.keys() {
    if let Some(root) = root_of(ship, &guides) {
      squadrons.entry(root.to_string()).or_default().push(ship.clone());
    }
  }
  let looped: Vec<String> = guides.keys().filter(|ship| root_of(ship, &guides).is_none()).cloned().collect();

  let ship_named = |name: &str| {
    own
      .iter()
      .find(|(ship, _, _)| ship.get_name() == name)
      .map(|(ship, _, _)| *ship)
  };
  let mut caps: HashMap<String, f64> = HashMap::new();
  let mut follows: HashMap<String, Helm> = HashMap::new();
  for (guide_name, followers) in &squadrons {
    let Some(guide) = ship_named(guide_name) else { continue };
    let members: Vec<&Ship> = std::iter::once(guide)
      .chain(followers.iter().filter_map(|name| ship_named(name)))
      .collect();
    let slowest = f64::from(members.iter().map(|ship| ship.max_acceleration()).min().unwrap_or(1));
    let margin = members
      .iter()
      .map(|ship| {
        straggler_margin(
          (ship.get_position() - guide.get_position()).magnitude(),
          (ship.get_velocity() - guide.get_velocity()).magnitude(),
        )
      })
      .fold(0.0, f64::max);
    let cap = (slowest - margin).max(STRAGGLER_STEP);
    caps.insert(guide_name.clone(), cap);
    let guide_helm = helms.get(guide_name).cloned().unwrap_or_default();
    for follower in followers {
      follows.insert(
        follower.clone(),
        Helm::Follow {
          guide: guide_name.clone(),
          guide_helm: Box::new(guide_helm.clone()),
          guide_cap: Some(cap),
        },
      );
    }
  }

  let mut intents = own
    .iter()
    .map(|(ship, choice, picture)| {
      let name = ship.get_name();
      let helm = if looped.iter().any(|ship| ship == name) {
        Helm::Hold
      } else {
        follows
          .remove(name)
          .unwrap_or_else(|| helms.get(name).cloned().unwrap_or_default())
      };
      let mut intent = intent_of(choice, picture, helm, ship, view);
      intent.max_thrust = caps.get(name).copied();
      (name.to_string(), intent)
    })
    .collect::<Vec<_>>();
  share_fire(&mut intents, own, view, missile_sharing);
  if !missile_sharing {
    return intents;
  }
  let ships: Vec<&Ship> = own.iter().map(|(ship, _, _)| *ship).collect();
  crate::network::missiles::share_missiles(&mut intents, &ships, view);
  intents
}

/// Expected damage, against a target's hull, that counts as enough: past it,
/// more guns on the same target are wasted. Above 1 because expected damage
/// is an average, and dice run short.
pub const ENOUGH: f64 = 1.5;

/// A ship's spinal mounts, by index. One cannot attack a fighter that is
/// manoeuvring (High Guard p. 34), so it has no damage on one to be shared by.
fn spinal_mounts(ship: &Ship) -> impl Iterator<Item = usize> + '_ {
  ship
    .weapons()
    .iter()
    .enumerate()
    .filter(|(_, weapon)| weapon.mount.spinal_multiple().is_some())
    .map(|(mount, _)| mount)
}

/// Share each ship's guns out across the side's targets so that none are
/// wasted on a target already dealt with. A ship's chosen fire target is her
/// priority: her guns go to it, smallest first, until the side's expected
/// damage on it is [`ENOUGH`] times its hull. Then the rest go, again
/// smallest first, to whichever targets they would do most good against --
/// the damage they would land, capped at what each target has left -- each up
/// to `ENOUGH` times its hull, counting what the ships before her sent. A big
/// ship takes every gun; a fighter takes one, and the rest move on. Guns left
/// over still fire at the priority. Called shots and named mounts are left
/// as they are. Launchers are not counted here: they are shared out after,
/// across the side (`missiles::share_missiles`).
fn share_fire(
  intents: &mut [(String, Intent)], own: &[(&Ship, &Choice, &Picture)], view: &Observation, skip_launchers: bool,
) {
  let enemies = view.enemies();
  let hull = |name: &str| {
    enemies
      .iter()
      .find(|contact| contact.name == name)
      .map_or(0.0, |contact| f64::from(contact.current_hull.max(1)))
  };
  // The side's expected damage on each target so far this round.
  let mut sent: HashMap<String, f64> = HashMap::new();
  for (name, intent) in intents.iter_mut() {
    let Some(ship) = own.iter().map(|(ship, _, _)| *ship).find(|ship| ship.get_name() == name) else {
      continue;
    };
    let [first] = intent.fire.as_slice() else { continue };
    if !first.weapons.is_empty() || first.called.is_some() {
      continue;
    }
    let priority = first.target.clone();
    // Each target's per-mount expected damage from this ship, at the band
    // she is at from it.
    let by_mount = |target: &str| -> Vec<(usize, f64)> {
      let Some(contact) = enemies.iter().find(|contact| contact.name == target) else {
        return vec![];
      };
      let band = tactics::band_at((contact.position - ship.get_position()).magnitude());
      // Launchers are shared out separately, across the side, against point
      // defence (`missiles::share_missiles`).
      let launchers: Vec<usize> = tactics::weapons_that_bear(ship, band)
        .into_iter()
        .filter(|(mount, kind)| skip_launchers && tactics::launch_size(&ship.weapons()[*mount], *kind).is_some())
        .map(|(mount, _)| mount)
        .collect();
      view
        .engagement(name, target)
        .and_then(|table| table.we_hit.iter().find(|odds| odds.band == band))
        .map(|odds| {
          odds
            .by_mount
            .iter()
            .copied()
            .filter(|(mount, damage)| *damage > 0.0 && !launchers.contains(mount))
            .collect()
        })
        .unwrap_or_default()
    };
    let room = |target: &str, sent: &HashMap<String, f64>| {
      (ENOUGH * hull(target) - sent.get(target).copied().unwrap_or(0.0)).max(0.0)
    };
    let mut free: Vec<usize> = by_mount(&priority).iter().map(|(mount, _)| *mount).collect();
    // A spinal that cannot lay on the priority is free for the next target.
    let spinals: Vec<usize> = spinal_mounts(ship).filter(|mount| !free.contains(mount)).collect();
    free.extend(spinals);
    let mut shares: Vec<(String, Vec<usize>)> = Vec::new();
    let mut target = Some(priority.clone());
    while let Some(aim) = target.take() {
      // This target's guns, smallest first, until it has enough.
      let mut mounts = by_mount(&aim);
      mounts.retain(|(mount, _)| free.contains(mount));
      mounts.sort_by(|a, b| a.1.total_cmp(&b.1));
      let mut given = Vec::new();
      for (mount, damage) in mounts {
        if room(&aim, &sent) <= 0.0 {
          break;
        }
        *sent.entry(aim.clone()).or_default() += damage;
        given.push(mount);
        free.retain(|left| *left != mount);
      }
      shares.push((aim, given));
      if free.is_empty() {
        break;
      }
      // The next target: where the free guns would land the most damage,
      // up to what it has room for.
      target = enemies
        .iter()
        .filter(|contact| !shares.iter().any(|(taken, _)| *taken == contact.name))
        .map(|contact| {
          let lands = by_mount(&contact.name)
            .iter()
            .filter(|(mount, _)| free.contains(mount))
            .map(|(_, damage)| damage)
            .sum::<f64>()
            .min(room(&contact.name, &sent));
          (contact.name.clone(), lands)
        })
        .filter(|(_, lands)| *lands > 0.0)
        .max_by(|a, b| a.1.total_cmp(&b.1).then_with(|| b.0.cmp(&a.0)))
        .map(|(name, _)| name);
    }
    shares.retain(|(_, mounts)| !mounts.is_empty());
    if shares.iter().all(|(aim, _)| *aim == priority) {
      // Nothing moved: fire as chosen, every mount that bears.
      continue;
    }
    // Guns left over stay on the priority.
    match shares.iter_mut().find(|(aim, _)| *aim == priority) {
      Some((_, mounts)) => mounts.extend(free.iter().copied()),
      None if !free.is_empty() => shares.push((priority.clone(), free.clone())),
      None => {}
    }
    intent.fire = shares
      .into_iter()
      .map(|(target, weapons)| Engage {
        target,
        weapons,
        ..Engage::default()
      })
      .collect();
  }
}

/// The choice that asks for `intent`, as near as the heads can say it: what a
/// baseline's orders look like through this action space. Anything the
/// intent leaves as it is (posture, dodge, assist) is chosen as the ship has
/// it now, so asking for it changes nothing.
#[must_use]
pub fn choice_of(intent: &Intent, ship: &Ship, picture: &Picture) -> Choice {
  let slot = |name: &str| {
    picture
      .enemies
      .iter()
      .position(|enemy| enemy == name)
      .and_then(|slot| i64::try_from(slot).ok())
  };
  let band = |band: Range| {
    HELM_BANDS
      .iter()
      .position(|helm_band| *helm_band == band)
      .and_then(|band| i64::try_from(band).ok())
      .unwrap_or(2)
  };
  let mut choice: Choice = [0; HEADS];
  let mut helm = |verb: Verb, target: &str, band_index: i64| {
    if let Some(target) = slot(target) {
      choice[Head::Verb as usize] = verb as i64;
      choice[Head::HelmTarget as usize] = target;
      choice[Head::Band as usize] = band_index;
    }
  };
  match &intent.helm {
    Helm::Intercept { target, band: b } => helm(Verb::Intercept, target, band(*b)),
    Helm::HoldRange { target, band: b } => helm(Verb::HoldRange, target, band(*b)),
    Helm::OpenRange { target } => helm(Verb::OpenRange, target, 0),
    Helm::Hold | Helm::Station { .. } | Helm::Follow { .. } => {}
  }
  choice[Head::Fire as usize] = intent
    .fire
    .first()
    .and_then(|engage| slot(&engage.target))
    .map_or(0, |slot| slot + 1);
  choice[Head::Lock as usize] = intent
    .sensors
    .iter()
    .find_map(|verb| match verb {
      SensorVerb::Lock { target } => slot(target),
      _ => None,
    })
    .map_or(0, |slot| slot + 1);
  let posture = intent
    .posture
    .and_then(|posture| POSTURES.iter().position(|p| *p == posture))
    .unwrap_or_else(|| posture_of(ship));
  choice[Head::Posture as usize] = i64::try_from(posture).unwrap_or(0);
  choice[Head::Dodge as usize] = i64::from(intent.evade.unwrap_or_else(|| ship.get_dodge_thrust())).min(2);
  choice[Head::Assist as usize] = i64::from(intent.assist_gunners.unwrap_or_else(|| ship.get_assist_gunners()));
  choice[Head::Jump as usize] = i64::from(intent.engineering.iter().any(|verb| matches!(verb, EngineerVerb::Jump)));
  choice[Head::Flight as usize] = match intent.flight {
    Some(Flight::Launch { .. }) => 1,
    Some(Flight::Land { .. }) => 2,
    Some(Flight::Hold) | None => 0,
  };
  choice
}

#[cfg(test)]
mod spinal_tests {
  use super::*;
  use crate::observation::observe;
  use callisto::entity::{Entities, Vec3};
  use callisto::ship::{ShipDesignTemplate, Team, Weapon, WeaponMount, WeaponType};
  use cgmath::Zero;
  use std::sync::Arc;

  /// A ship whose priority is a fighter keeps her turret on it and sends her
  /// spinal, which cannot attack a fighter, to the cruiser beside it.
  #[test]
  fn a_spinal_goes_to_the_target_it_can_hit() {
    let monitor = Arc::new(ShipDesignTemplate {
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
      ..ShipDesignTemplate::default()
    });
    let fighter = Arc::new(ShipDesignTemplate {
      name: "Fighter".to_string(),
      displacement: 10,
      hull: 4,
      maneuver: 6,
      power: 10,
      ..ShipDesignTemplate::default()
    });
    let mut world = Entities::default();
    world.add_ship("Monitor".to_string(), Vec3::zero(), Vec3::zero(), &monitor, None, None);
    world.add_ship(
      "Fighter".to_string(),
      Vec3::new(5.0e6, 0.0, 0.0),
      Vec3::zero(),
      &fighter,
      None,
      None,
    );
    world.add_ship(
      "Cruiser".to_string(),
      Vec3::new(5.5e6, 0.0, 0.0),
      Vec3::zero(),
      &monitor,
      None,
      None,
    );
    world.ships["Monitor"].write().unwrap().team = Some(Team::Blue);
    world.ships["Fighter"].write().unwrap().team = Some(Team::Red);
    world.ships["Cruiser"].write().unwrap().team = Some(Team::Red);
    world.establish_initial_contacts();
    let view = observe(&world, &["Monitor".to_string()]);
    let ship = world.ships["Monitor"].read().unwrap().clone();
    let choice: Choice = [0; HEADS];
    let picture = Picture::default();
    let mut intents = vec![(
      "Monitor".to_string(),
      Intent {
        fire: vec![Engage {
          target: "Fighter".to_string(),
          ..Engage::default()
        }],
        ..Intent::default()
      },
    )];
    share_fire(&mut intents, &[(&ship, &choice, &picture)], &view, false);
    let fire = &intents[0].1.fire;
    let on = |target: &str| {
      fire
        .iter()
        .find(|engage| engage.target == target)
        .map(|engage| engage.weapons.clone())
    };
    assert_eq!(on("Fighter"), Some(vec![1]), "{fire:?}");
    assert_eq!(on("Cruiser"), Some(vec![0]), "{fire:?}");
  }
}

#[cfg(test)]
mod flight_tests {
  use super::*;
  use callisto::ship::ShipDesignTemplate;
  use cgmath::Zero;
  use std::sync::Arc;

  fn ship(carrier: Option<&str>, aboard: usize) -> Ship {
    let design = Arc::new(ShipDesignTemplate::default());
    let mut ship = Ship::new(
      "Ship".to_string(),
      callisto::entity::Vec3::zero(),
      callisto::entity::Vec3::zero(),
      &design,
      None,
      None,
    );
    ship.carrier = carrier.map(str::to_string);
    for i in 0..aboard {
      ship.embarked.push(Ship::new(
        format!("Craft {i}"),
        callisto::entity::Vec3::zero(),
        callisto::entity::Vec3::zero(),
        &design,
        None,
        None,
      ));
    }
    ship
  }

  /// "none" is the crews' own rules for everyone; "launch" and "land" mean
  /// what they say where they apply, and are "none" where they do not.
  #[test]
  fn the_flight_head_becomes_a_flight_order() {
    let launch = Some(Flight::Launch {
      craft: vec![],
      count: None,
    });
    assert_eq!(flight_of(0, &ship(None, 0)), None);
    assert_eq!(flight_of(1, &ship(None, 0)), None);
    assert_eq!(flight_of(0, &ship(None, 2)), None);
    assert_eq!(flight_of(1, &ship(None, 2)), launch);
    assert_eq!(flight_of(2, &ship(None, 2)), None);
    assert_eq!(flight_of(0, &ship(Some("Mother"), 0)), None);
    assert_eq!(flight_of(1, &ship(Some("Mother"), 0)), None);
    assert_eq!(
      flight_of(2, &ship(Some("Mother"), 0)),
      Some(Flight::Land {
        carrier: "Mother".to_string()
      })
    );
  }

  #[test]
  fn a_flight_order_reads_back_as_its_choice() {
    let me = ship(Some("Mother"), 0);
    let picture = Picture::default();
    let choice = |flight: Option<Flight>| {
      choice_of(
        &Intent {
          flight,
          ..Intent::default()
        },
        &me,
        &picture,
      )[Head::Flight as usize]
    };
    assert_eq!(choice(None), 0);
    assert_eq!(choice(Some(Flight::Hold)), 0);
    assert_eq!(
      choice(Some(Flight::Launch {
        craft: vec![],
        count: None
      })),
      1
    );
    assert_eq!(
      choice(Some(Flight::Land {
        carrier: "Mother".to_string()
      })),
      2
    );
  }
}

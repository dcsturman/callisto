//! Flight operations: a carrier's craft going out and coming back.
//!
//! A craft aboard is not on the plot. Launching puts her into
//! `Entities.ships` as the round ends; recovery takes her out. The rules are
//! High Guard's: ten craft a round through a launch tube or a recovery deck
//! (HG FAQ p. 3), D3 rounds through a docking space with the carrier pinned
//! (HG p. 61), and the Pilot check on a deck (HG p. 104). The rulings around
//! them are in `docs/capital_ships_plan.md` §4 and the FAQ.
//!
//! Two passes a round: [`begin_round`] reads the Launch orders and pins the
//! ships a berth operation holds still, before anything fires or moves;
//! [`end_round`] runs after movement, completes the operations that are
//! due, resolves every Land order, rearms the craft that have had a round
//! aboard, and puts launched craft on the plot.

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::hash::BuildHasher;
use std::sync::{Arc, RwLock};

use cgmath::InnerSpace;
use rand::RngCore;

use crate::action::ShipAction;
use crate::combat::{find_range_band, roll, roll_dice, STANDARD_ROLL_THRESHOLD};
use crate::entity::{Entities, Entity, Vec3, G};
use crate::payloads::{EffectMsg, MessageCategory};
use crate::ship::{BridgeStation, FlightOp, FlightOpKind, Range, Ship, MAX_SQUADRON};
use crate::{debug, warn};

/// Craft of this tonnage and up take 1D rounds through a berth rather than
/// D3 (HG pp. 57, 61).
const LARGE_CRAFT_TONS: u32 = 2_000;

/// How far ahead of the carrier a launched craft appears, in metres: a
/// little ahead, as a missile does, and within Close range of her.
const LAUNCH_LEAD_M: f64 = 5_000.0;

/// The ring launched craft are spread around that point, so a wing is not
/// one point on the plot.
const LAUNCH_SPREAD_M: f64 = 1_000.0;

/// A craft coming in must be within this much of the carrier's velocity for
/// each G of Thrust she has: a minute's burn (plan §4, an assumption; HG
/// has no relative velocity).
const LANDING_SECONDS_PER_G: f64 = 60.0;

/// Rounds a berth operation takes for a craft of `tons`: D3, or 1D for a
/// large one.
fn berth_rounds(tons: u32, rng: &mut dyn RngCore) -> u8 {
  let d6 = roll(rng);
  if tons >= LARGE_CRAFT_TONS {
    d6
  } else {
    d6.div_ceil(2)
  }
}

/// The relative velocity a craft of `thrust` G may come in at, in m/s. A
/// crippled craft keeps a G's worth: she may still drift into a match.
fn landing_velocity_limit(thrust: u8) -> f64 {
  f64::from(thrust.max(1)) * G * LANDING_SECONDS_PER_G
}

/// Set `pinned` on every ship from the berth operations under way: a
/// carrier with one, and a craft being recovered through one.
fn refresh_pinned(entities: &Entities) {
  let mut pinned: HashSet<String> = HashSet::new();
  for (name, ship) in &entities.ships {
    let ship = ship.read().unwrap();
    for op in ship.flight_ops.iter().filter(|op| op.kind.pins_carrier()) {
      pinned.insert(name.clone());
      if op.kind == FlightOpKind::BerthRecovery {
        pinned.insert(op.craft.clone());
      }
    }
  }
  for (name, ship) in &entities.ships {
    ship.write().unwrap().pinned = pinned.contains(name);
  }
}

/// The ships a flight operation holds still this round, as set by
/// [`begin_round`]: no Thrust, no dodge, no attacks.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[must_use]
pub fn pinned_ships(entities: &Entities) -> HashSet<String> {
  entities
    .ships
    .iter()
    .filter(|(_, ship)| ship.read().unwrap().pinned)
    .map(|(name, _)| name.clone())
    .collect()
}

/// Read the round's Launch orders into flight operations and pin whoever a
/// berth operation holds still. Before fire and movement.
///
/// A carrier with a powered tube that takes the craft launches through it,
/// ten a tube a round, and the surplus waits for next round's order. One
/// with no working tube releases through the berths: D3 rounds (1D for a
/// large craft) with the carrier pinned until the last is out, each craft
/// on her own count.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
pub fn begin_round(entities: &mut Entities, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
  let mut effects = Vec::new();
  let mut launches: Vec<(String, Vec<String>)> = entities
    .actions
    .iter()
    .flat_map(|(ship_name, actions)| {
      actions.iter().filter_map(move |action| match action {
        ShipAction::Launch { craft } => Some((ship_name.clone(), craft.clone())),
        _ => None,
      })
    })
    .collect();
  launches.sort_by(|a, b| a.0.cmp(&b.0));

  for (carrier_name, craft_names) in launches {
    let Some(carrier) = entities.ships.get(&carrier_name) else {
      warn!("(flight.begin_round) No carrier {carrier_name} for a launch order.");
      continue;
    };
    let mut carrier = carrier.write().unwrap();
    let mut by_tube: Vec<String> = Vec::new();
    let mut tube_used = 0;
    for name in craft_names {
      let Some(craft) = carrier.craft_aboard(&name) else {
        effects.push(EffectMsg::about(
          &carrier_name,
          MessageCategory::Info,
          format!("%s has no craft named {name} aboard to launch."),
        ));
        continue;
      };
      if carrier.in_flight_op(&name) {
        continue;
      }
      let tons = craft.design.displacement;
      let rate = carrier.launch_per_round(tons);
      if rate > 0 {
        if tube_used >= rate {
          effects.push(EffectMsg::about(
            &carrier_name,
            MessageCategory::Info,
            format!("%s's launch tubes are full this round ({rate} craft a round): {name} waits."),
          ));
          continue;
        }
        tube_used += 1;
        carrier.flight_ops.push(FlightOp {
          craft: name.clone(),
          kind: FlightOpKind::TubeLaunch,
          rounds_left: 1,
        });
        by_tube.push(name);
      } else {
        let rounds = berth_rounds(tons, rng);
        carrier.flight_ops.push(FlightOp {
          craft: name.clone(),
          kind: FlightOpKind::BerthLaunch,
          rounds_left: rounds,
        });
        effects.push(EffectMsg::about(
          &carrier_name,
          MessageCategory::Info,
          format!(
            "%s begins releasing {name} from her berth, with no launch tube for her: {rounds} round{}, holding still and making no attacks until she is out.",
            if rounds == 1 { "" } else { "s" }
          ),
        ));
      }
    }
    if !by_tube.is_empty() {
      effects.push(EffectMsg::about(
        &carrier_name,
        MessageCategory::Info,
        format!(
          "%s launches {} through her tubes: {}. They are on the plot as the round ends.",
          by_tube.len(),
          by_tube.join(", ")
        ),
      ));
    }
  }
  refresh_pinned(entities);
  effects
}

/// A Land order as the end of the round sees it.
struct Landing {
  craft: String,
  carrier: String,
}

/// Where the `index`th craft a carrier launches this round appears: ahead
/// of her along her course, spread around a ring.
fn launch_position(carrier: &Ship, index: usize) -> Vec3 {
  let velocity = carrier.get_velocity();
  let ahead = if velocity.magnitude() > 1.0 {
    velocity.normalize()
  } else {
    Vec3::unit_x()
  };
  let up = if ahead.z.abs() < 0.9 {
    Vec3::unit_z()
  } else {
    Vec3::unit_y()
  };
  let side = ahead.cross(up).normalize();
  let up = side.cross(ahead).normalize();
  // The golden angle spaces any number of craft evenly around the ring.
  #[allow(clippy::cast_precision_loss)]
  let angle = index as f64 * 2.399_963;
  carrier.get_position() + ahead * LAUNCH_LEAD_M + (side * angle.cos() + up * angle.sin()) * LAUNCH_SPREAD_M
}

/// Name the squadron a launched craft flies in: hers if the scenario gave
/// her one, otherwise her carrier's air group, spilling into "air group 2"
/// and so on at the squadron cap.
fn join_air_group(entities: &Entities, craft: &str, carrier: &str) {
  let named = entities.ships.get(craft).and_then(|ship| ship.read().unwrap().squadron.clone());
  if let Some(squadron) = named {
    if entities.set_squadron(&[craft.to_string()], Some(&squadron)).is_ok() {
      return;
    }
  }
  let base = format!("{carrier} air group");
  for n in 1..=u32::try_from(MAX_SQUADRON).unwrap_or(u32::MAX) * 50 {
    let name = if n == 1 { base.clone() } else { format!("{base} {n}") };
    if entities.set_squadron(&[craft.to_string()], Some(&name)).is_ok() {
      return;
    }
  }
}

/// Complete the operations that are due, land what can land, rearm what
/// has had a round aboard, and put launched craft on the plot. After
/// movement.
///
/// `snapshot` is the world as the round began: what each ship's plan was
/// when it was flown, which is the Thrust the landing check counts.
///
/// # Panics
/// Panics if a ship's lock is poisoned.
#[allow(clippy::too_many_lines)]
pub fn end_round<S: BuildHasher>(
  entities: &mut Entities, snapshot: &HashMap<String, Ship, S>, rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
  let mut effects = Vec::new();
  let mut carriers: Vec<String> = entities
    .ships
    .iter()
    .filter(|(_, ship)| {
      let ship = ship.read().unwrap();
      ship.is_carrier() || !ship.flight_ops.is_empty()
    })
    .map(|(name, _)| name.clone())
    .collect();
  carriers.sort();

  // Craft that come out this round, with where they appear.
  let mut launched: Vec<(String, Ship)> = Vec::new();
  // Craft that go in this round, by carrier, so they are not counted as
  // having spent a round aboard.
  let mut landed: HashMap<String, Vec<String>> = HashMap::new();
  let mut destroyed: Vec<String> = Vec::new();

  // Operations that are due.
  for carrier_name in &carriers {
    let carrier = entities.ships[carrier_name].clone();
    let due: Vec<FlightOp> = {
      let mut carrier = carrier.write().unwrap();
      for op in &mut carrier.flight_ops {
        op.rounds_left = op.rounds_left.saturating_sub(1);
      }
      let (due, pending): (Vec<FlightOp>, Vec<FlightOp>) =
        carrier.flight_ops.drain(..).partition(|op| op.rounds_left == 0);
      carrier.flight_ops = pending;
      due
    };
    let mut out: Vec<String> = Vec::new();
    for op in due {
      match op.kind {
        FlightOpKind::TubeLaunch | FlightOpKind::BerthLaunch => {
          let mut carrier = carrier.write().unwrap();
          let position = launch_position(&carrier, out.len() + launched.len());
          if let Some(craft) = carrier.release(&op.craft, position) {
            out.push(op.craft.clone());
            launched.push((carrier_name.clone(), craft));
          }
        }
        FlightOpKind::BerthRecovery => {
          let Some(craft) = entities.ships.remove(&op.craft) else {
            // She was lost while coming in; nothing to take aboard.
            continue;
          };
          let craft = Arc::try_unwrap(craft).map_or_else(
            |arc| arc.read().unwrap().clone(),
            |lock| lock.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner),
          );
          carrier.write().unwrap().embark(craft);
          landed.entry(carrier_name.clone()).or_default().push(op.craft.clone());
          effects
            .push(EffectMsg::about(&op.craft, MessageCategory::Info, "%s is aboard %t.".to_string()).at(carrier_name));
        }
      }
    }
    if !out.is_empty() {
      effects.push(EffectMsg::about(
        carrier_name,
        MessageCategory::Info,
        format!("%s's craft are out: {}.", out.join(", ")),
      ));
    }
  }

  // Land orders, in the craft's name order so a seeded run reads the same.
  let mut landings: Vec<Landing> = entities
    .actions
    .iter()
    .flat_map(|(ship_name, actions)| {
      actions.iter().filter_map(move |action| match action {
        ShipAction::Land { carrier } => Some(Landing {
          craft: ship_name.clone(),
          carrier: carrier.clone(),
        }),
        _ => None,
      })
    })
    .collect();
  landings.sort_by(|a, b| a.craft.cmp(&b.craft));
  // How many have used each carrier's decks this round.
  let mut deck_used: HashMap<String, u32> = HashMap::new();

  for landing in &landings {
    let Some(craft_arc) = entities.ships.get(&landing.craft).cloned() else {
      continue;
    };
    let Some(carrier_arc) = entities.ships.get(&landing.carrier).cloned() else {
      effects.push(EffectMsg::about(
        &landing.craft,
        MessageCategory::Info,
        format!("%s cannot land: there is no {} to land on.", landing.carrier),
      ));
      continue;
    };
    // Already on her way in through a berth: the operation sees to it.
    if carrier_arc.read().unwrap().in_flight_op(&landing.craft) {
      continue;
    }
    let refusal = {
      let craft = craft_arc.read().unwrap();
      let carrier = carrier_arc.read().unwrap();
      let tons = craft.design.displacement;
      let distance = (craft.get_position() - carrier.get_position()).magnitude();
      #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
      let band = find_range_band(distance.min(f64::from(u32::MAX)) as u32);
      let closing = (craft.get_velocity() - carrier.get_velocity()).magnitude();
      let limit = landing_velocity_limit(craft.current_maneuver);
      if !carrier.is_carrier() {
        Some("%t has no berth to take her.".to_string())
      } else if craft.team != carrier.team {
        Some("%t is not her side's ship.".to_string())
      } else if !craft.station_working(BridgeStation::Pilot) {
        Some("her pilot's station is out.".to_string())
      } else if band > Range::Short {
        Some(format!(
          "%t is at {band} range, {:.0} km off; she must be within Short.",
          distance / 1000.0
        ))
      } else if closing > limit {
        Some(format!(
          "she is passing %t at {:.1} km/s, and may come in at no more than {:.1} km/s.",
          closing / 1000.0,
          limit / 1000.0
        ))
      } else if !carrier.has_berth_for(tons) {
        Some("%t has no berth free for her; she waits.".to_string())
      } else {
        None
      }
    };
    if let Some(why) = refusal {
      effects.push(
        EffectMsg::about(&landing.craft, MessageCategory::Info, format!("%s cannot land: {why}")).at(&landing.carrier),
      );
      continue;
    }

    let (tons, rate) = {
      let craft = craft_arc.read().unwrap();
      let carrier = carrier_arc.read().unwrap();
      let tons = craft.design.displacement;
      (tons, carrier.recover_per_round(tons))
    };
    if rate == 0 {
      // No deck for her: in through a berth, both ships held still and no
      // check. She matches the carrier's velocity now and is aboard as the
      // round in which the count runs out ends.
      let rounds = berth_rounds(tons, rng);
      let velocity = carrier_arc.read().unwrap().get_velocity();
      craft_arc.write().unwrap().set_velocity(velocity);
      carrier_arc.write().unwrap().flight_ops.push(FlightOp {
        craft: landing.craft.clone(),
        kind: FlightOpKind::BerthRecovery,
        rounds_left: rounds,
      });
      effects.push(
        EffectMsg::about(
          &landing.craft,
          MessageCategory::Info,
          format!(
            "%s begins docking with %t, which has no recovery deck for her: {rounds} round{}, both holding still and making no attacks until she is in.",
            if rounds == 1 { "" } else { "s" }
          ),
        )
        .at(&landing.carrier),
      );
      continue;
    }

    let used = deck_used.entry(landing.carrier.clone()).or_default();
    if *used >= rate {
      effects.push(
        EffectMsg::about(
          &landing.craft,
          MessageCategory::Info,
          format!("%s cannot land this round: %t's recovery decks have taken their {rate}; she waits."),
        )
        .at(&landing.carrier),
      );
      continue;
    }
    *used += 1;

    // The check (HG p. 104): Routine (6+) Pilot, DM-1 per point of Thrust
    // either ship spent this round, DM-1 per other craft coming in to this
    // carrier this round that is not of her squadron.
    let thrust_of = |name: &str| snapshot.get(name).map_or(0, Ship::thrust_in_g);
    let (craft_g, carrier_g) = (thrust_of(&landing.craft), thrust_of(&landing.carrier));
    let squadron = craft_arc.read().unwrap().squadron.clone();
    let others = landings
      .iter()
      .filter(|other| other.carrier == landing.carrier && other.craft != landing.craft)
      .filter(|other| {
        let theirs = entities
          .ships
          .get(&other.craft)
          .and_then(|ship| ship.read().unwrap().squadron.clone());
        squadron.is_none() || theirs != squadron
      })
      .count();
    let (pilot, crew) = {
      let craft = craft_arc.read().unwrap();
      (i16::from(craft.get_crew().get_pilot()), craft.crew_dm())
    };
    let dice = roll_dice(2, rng);
    let terms: Vec<(&str, i16)> = vec![
      ("pilot", pilot),
      ("crew", crew),
      ("craft's thrust", -i16::from(craft_g)),
      ("carrier's thrust", -i16::from(carrier_g)),
      ("other craft landing", -i16::try_from(others).unwrap_or(i16::MAX)),
    ];
    let total = terms.iter().fold(i32::from(dice), |sum, (_, dm)| sum + i32::from(*dm));
    let mut arithmetic = format!("roll {dice}");
    for (what, dm) in terms.iter().filter(|(_, dm)| *dm != 0) {
      let _ = write!(arithmetic, " {} {what} {}", if *dm < 0 { "-" } else { "+" }, dm.abs());
    }
    let _ = write!(arithmetic, " = {total} vs {STANDARD_ROLL_THRESHOLD}");
    if total >= STANDARD_ROLL_THRESHOLD {
      let Some(craft) = entities.ships.remove(&landing.craft) else {
        continue;
      };
      let craft = Arc::try_unwrap(craft).map_or_else(
        |arc| arc.read().unwrap().clone(),
        |lock| lock.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner),
      );
      carrier_arc.write().unwrap().embark(craft);
      landed.entry(landing.carrier.clone()).or_default().push(landing.craft.clone());
      effects.push(
        EffectMsg::outcome(
          &landing.craft,
          MessageCategory::Info,
          "%s lands on %t's recovery deck.".to_string(),
          true,
        )
        .at(&landing.carrier)
        .rolled(arithmetic),
      );
    } else {
      // 1D x the Thrust both spent, to both, ignoring armour (HG p. 104).
      let damage = u32::from(roll(rng)) * u32::from(craft_g + carrier_g);
      for ship in [&craft_arc, &carrier_arc] {
        let mut ship = ship.write().unwrap();
        ship.current_hull = ship.current_hull.saturating_sub(damage);
        if ship.current_hull == 0 {
          destroyed.push(ship.get_name().to_string());
        }
      }
      effects.push(
        EffectMsg::outcome(
          &landing.craft,
          MessageCategory::Damage,
          format!("%s fails to land on %t: {damage} damage to each, ignoring armour."),
          false,
        )
        .at(&landing.carrier)
        .rolled(arithmetic),
      );
    }
  }

  // A craft or a carrier wrecked on the deck is gone, with everything aboard.
  destroyed.sort();
  destroyed.dedup();
  for name in &destroyed {
    let Some(ship) = entities.ships.remove(name) else {
      continue;
    };
    let ship = ship.read().unwrap();
    effects.push(EffectMsg::ShipDestroyed {
      position: ship.get_position(),
    });
    effects.push(EffectMsg::about(
      name,
      MessageCategory::Destruction,
      "%s destroyed on the deck.".to_string(),
    ));
    effects.extend(ship.embarked.iter().map(|craft| {
      EffectMsg::about(
        craft.get_name(),
        MessageCategory::Destruction,
        "%s destroyed in her berth aboard %t.".to_string(),
      )
      .at(name)
    }));
  }

  // A round aboard rearms a craft from the carrier's stores.
  for carrier_name in &carriers {
    let Some(carrier) = entities.ships.get(carrier_name) else {
      continue;
    };
    let mut carrier = carrier.write().unwrap();
    let fresh = landed.get(carrier_name).cloned().unwrap_or_default();
    for index in 0..carrier.embarked.len() {
      let name = carrier.embarked[index].get_name().to_string();
      if fresh.contains(&name) || carrier.in_flight_op(&name) {
        continue;
      }
      carrier.embarked[index].rounds_aboard += 1;
      if !carrier.craft_wants_rearming(index) {
        continue;
      }
      if let Some(taken) = carrier.rearm_craft(index) {
        let stores = carrier.magazine;
        let counts: Vec<String> = [
          ("missiles", taken.missiles, stores.missiles),
          ("torpedoes", taken.torpedoes, stores.torpedoes),
          ("sand canisters", taken.sand, stores.sand),
        ]
        .into_iter()
        .filter(|(_, got, _)| *got > 0)
        .map(|(what, got, left)| format!("{got} {what} ({left} left in stores)"))
        .collect();
        effects.push(
          EffectMsg::about(
            carrier_name,
            MessageCategory::Info,
            format!("%s rearms %t: {}.", counts.join("; ")),
          )
          .at(&name),
        );
      }
    }
  }

  // Launched craft take the plot: in their squadron, with the fleet's
  // orders, to act from next round.
  for (carrier_name, craft) in launched {
    let name = craft.get_name().to_string();
    debug!("(flight.end_round) {name} launched from {carrier_name}.");
    entities.ships.insert(name.clone(), Arc::new(RwLock::new(craft)));
    join_air_group(entities, &name, &carrier_name);
    entities.take_up_fleet_orders(&name);
  }

  if !landed.is_empty() || !destroyed.is_empty() {
    entities.prune_ship_references();
    effects.append(&mut entities.prune_orphaned_missiles());
  }
  refresh_pinned(entities);
  effects
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ship::{config_test_ship_templates, get_ship_template, FeatureKind, ShipDesignTemplate, ShipFeature};
  use cgmath::Zero;
  use rand::rngs::SmallRng;
  use rand::SeedableRng;
  use std::sync::Arc;

  /// A 50,000-ton carrier design with `berths` docking spaces for 50-ton
  /// craft, `tubes` launch tubes and `decks` recovery decks.
  fn carrier_design(name: &str, berths: u32, tubes: u32, decks: u32) -> Arc<ShipDesignTemplate> {
    let mut features = vec![ShipFeature {
      name: "Docking spaces".to_string(),
      power: 0,
      default_on: false,
      kind: Some(FeatureKind::DockingSpace {
        craft_tons: 50,
        count: berths,
      }),
    }];
    if tubes > 0 {
      features.push(ShipFeature {
        name: "Launch tube".to_string(),
        power: 500,
        default_on: true,
        kind: Some(FeatureKind::LaunchTube {
          craft_tons: 50,
          count: tubes,
        }),
      });
    }
    if decks > 0 {
      features.push(ShipFeature {
        name: "Recovery deck".to_string(),
        power: 500,
        default_on: true,
        kind: Some(FeatureKind::RecoveryDeck {
          craft_tons: 50,
          count: decks,
        }),
      });
    }
    Arc::new(ShipDesignTemplate {
      name: name.to_string(),
      displacement: 50_000,
      hull: 27_500,
      armor: 10,
      maneuver: 4,
      jump: 2,
      power: 60_000,
      fuel: 10_000,
      crew: 500,
      features,
      magazine: crate::ship::Magazine {
        missiles: 100,
        ..Default::default()
      },
      ..Default::default()
    })
  }

  /// A world with one carrier at the origin and `craft` heavy fighters
  /// aboard, named "Fighter 1" and up, all on Blue.
  async fn carrier_world(design: &Arc<ShipDesignTemplate>, craft: usize) -> Entities {
    config_test_ship_templates().await;
    let mut entities = Entities::new();
    entities.add_ship("Carrier".to_string(), Vec3::zero(), Vec3::zero(), design, None, None);
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    {
      let mut carrier = entities.ships["Carrier"].write().unwrap();
      carrier.team = Some(crate::ship::Team::Blue);
      for n in 1..=craft {
        let mut ship = Ship::new(format!("Fighter {n}"), Vec3::zero(), Vec3::zero(), &fighter, None, None);
        ship.team = Some(crate::ship::Team::Blue);
        carrier.embark(ship);
      }
    }
    entities
  }

  fn names(n: usize) -> Vec<String> {
    (1..=n).map(|i| format!("Fighter {i}")).collect()
  }

  fn order(entities: &mut Entities, ship: &str, action: ShipAction) {
    crate::action::merge(entities, vec![(ship.to_string(), vec![action])]);
  }

  fn lines(effects: &[EffectMsg]) -> Vec<String> {
    effects
      .iter()
      .filter_map(|e| match e {
        EffectMsg::Message { .. } => Some(e.rendered()),
        _ => None,
      })
      .collect()
  }

  fn aboard(entities: &Entities, carrier: &str) -> Vec<String> {
    let mut names: Vec<String> = entities.ships[carrier]
      .read()
      .unwrap()
      .embarked
      .iter()
      .map(|c| c.get_name().to_string())
      .collect();
    names.sort();
    names
  }

  #[tokio::test]
  async fn a_tube_launches_ten_a_round_and_the_rest_wait() {
    let design = carrier_design("Tube Carrier", 30, 1, 1);
    let mut entities = carrier_world(&design, 12).await;
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    order(&mut entities, "Carrier", ShipAction::Launch { craft: names(12) });
    let begun = begin_round(&mut entities, &mut rng);
    assert!(lines(&begun).iter().any(|l| l.contains("Fighter 11 waits")), "{begun:?}");
    assert!(!entities.ships["Carrier"].read().unwrap().pinned, "a tube launch pins nothing");
    let snapshot = entities.ship_deep_copy();
    end_round(&mut entities, &snapshot, &mut rng);
    assert_eq!(entities.ships.len(), 11, "the carrier and ten craft");
    assert_eq!(aboard(&entities, "Carrier"), vec!["Fighter 11", "Fighter 12"]);
    let out = entities.ships["Fighter 1"].read().unwrap();
    assert_eq!(out.carrier.as_deref(), Some("Carrier"));
    assert_eq!(out.squadron.as_deref(), Some("Carrier air group"));
    assert_eq!(out.team, Some(crate::ship::Team::Blue));
    let km = (out.get_position() - entities.ships["Carrier"].read().unwrap().get_position()).magnitude() / 1000.0;
    assert!(km < 10.0 && km > 1.0, "launched {km:.1} km from the carrier");
  }

  #[tokio::test]
  async fn three_tubes_launch_thirty_and_a_wing_spills_into_a_second_air_group() {
    let design = carrier_design("Three Tubes", 30, 3, 1);
    let mut entities = carrier_world(&design, 25).await;
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    order(&mut entities, "Carrier", ShipAction::Launch { craft: names(25) });
    begin_round(&mut entities, &mut rng);
    let snapshot = entities.ship_deep_copy();
    end_round(&mut entities, &snapshot, &mut rng);
    assert_eq!(entities.ships.len(), 26, "the carrier and all 25");
    let mut squadrons: HashMap<String, usize> = HashMap::new();
    for ship in entities.ships.values() {
      if let Some(squadron) = &ship.read().unwrap().squadron {
        *squadrons.entry(squadron.clone()).or_default() += 1;
      }
    }
    assert_eq!(squadrons["Carrier air group"], 20);
    assert_eq!(squadrons["Carrier air group 2"], 5);
    assert!(squadrons.values().all(|n| *n <= MAX_SQUADRON));
  }

  #[tokio::test]
  async fn a_named_squadron_is_kept_on_launch() {
    let design = carrier_design("Tube Carrier", 30, 1, 1);
    let mut entities = carrier_world(&design, 2).await;
    entities.ships["Carrier"].write().unwrap().embarked[0].squadron = Some("Red Wing".to_string());
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    order(&mut entities, "Carrier", ShipAction::Launch { craft: names(2) });
    begin_round(&mut entities, &mut rng);
    let snapshot = entities.ship_deep_copy();
    end_round(&mut entities, &snapshot, &mut rng);
    assert_eq!(
      entities.ships["Fighter 1"].read().unwrap().squadron.as_deref(),
      Some("Red Wing")
    );
    assert_eq!(
      entities.ships["Fighter 2"].read().unwrap().squadron.as_deref(),
      Some("Carrier air group")
    );
  }

  #[tokio::test]
  async fn without_a_tube_the_carrier_is_pinned_until_the_craft_is_out() {
    let design = carrier_design("Berth Carrier", 30, 0, 0);
    let mut entities = carrier_world(&design, 1).await;
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    order(&mut entities, "Carrier", ShipAction::Launch { craft: names(1) });
    let begun = begin_round(&mut entities, &mut rng);
    assert!(lines(&begun)[0].contains("begins releasing Fighter 1"), "{begun:?}");
    let rounds = entities.ships["Carrier"].read().unwrap().flight_ops[0].rounds_left;
    assert!((1..=3).contains(&rounds), "D3 rounds, got {rounds}");
    assert!(entities.ships["Carrier"].read().unwrap().pinned);
    assert_eq!(entities.ships["Carrier"].read().unwrap().get_dodge_thrust(), 0);
    for round in 1..=rounds {
      let snapshot = entities.ship_deep_copy();
      end_round(&mut entities, &snapshot, &mut rng);
      let out = entities.ships.contains_key("Fighter 1");
      assert_eq!(out, round == rounds, "round {round} of {rounds}");
      // Pinned until the last round is done, free once she is out.
      assert_eq!(entities.ships["Carrier"].read().unwrap().pinned, round < rounds);
      if round < rounds {
        begin_round(&mut entities, &mut rng);
      }
    }
  }

  /// A pinned carrier coasts: her plan is not flown this round.
  #[tokio::test]
  async fn a_pinned_carrier_spends_no_thrust() {
    let design = carrier_design("Berth Carrier", 30, 0, 0);
    let mut entities = carrier_world(&design, 1).await;
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let burn = crate::ship::FlightPlan::new((Vec3::unit_x() * G * 2.0, 360).into(), None);
    entities.ships["Carrier"].write().unwrap().plan = burn;
    order(&mut entities, "Carrier", ShipAction::Launch { craft: names(1) });
    begin_round(&mut entities, &mut rng);
    entities.ships["Carrier"].write().unwrap().update();
    let carrier = entities.ships["Carrier"].read().unwrap();
    assert_eq!(carrier.get_velocity(), Vec3::zero(), "she did not burn");
    assert!(carrier.held_still);
  }

  /// A world with a carrier and a fighter in space beside her, `offset`
  /// metres away and `closing` m/s relative, with Land ordered.
  async fn landing_world(design: &Arc<ShipDesignTemplate>, offset: f64, closing: f64) -> Entities {
    let mut entities = carrier_world(design, 0).await;
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    entities.add_ship(
      "Fighter 1".to_string(),
      Vec3::unit_x() * offset,
      Vec3::unit_x() * closing,
      &fighter,
      None,
      None,
    );
    entities.ships["Fighter 1"].write().unwrap().team = Some(crate::ship::Team::Blue);
    order(
      &mut entities,
      "Fighter 1",
      ShipAction::Land {
        carrier: "Carrier".to_string(),
      },
    );
    entities
  }

  #[tokio::test]
  async fn a_deck_landing_needs_short_range_and_a_matched_velocity() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    // Too far: Medium range.
    let mut entities = landing_world(&design, 2_000_000.0, 0.0).await;
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("must be within Short"), "{effects:?}");
    assert!(entities.ships.contains_key("Fighter 1"));
    // Too fast: a heavy fighter (Thrust 9) may come in at up to 9 x 60 s of G.
    let limit = landing_velocity_limit(9);
    let mut entities = landing_world(&design, 100_000.0, limit + 1.0).await;
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("may come in at no more than"), "{effects:?}");
    // Just under: the check is made.
    let mut entities = landing_world(&design, 100_000.0, limit - 1.0).await;
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(
      lines(&effects)[0].contains("lands on") || lines(&effects)[0].contains("fails to land"),
      "{effects:?}"
    );
  }

  /// With skill 3 and no DMs a 2D roll of 3+ lands; the seeded run here is
  /// pinned to what it does, and the DMs are checked on the hover.
  #[tokio::test]
  async fn the_landing_check_counts_thrust_and_other_craft() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut entities = landing_world(&design, 100_000.0, 0.0).await;
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    // A second craft, of no squadron, coming in the same round.
    entities.add_ship(
      "Other".to_string(),
      Vec3::unit_y() * 100_000.0,
      Vec3::zero(),
      &fighter,
      None,
      None,
    );
    entities.ships["Other"].write().unwrap().team = Some(crate::ship::Team::Blue);
    order(
      &mut entities,
      "Other",
      ShipAction::Land {
        carrier: "Carrier".to_string(),
      },
    );
    // The carrier burned 2G this round and the fighter 3G.
    entities.ships["Carrier"].write().unwrap().plan =
      crate::ship::FlightPlan::new((Vec3::unit_x() * G * 2.0, 360).into(), None);
    entities.ships["Fighter 1"].write().unwrap().plan =
      crate::ship::FlightPlan::new((Vec3::unit_x() * G * 3.0, 360).into(), None);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    let roll = effects
      .iter()
      .find_map(|e| match e {
        EffectMsg::Message { ship, roll, .. } if ship.name == "Fighter 1" => roll.clone(),
        _ => None,
      })
      .expect("the fighter's check is reported");
    assert!(roll.contains("- craft's thrust 3"), "{roll}");
    assert!(roll.contains("- carrier's thrust 2"), "{roll}");
    assert!(roll.contains("- other craft landing 1"), "{roll}");
    assert!(roll.contains("vs 8"), "{roll}");
  }

  #[tokio::test]
  async fn a_failed_landing_hurts_both_ships_by_their_thrust() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut entities = landing_world(&design, 100_000.0, 0.0).await;
    // Pilot 0 and DM-8 from thrust: the check cannot pass.
    entities.ships["Carrier"].write().unwrap().plan =
      crate::ship::FlightPlan::new((Vec3::unit_x() * G * 4.0, 360).into(), None);
    entities.ships["Fighter 1"].write().unwrap().plan =
      crate::ship::FlightPlan::new((Vec3::unit_x() * G * 4.0, 360).into(), None);
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let hull_before = (
      entities.ships["Carrier"].read().unwrap().current_hull,
      entities.ships["Fighter 1"].read().unwrap().current_hull,
    );
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("fails to land"), "{effects:?}");
    let carrier_lost = hull_before.0 - entities.ships["Carrier"].read().unwrap().current_hull;
    assert!(
      (8..=48).contains(&carrier_lost) && carrier_lost % 8 == 0,
      "1D x 8: {carrier_lost}"
    );
    let fighter = entities.ships.get("Fighter 1").map(|s| s.read().unwrap().current_hull);
    // 22 hull: a roll of 3+ wrecks her, and then she is gone.
    assert!(fighter.is_none_or(|hull| hull == hull_before.1 - carrier_lost), "{fighter:?}");
    assert_eq!(entities.ships["Carrier"].read().unwrap().embarked.len(), 0);
  }

  #[tokio::test]
  async fn a_deck_takes_ten_a_round_and_the_rest_wait() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut entities = carrier_world(&design, 0).await;
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    let mut crew = crate::crew::Crew::new();
    crew.set_skill(crate::crew::Skills::Pilot, 4);
    for n in 1..=12 {
      entities.add_ship(
        format!("Fighter {n:02}"),
        Vec3::unit_x() * 50_000.0,
        Vec3::zero(),
        &fighter,
        Some(crew.clone()),
        None,
      );
      entities.ships[&format!("Fighter {n:02}")].write().unwrap().team = Some(crate::ship::Team::Blue);
      order(
        &mut entities,
        &format!("Fighter {n:02}"),
        ShipAction::Land {
          carrier: "Carrier".to_string(),
        },
      );
    }
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    let waits = lines(&effects).iter().filter(|l| l.contains("taken their 10")).count();
    assert_eq!(waits, 2, "{effects:?}");
    let landed = entities.ships["Carrier"].read().unwrap().embarked.len();
    let failed = lines(&effects).iter().filter(|l| l.contains("fails to land")).count();
    assert_eq!(landed + failed, 10, "{effects:?}");
  }

  #[tokio::test]
  async fn without_a_deck_both_are_pinned_and_there_is_no_check() {
    let design = carrier_design("Berth Carrier", 30, 0, 0);
    let mut entities = landing_world(&design, 100_000.0, 100.0).await;
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("begins docking"), "{effects:?}");
    assert!(entities.ships["Carrier"].read().unwrap().pinned);
    assert!(entities.ships["Fighter 1"].read().unwrap().pinned);
    assert_eq!(
      entities.ships["Fighter 1"].read().unwrap().get_velocity(),
      Vec3::zero(),
      "she matched the carrier's velocity"
    );
    let rounds = entities.ships["Carrier"].read().unwrap().flight_ops[0].rounds_left;
    for _ in 0..rounds {
      begin_round(&mut entities, &mut rng);
      let snapshot = entities.ship_deep_copy();
      end_round(&mut entities, &snapshot, &mut rng);
    }
    assert!(!entities.ships.contains_key("Fighter 1"));
    assert_eq!(aboard(&entities, "Carrier"), vec!["Fighter 1"]);
    assert!(!entities.ships["Carrier"].read().unwrap().pinned);
  }

  #[tokio::test]
  async fn a_round_aboard_rearms_from_the_carriers_stores() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut entities = carrier_world(&design, 1).await;
    {
      let mut carrier = entities.ships["Carrier"].write().unwrap();
      // The heavy fighter's rack holds 4 of its 60; fire them all off.
      let craft = &mut carrier.embarked[0];
      craft.loaded[1] = crate::ship::Magazine::default();
      craft.magazine = crate::ship::Magazine::default();
      carrier.embarked[0].rounds_aboard = 0;
    }
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(
      lines(&effects)
        .iter()
        .any(|l| l.contains("rearms Fighter 1: 4 missiles (96 left in stores)")),
      "{effects:?}"
    );
    let carrier = entities.ships["Carrier"].read().unwrap();
    assert_eq!(carrier.magazine.missiles, 96);
    assert_eq!(carrier.embarked[0].loaded(1).missiles, 4);
    assert_eq!(carrier.embarked[0].rounds_aboard, 1);
  }

  #[tokio::test]
  async fn a_craft_that_just_landed_is_not_yet_rearmed_and_stores_run_out() {
    let design = carrier_design("Deck Carrier", 30, 1, 1);
    let mut entities = landing_world(&design, 50_000.0, 0.0).await;
    {
      let mut carrier = entities.ships["Carrier"].write().unwrap();
      carrier.magazine.missiles = 3;
      let mut craft = entities.ships["Fighter 1"].write().unwrap();
      craft.loaded[1] = crate::ship::Magazine::default();
      craft.magazine = crate::ship::Magazine::default();
      let mut crew = crate::crew::Crew::new();
      crew.set_skill(crate::crew::Skills::Pilot, 4);
      craft.set_crew(crew);
      drop(craft);
      drop(carrier);
    }
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("lands on"), "{effects:?}");
    assert!(!lines(&effects).iter().any(|l| l.contains("rearms")), "{effects:?}");
    begin_round(&mut entities, &mut rng);
    let snapshot = entities.ship_deep_copy();
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(
      lines(&effects)
        .iter()
        .any(|l| l.contains("rearms Fighter 1: 3 missiles (0 left in stores)")),
      "{effects:?}"
    );
    let carrier = entities.ships["Carrier"].read().unwrap();
    assert_eq!(carrier.embarked[0].loaded(1).missiles, 3);
    assert_eq!(carrier.magazine.missiles, 0);
  }

  #[tokio::test]
  async fn a_full_hangar_counts_as_a_berth_and_a_full_deck_refuses() {
    let design = carrier_design("Deck Carrier", 1, 1, 1);
    let mut entities = carrier_world(&design, 1).await;
    assert!(!entities.ships["Carrier"].read().unwrap().has_berth_for(50));
    let fighter = get_ship_template("Heavy Fighter").unwrap();
    entities.add_ship(
      "Late".to_string(),
      Vec3::unit_x() * 50_000.0,
      Vec3::zero(),
      &fighter,
      None,
      None,
    );
    entities.ships["Late"].write().unwrap().team = Some(crate::ship::Team::Blue);
    order(
      &mut entities,
      "Late",
      ShipAction::Land {
        carrier: "Carrier".to_string(),
      },
    );
    let snapshot = entities.ship_deep_copy();
    let mut rng = SmallRng::seed_from_u64(0xD1CE);
    let effects = end_round(&mut entities, &snapshot, &mut rng);
    assert!(lines(&effects)[0].contains("no berth free"), "{effects:?}");
  }
}

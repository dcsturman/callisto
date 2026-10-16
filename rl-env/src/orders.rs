//! The orders that take more than one choice to carry out, worked out from
//! the ship's state each round: withdrawing by jump, the captain's boosts,
//! the computer's loadout, and the sensor operators' verbs.
//!
//! The network decides *whether*; this module decides *how*, from what is
//! true now. A withdrawal, for instance, is a standing order: while the
//! network keeps choosing it, each round does whatever is still outstanding
//! -- start Jump Control, burn out of the gravity well, have the engineers
//! shut systems down so the drive is powered the round the ship is clear,
//! then make the jump check. The ship's state is the order's memory.

use ai_ships::commander::REPAIR_PRIORITY;
use ai_ships::observation::Observation;
use callisto::entity::{Entity, Vec3, DELTA_TIME_F64, G};
use callisto::ship::{BridgeStation, PowerSystem, Ship};
use callisto::software::{Software, SoftwareKind};
use callisto::tactics::{Boost, EngineerVerb, Helm, SensorVerb, SoftwareOrder};
use cgmath::InnerSpace;

/// A ship must be this many of a body's radii from it to jump: 100
/// diameters.
pub const JUMP_RADII: f64 = 200.0;

/// What the captain's boosts go to, in the order the network ranks them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
  /// Every mount's attack.
  Guns,
  /// The pilot's dodge.
  Defence,
  /// The search for ships not yet found, and the sensor operators' actions.
  Detection,
}

/// Every ranking of the three, as the captain head numbers them.
pub const RANKINGS: [[Focus; 3]; 6] = [
  [Focus::Guns, Focus::Defence, Focus::Detection],
  [Focus::Guns, Focus::Detection, Focus::Defence],
  [Focus::Defence, Focus::Guns, Focus::Detection],
  [Focus::Defence, Focus::Detection, Focus::Guns],
  [Focus::Detection, Focus::Guns, Focus::Defence],
  [Focus::Detection, Focus::Defence, Focus::Guns],
];

/// The captain's boosts, most wanted first: each focus in turn, expanded into
/// the rolls it covers this round. The captain's check decides how many land.
#[must_use]
pub fn captain_boosts(ship: &Ship, view: &Observation, ranking: [Focus; 3], dodging: bool) -> Vec<Boost> {
  let blips: Vec<&str> = view.blips.iter().map(|blip| blip.name.as_str()).collect();
  ranking
    .iter()
    .flat_map(|focus| match focus {
      Focus::Guns => (0..ship.weapons().len()).map(|mount| Boost::Fire { mount }).collect::<Vec<_>>(),
      Focus::Defence => {
        if dodging {
          vec![Boost::Dodge]
        } else {
          vec![]
        }
      }
      Focus::Detection => blips
        .iter()
        .map(|target| Boost::Detection {
          target: (*target).to_string(),
        })
        .chain((0..ship.get_crew().sensor_operators()).map(|operator| Boost::Sensor { operator }))
        .collect(),
    })
    .collect()
}

/// What a sensor operator does, as the sensor-order heads number it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensorOrder {
  Lock,
  JamComms,
  JamMissiles,
}

pub const SENSOR_ORDERS: [SensorOrder; 3] = [SensorOrder::Lock, SensorOrder::JamComms, SensorOrder::JamMissiles];

/// A sensor operator's verb, if the order can be given: lock and jam comms
/// need a target.
#[must_use]
pub fn sensor_verb(order: SensorOrder, target: Option<&str>) -> Option<SensorVerb> {
  match (order, target) {
    (SensorOrder::Lock, Some(target)) => Some(SensorVerb::Lock {
      target: target.to_string(),
    }),
    (SensorOrder::JamComms, Some(target)) => Some(SensorVerb::JamComms {
      target: target.to_string(),
    }),
    (SensorOrder::JamMissiles, _) => Some(SensorVerb::JamMissiles),
    _ => None,
  }
}

/// Missiles in flight at one of this ship's side, which a jammer could spoil.
#[must_use]
pub fn missiles_inbound(view: &Observation) -> usize {
  let ours: Vec<&str> = view.own.iter().map(Entity::get_name).collect();
  view
    .missiles
    .iter()
    .filter(|missile| ours.contains(&missile.target.as_str()))
    .count()
}

/// How the computer is loaded, as the computer head numbers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Computer {
  /// The design's full loadout: the most protection, nothing spare, so no
  /// sensor hand-off either way.
  Fight,
  /// One point kept spare, by stopping the cheapest program that frees it,
  /// so the ship can receive a hand-off -- and send one, if her posture
  /// shares.
  Link,
}

/// What the computer should run: the design's loadout (as the ship comes up),
/// less for a link, or Jump Control first when withdrawing.
fn wanted_software(ship: &Ship, mode: Computer, withdrawing: bool) -> Vec<Software> {
  let capacity = ship.processing();
  let free: Vec<Software> = ship.software.iter().copied().filter(Software::always_running).collect();
  let mut paid: Vec<Software> = ship
    .software
    .iter()
    .copied()
    .filter(|package| !package.always_running() && package.kind != SoftwareKind::JumpControl)
    .collect();
  let mut wanted = free;
  let mut used = 0;
  if withdrawing {
    if let Some(jump) = ship
      .software
      .iter()
      .copied()
      .filter(|package| package.kind == SoftwareKind::JumpControl)
      .max_by_key(|package| package.level)
    {
      used += ship.bandwidth_cost(jump);
      wanted.push(jump);
    }
  }
  let mut loaded: Vec<Software> = Vec::new();
  for package in paid.drain(..) {
    let cost = ship.bandwidth_cost(package);
    if used + cost <= capacity {
      used += cost;
      loaded.push(package);
    }
  }
  if mode == Computer::Link && !withdrawing && used >= capacity {
    // Drop the cheapest program that frees a point.
    if let Some(cheapest) = loaded
      .iter()
      .enumerate()
      .filter(|(_, package)| ship.bandwidth_cost(**package) > 0)
      .min_by_key(|(_, package)| ship.bandwidth_cost(**package))
      .map(|(index, _)| index)
    {
      loaded.remove(cheapest);
    }
  }
  wanted.extend(loaded);
  wanted
}

/// The software orders that bring the computer to what is wanted: stops for
/// what should not run, starts for what should.
#[must_use]
pub fn software_orders(ship: &Ship, mode: Computer, withdrawing: bool) -> Vec<SoftwareOrder> {
  let wanted = wanted_software(ship, mode, withdrawing);
  let running = &ship.software_running;
  let stops = running
    .iter()
    .filter(|package| !package.always_running() && !wanted.iter().any(|w| w.kind == package.kind))
    .map(|package| SoftwareOrder {
      program: package.kind,
      running: false,
    });
  let starts = wanted
    .iter()
    .filter(|package| !running.iter().any(|r| r.kind == package.kind))
    .map(|package| SoftwareOrder {
      program: package.kind,
      running: true,
    });
  stops.chain(starts).collect()
}

/// Whether a ship could ever jump out of this fight: a working jump drive,
/// fuel for a jump-1, an engineer, and astrogation and the computer up.
/// Whether she is clear of gravity wells and powered is for the withdrawal
/// to see to.
#[must_use]
pub fn can_withdraw(ship: &Ship) -> bool {
  ship.design.jump > 0
    && ship.current_jump > 0
    // Fuel for a jump-1. (The engine's jump range counts Jump Control too,
    // which a withdrawal starts itself.)
    && ship.fuel_per_jump_number() > 0
    && ship.current_fuel >= ship.fuel_per_jump_number()
    && !ship.get_crew().engineers().is_empty()
    && ship.station_working(BridgeStation::Astrogation)
    && ship.station_working(BridgeStation::Computer)
    // Power enough for the drive, now or once the engineers have shut down
    // what they can.
    && (ship.jump_powered() || !shutdowns(ship).is_empty())
}

/// How far a ship still has to go to be clear to jump, and the body she is
/// clearing: none when she is clear already.
#[must_use]
pub fn well(ship: &Ship, view: &Observation) -> Option<(f64, Vec3)> {
  let here = ship.get_position();
  view
    .planets
    .iter()
    .map(|planet| {
      let centre = planet.get_position();
      (JUMP_RADII * planet.radius - (here - centre).magnitude(), centre)
    })
    .filter(|(short, _)| *short > 0.0)
    .max_by(|a, b| a.0.total_cmp(&b.0))
}

/// Rounds until the ship is clear of the well, burning straight out of it at
/// `thrust` (m/s²) on top of the speed she already has outward.
fn rounds_to_clear(short: f64, outward_speed: f64, thrust: f64) -> f64 {
  let time = if thrust > 0.0 {
    (-outward_speed + (outward_speed * outward_speed + 2.0 * thrust * short).sqrt()) / thrust
  } else if outward_speed > 0.0 {
    short / outward_speed
  } else {
    f64::INFINITY
  };
  time / DELTA_TIME_F64
}

/// The systems to shut down, in order, until the jump drive is powered:
/// weapons, then sensors, then the manoeuvre drive. Empty if it is powered.
#[must_use]
pub fn shutdowns(ship: &Ship) -> Vec<PowerSystem> {
  let Some(jump) = ship.power_line(PowerSystem::Jump) else {
    return vec![];
  };
  let lines = ship.power_lines();
  let running: u32 = lines.iter().filter(|line| !line.on_demand).map(|line| line.received).sum();
  let mut spare = ship.available_power().saturating_sub(running);
  let mut order: Vec<_> = lines
    .iter()
    .filter(|line| line.online && !line.on_demand && line.received > 0)
    .filter(|line| {
      matches!(
        line.system,
        PowerSystem::Weapon(_) | PowerSystem::Sensors | PowerSystem::Maneuver
      )
    })
    .collect();
  order.sort_by_key(|line| match line.system {
    PowerSystem::Weapon(_) => 0,
    PowerSystem::Sensors => 1,
    _ => 2,
  });
  let mut shut = Vec::new();
  for line in order {
    if spare >= jump.draw {
      break;
    }
    spare += line.received;
    shut.push(line.system);
  }
  if spare >= jump.draw {
    shut
  } else {
    // Not even everything would do it: nothing to be gained by trying.
    vec![]
  }
}

/// A withdrawal's orders for this round.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Withdrawal {
  /// The helm, while the ship is still inside a gravity well: straight out.
  pub helm: Option<Helm>,
  /// One verb per engineer.
  pub engineering: Vec<EngineerVerb>,
}

/// This round's part of jumping out.
#[must_use]
pub fn withdraw(ship: &Ship, view: &Observation) -> Withdrawal {
  let engineers = ship.get_crew().engineers().len();
  let repair = EngineerVerb::Repair {
    priority: REPAIR_PRIORITY.to_vec(),
  };
  let clearing = well(ship, view);
  let thrust = f64::from(ship.max_acceleration()) * G;
  let shut = shutdowns(ship);
  let needed_rounds = shut.len().div_ceil(engineers.max(1)) as f64 + 1.0;
  let (helm, rounds, coasting_clear) = match clearing {
    None => (None, 0.0, true),
    Some((short, centre)) => {
      let out = (ship.get_position() - centre).normalize();
      let outward_speed = ship.get_velocity().dot(out);
      // Aim well past the edge so the burn is still outward when she crosses.
      let target = ship.get_position() + out * (short * 1.5 + 1_000_000.0);
      // The drive can go off once she will coast clear in the time the other
      // shutdowns still need.
      let coasting = outward_speed > 0.0 && short / outward_speed / DELTA_TIME_F64 <= needed_rounds;
      (
        Some(Helm::Station {
          position: [target.x, target.y, target.z],
        }),
        rounds_to_clear(short, outward_speed, thrust),
        coasting,
      )
    }
  };
  let jump_ready = clearing.is_none() && shut.is_empty() && ship.jump_powered();
  let mut engineering = Vec::new();
  if jump_ready {
    engineering.push(EngineerVerb::Jump);
  } else if rounds <= needed_rounds {
    // Shut down what the jump needs, the drive only once she will be clear
    // without it.
    engineering.extend(
      shut
        .iter()
        .filter(|system| **system != PowerSystem::Maneuver || coasting_clear)
        .take(engineers)
        .map(|system| EngineerVerb::Power {
          system: *system,
          online: false,
        }),
    );
  }
  engineering.resize(engineers, repair);
  Withdrawal { helm, engineering }
}

/// When not withdrawing: bring back anything a withdrawal shut down, weapons
/// first, then repair.
#[must_use]
pub fn restore(ship: &Ship) -> Vec<EngineerVerb> {
  let engineers = ship.get_crew().engineers().len();
  let mut offline: Vec<PowerSystem> = ship
    .offline
    .iter()
    .copied()
    .filter(|system| matches!(system, PowerSystem::Weapon(_) | PowerSystem::Sensors | PowerSystem::Maneuver))
    .collect();
  offline.sort_by_key(|system| match system {
    PowerSystem::Weapon(_) => 0,
    PowerSystem::Maneuver => 1,
    _ => 2,
  });
  let mut verbs: Vec<EngineerVerb> = offline
    .into_iter()
    .take(engineers)
    .map(|system| EngineerVerb::Power { system, online: true })
    .collect();
  verbs.resize(
    engineers.max(1),
    EngineerVerb::Repair {
      priority: REPAIR_PRIORITY.to_vec(),
    },
  );
  verbs
}

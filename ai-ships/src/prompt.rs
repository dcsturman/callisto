//! What the model is told.
//!
//! Two parts. The system prompt, `prompt/system.md`, is fixed: the rules that
//! decide tactics, what the model will be shown, and how to answer. It is
//! Markdown in its own file so it can be read and rewritten without touching
//! Rust, and it is the part both providers cache. [`situation`] renders an
//! [`Observation`] into the round's message: the side's orders and plan, its
//! ships, its contacts, and the odds and exposure tables -- the human screen,
//! in words and numbers, nothing the screen would not show.

use std::fmt::Write;

use callisto::entity::{Entity, Vec3, DELTA_TIME_F64};
use callisto::ship::{Ship, ShipSystem, Weapon, WeaponType};
use callisto::tactics::{band_at, mount_card, BandOdds, Posture};
use cgmath::InnerSpace;

use crate::observation::{Contact, Engagement, Exposure, Observation};

/// The fixed part of the prompt.
#[must_use]
pub fn system() -> &'static str {
  include_str!("prompt/system.md")
}

/// What a commander remembers between rounds, so the fight so far can count
/// what is gone as well as what is left: every ship the side started with,
/// and every enemy it has ever held.
#[derive(Debug, Clone, Default)]
pub struct Memory {
  /// Our ships, with the hull each was built with, in the order first seen.
  pub roster: Vec<(String, u32)>,
  /// Enemies ever in contact, with the hull each class is built with.
  pub enemies: Vec<(String, u32)>,
}

impl Memory {
  /// Take in this round's view.
  pub fn update(&mut self, view: &Observation) {
    for ship in &view.own {
      if !self.roster.iter().any(|(name, _)| name == ship.get_name()) {
        self.roster.push((ship.get_name().to_string(), ship.design.hull));
      }
    }
    for contact in view.enemies() {
      if !self.enemies.iter().any(|(name, _)| *name == contact.name) {
        self.enemies.push((contact.name.clone(), contact.design_hull));
      }
    }
  }
}

/// This round's message: everything the side may know, for the model.
#[must_use]
pub fn situation(view: &Observation, plan: Option<&str>, memory: &Memory) -> String {
  let mut out = String::new();
  heading(&mut out, view);
  status(&mut out, view, memory);
  orders(&mut out, view);
  let _ = writeln!(out, "## Your plan from last round\n\n{}\n", plan.unwrap_or("(none yet)"));
  let _ = writeln!(out, "## Your ships\n");
  for ship in &view.own {
    own_ship(&mut out, view, ship);
  }
  stealth(&mut out, view);
  hunting(&mut out, view);
  allies(&mut out, view);
  contacts(&mut out, view);
  blips(&mut out, view);
  odds(&mut out, view);
  exposures(&mut out, view);
  missiles(&mut out, view);
  planets(&mut out, view);
  out
}

fn heading(out: &mut String, view: &Observation) {
  let names: Vec<&str> = view.own.iter().map(Entity::get_name).collect();
  let side = view
    .team
    .map_or_else(|| "no team".to_string(), |team| format!("the {team:?} side"));
  let _ = writeln!(out, "# Round {}. You command {side}: {}.\n", view.round, names.join(", "));
}

/// How the fight stands, first: what each side has left, as far as we can
/// see. Whether it is going against us is read here, not inferred.
fn status(out: &mut String, view: &Observation, memory: &Memory) {
  let _ = writeln!(out, "## The fight so far\n");
  let alive = |name: &str| view.own.iter().any(|ship| ship.get_name() == name);
  let started: u32 = memory.roster.iter().map(|(_, hull)| hull).sum::<u32>().max(1);
  let left: u32 = view.own.iter().map(|ship| ship.current_hull).sum();
  let lost: Vec<&str> = memory
    .roster
    .iter()
    .filter(|(name, _)| !alive(name))
    .map(|(name, _)| name.as_str())
    .collect();
  let ships: Vec<String> = view
    .own
    .iter()
    .map(|ship| {
      let risk = at_risk(view, ship);
      format!(
        "{} {}%{}{}{}",
        ship.get_name(),
        100 * ship.current_hull / ship.design.hull.max(1),
        if ship.current_maneuver == 0 {
          " (cannot manoeuvre)"
        } else {
          ""
        },
        if risk > 0.0 {
          format!(
            " (if every enemy in reach shot at her, they could take {} of what she has left next round)",
            percent(risk.min(1.0))
          )
        } else {
          String::new()
        },
        eyes_on(view, ship)
      )
    })
    .collect();
  let _ = writeln!(
    out,
    "- Your side: {} of {} ships left, {}% of the hull it started with. {}",
    view.own.len(),
    memory.roster.len().max(view.own.len()),
    100 * left / started,
    ships.join("; ")
  );
  if !lost.is_empty() {
    let _ = writeln!(out, "- Your ships lost (destroyed, or jumped out): {}", lost.join(", "));
  }
  let enemies = view.enemies();
  let gone: Vec<&str> = memory
    .enemies
    .iter()
    .filter(|(name, _)| !enemies.iter().any(|contact| contact.name == *name))
    .map(|(name, _)| name.as_str())
    .collect();
  if enemies.is_empty() {
    let _ = writeln!(out, "- Enemy: none in contact.");
  } else {
    let seen: Vec<String> = enemies
      .iter()
      .map(|contact| {
        let severity: u32 = contact.crit_level.iter().map(|level| u32::from(*level)).sum();
        let state = if contact.current_maneuver == 0 {
          "DISABLED, cannot manoeuvre".to_string()
        } else if severity == 0 {
          "no visible damage".to_string()
        } else {
          format!("damaged, criticals of total severity {severity}")
        };
        format!("{} ({state})", contact.name)
      })
      .collect();
    let _ = writeln!(
      out,
      "- Enemy in contact: {} ships: {}. Their hull points are not visible.",
      enemies.len(),
      seen.join(", ")
    );
  }
  if !gone.is_empty() {
    let _ = writeln!(
      out,
      "- Enemies once held and no longer in contact (destroyed, jumped, or out of sight): {}",
      gone.join(", ")
    );
  }
  out.push('\n');
}

/// "Eyes on us": who has found this ship, as her sensor station shows it.
/// For a stealthed ship that nobody has found, and who is giving herself
/// away, it says so: that is the moment the choice matters.
fn eyes_on(view: &Observation, ship: &Ship) -> String {
  let watchers: Vec<String> = view
    .watchers
    .iter()
    .filter(|watcher| watcher.ours == ship.get_name())
    .map(|watcher| {
      if watcher.lock {
        format!("{} (locked on)", watcher.by)
      } else {
        watcher.by.clone()
      }
    })
    .collect();
  if !watchers.is_empty() {
    return format!("; found by {}", watchers.join(", "));
  }
  if ship.design.stealth.is_none() {
    return "; no enemy has found her".to_string();
  }
  let loud = ship.active_sensors || ship.is_transmitting();
  if loud {
    "; NO ENEMY HAS FOUND HER, but she is stealthed and radiating: see her stealth line before keeping her loud"
      .to_string()
  } else {
    "; no enemy has found her: she is hidden".to_string()
  }
}

/// The share of what a ship has left that the contacts could take next
/// round, each firing from the band it is in now: the sum of their expected
/// damage at that band over her remaining hull.
fn at_risk(view: &Observation, ship: &Ship) -> f64 {
  let hull = f64::from(ship.current_hull.max(1));
  view
    .contacts
    .iter()
    .filter_map(|contact| {
      let band = band_at((contact.position - ship.get_position()).magnitude());
      view
        .engagement(ship.get_name(), &contact.name)?
        .they_hit
        .iter()
        .find(|odds| odds.band == band)
        .map(|odds| odds.expected_damage)
    })
    .sum::<f64>()
    / hull
}

/// Each ship's standing orders, with ships under the same orders listed
/// together so a squadron's doctrine is read once.
fn orders(out: &mut String, view: &Observation) {
  let _ = writeln!(out, "## Standing orders\n");
  let mut groups: Vec<(&str, Vec<&str>)> = Vec::new();
  for ship in &view.own {
    let text = ship.orders.as_deref().unwrap_or("(none given)");
    match groups.iter_mut().find(|(orders, _)| *orders == text) {
      Some((_, ships)) => ships.push(ship.get_name()),
      None => groups.push((text, vec![ship.get_name()])),
    }
  }
  for (text, ships) in groups {
    let _ = writeln!(out, "- {}: {text}", ships.join(", "));
  }
  out.push('\n');
}

/// Thousands-separated kilometres from metres.
fn km(metres: f64) -> String {
  #[allow(clippy::cast_possible_truncation)]
  let whole = (metres / 1000.0).round() as i64;
  let digits = whole.abs().to_string();
  let mut grouped = String::new();
  for (i, digit) in digits.chars().enumerate() {
    if i > 0 && (digits.len() - i).is_multiple_of(3) {
      grouped.push(',');
    }
    grouped.push(digit);
  }
  if whole < 0 {
    grouped.insert(0, '-');
  }
  grouped
}

fn vec_km(v: Vec3) -> String {
  format!("[{}, {}, {}]", km(v.x), km(v.y), km(v.z))
}

fn vec_kms(v: Vec3) -> String {
  format!("[{:.1}, {:.1}, {:.1}]", v.x / 1000.0, v.y / 1000.0, v.z / 1000.0)
}

fn percent(p: f64) -> String {
  format!("{:.0}%", (p * 100.0).max(0.0))
}

fn crits(crit_level: &[u8; 11]) -> String {
  let listed: Vec<String> = crit_level
    .iter()
    .enumerate()
    .filter(|(_, level)| **level > 0)
    .filter_map(|(index, level)| ShipSystem::from_repr(index).map(|system| format!("{system:?} {level}")))
    .collect();
  if listed.is_empty() {
    "none".to_string()
  } else {
    listed.join(", ")
  }
}

/// One mount as a gunner's card: `#0 barbette: 1x Particle (4D x3 per hit,
/// reaches Very Long, to-hit +0, radiation)`.
fn mount_line(mount: usize, weapon: &Weapon, gunner: Option<u8>) -> String {
  let card = mount_card(weapon);
  let kinds: Vec<String> = weapon.guns.iter().map(|gun| format!("{:?}", gun.kind)).collect();
  let guns = if weapon.is_uniform() {
    format!("{}x {}", weapon.guns.len(), kinds[0])
  } else {
    kinds.join("/")
  };
  let mut facts: Vec<String> = Vec::new();
  if let Some((kind, profile)) = &card.attack {
    let dice = if card.multiple > 1 {
      format!("{}D x{} per hit", profile.damage_dice, card.multiple)
    } else {
      format!("{}D per hit", profile.damage_dice)
    };
    facts.push(if card.guns > 1 {
      format!("{dice}, {} guns", card.guns)
    } else {
      dice
    });
    facts.push(match profile.max_range {
      Some(range) => format!("reaches {range}"),
      None => "flies to its target at any range".to_string(),
    });
    if profile.salvo.is_none() {
      facts.push(format!("to-hit {:+}", profile.hit_mod));
    }
    if profile.ap > 0 {
      facts.push(format!("AP {}", profile.ap));
    }
    if profile.radiation {
      facts.push("radiation".to_string());
    }
    if profile.ion {
      facts.push("drains power, not hull".to_string());
    }
    if *kind == WeaponType::Missile || *kind == WeaponType::Torpedo {
      facts.push("a launch is seen by everyone".to_string());
    }
  }
  if card.point_defence {
    facts.push("can do point defence".to_string());
  }
  if card.battery {
    facts.push("point-defence battery, automatic".to_string());
  }
  if card.sand {
    facts.push("sandcaster, automatic".to_string());
  }
  let mount_kind = format!("{:?}", weapon.mount).to_lowercase();
  let gunner = gunner.map_or_else(String::new, |skill| format!(", gunner {skill}"));
  format!("#{mount} {mount_kind}: {guns} ({}){gunner}", facts.join(", "))
}

fn weapon_list(ship: &Ship) -> String {
  ship
    .weapons()
    .iter()
    .enumerate()
    .map(|(mount, weapon)| mount_line(mount, weapon, Some(ship.get_crew().get_gunnery(mount))))
    .collect::<Vec<_>>()
    .join("; ")
}

fn own_ship(out: &mut String, view: &Observation, ship: &Ship) {
  let design = &ship.design;
  let _ = writeln!(
    out,
    "### {} ({}, {} tons, TL {}{})\n",
    ship.get_name(),
    design.name,
    design.displacement,
    design.tl,
    design
      .countermeasures
      .map_or_else(String::new, |cm| format!(", {cm:?} countermeasures"))
  );
  let _ = writeln!(
    out,
    "- Hull {}/{}, armour {}, power {}/{}, thrust {} G of {}, jump {}, fuel {}/{}",
    ship.current_hull,
    design.hull,
    ship.get_current_armor(),
    ship.current_power,
    design.power,
    ship.max_acceleration(),
    design.maneuver,
    ship.current_jump,
    ship.current_fuel,
    design.fuel
  );
  let burn = ship.plan.0 .0;
  let burning = if burn.magnitude() < 1e-6 {
    "coasting".to_string()
  } else {
    format!(
      "burning {:.1} G toward [{:.2}, {:.2}, {:.2}]",
      burn.magnitude() / callisto::entity::G,
      burn.x / burn.magnitude(),
      burn.y / burn.magnitude(),
      burn.z / burn.magnitude()
    )
  };
  let _ = writeln!(
    out,
    "- Position (km) {}; velocity (km/s) {}; {burning}",
    vec_km(ship.get_position()),
    vec_kms(ship.get_velocity())
  );
  let posture = match (ship.active_sensors, ship.handoff_sensors, ship.transmitting) {
    (true, _, _) => "Active (sweeping; loud)",
    (false, true, _) | (false, false, true) => "Sharing (transmitting; loud)",
    (false, false, false) => "Silent",
  };
  let _ = writeln!(
    out,
    "- Posture {posture}; dodge reserve {} G (each dodge is -{} to the attacker, the pilot's skill); sensors {:?}{}",
    ship.get_dodge_thrust(),
    ship.get_crew().get_pilot(),
    ship.current_sensors,
    design
      .stealth
      .map_or_else(String::new, |stealth| format!("; stealth {stealth:?}"))
  );
  jump_line(out, ship);
  stealth_line(out, view, ship);
  let _ = writeln!(
    out,
    "- Contacts held: {}; locked: {}",
    list(&ship.contacts),
    list(&ship.sensor_locks)
  );
  let _ = writeln!(out, "- Weapons: {}", weapon_list(ship));
  let magazine = &ship.magazine;
  if magazine.missiles + magazine.torpedoes + magazine.sand > 0 {
    let _ = writeln!(
      out,
      "- Magazine: {} missiles, {} torpedoes, {} sand",
      magazine.missiles, magazine.torpedoes, magazine.sand
    );
  }
  let crew = ship.get_crew();
  let engineers: Vec<String> = crew
    .engineers()
    .iter()
    .map(|e| format!("drive {} power {} mechanic {}", e.maneuver, e.power, e.mechanic))
    .collect();
  let _ = writeln!(
    out,
    "- Crew: pilot {}, sensor operators {:?}, engineers [{}], leadership {}",
    crew.get_pilot(),
    crew.sensor_skills(),
    engineers.join("; "),
    crew.get_leadership()
  );
  computer_line(out, ship);
  let _ = writeln!(out, "- Damage: {}\n", crits(&ship.crit_level));
}

/// For a stealthed ship, what each posture costs her this round against the
/// enemy most likely to find her.
fn stealth_line(out: &mut String, view: &Observation, ship: &Ship) {
  let Some(stealth) = ship.design.stealth else {
    return;
  };
  let found_by: Vec<&str> = view
    .watchers
    .iter()
    .filter(|watcher| watcher.ours == ship.get_name())
    .map(|watcher| watcher.by.as_str())
    .collect();
  if !found_by.is_empty() {
    let odds: Vec<String> = found_by
      .iter()
      .map(|by| {
        view
          .exposures
          .iter()
          .find(|exposure| exposure.ours == ship.get_name() && exposure.contact == *by)
          .map_or_else(
            || format!("{by} unknown (you have not found her)"),
            |exposure| {
              let slip = exposure
                .odds
                .iter()
                .find(|odds| odds.posture == Posture::Silent && !odds.firing)
                .map_or(0.0, |odds| odds.slip_by_opening);
              format!("{by} {}", percent(slip))
            },
          )
      })
      .collect();
    let _ = writeln!(
      out,
      "- Stealth {stealth:?}, but she has been found. Opening the range by one band would make each lose her: {}. That is how she gets her stealth back: open a band, then go silent on a light burn so they do not find her again. Until then a thrust cap hides nothing: burn as hard as the fight needs.",
      odds.join(", ")
    );
    return;
  }
  let worst = view
    .exposures
    .iter()
    .filter(|exposure| exposure.ours == ship.get_name())
    .max_by(|a, b| found(a, Posture::Active, false).total_cmp(&found(b, Posture::Active, false)));
  let Some(worst) = worst else {
    let _ = writeln!(out, "- Stealth {stealth:?}: no enemy in contact to hide from yet.");
    return;
  };
  let _ = writeln!(
    out,
    "- Stealth {stealth:?}. {} is the likeliest to find her this round: silent {}, silent and firing {}, sharing {}, active (needed for a sensor lock) {}.",
    worst.contact,
    percent(found(worst, Posture::Silent, false)),
    percent(found(worst, Posture::Silent, true)),
    percent(found(worst, Posture::Passive, false)),
    percent(found(worst, Posture::Active, false))
  );
}

fn found(exposure: &Exposure, posture: Posture, firing: bool) -> f64 {
  exposure
    .odds
    .iter()
    .find(|odds| odds.posture == posture && odds.firing == firing)
    .map_or(0.0, |odds| odds.found)
}

/// Whether the ship can jump out, and if not, why.
fn jump_line(out: &mut String, ship: &Ship) {
  let design = &ship.design;
  let jump = if design.jump == 0 {
    "no jump drive".to_string()
  } else if ship.can_jump() && ship.jump_range_available() > 0 {
    format!("clear to jump (jump-{} with the fuel aboard)", ship.jump_range_available())
  } else if ship.running_level(callisto::software::SoftwareKind::JumpControl).is_none() {
    "cannot jump until Jump Control is running".to_string()
  } else if ship.jump_range_available() == 0 {
    "cannot jump: no fuel for it".to_string()
  } else {
    "cannot jump: inside a body's 100-diameter limit, or the drive is down".to_string()
  };
  let _ = writeln!(out, "- Jump: {jump}");
}

/// The computer: its bandwidth, what runs, and what is aboard but off.
fn computer_line(out: &mut String, ship: &Ship) {
  let describe = |package: &callisto::software::Software| {
    if package.always_running() {
      None
    } else {
      Some(format!(
        "{}/{} ({} bandwidth)",
        package.kind.label(),
        package.level,
        ship.bandwidth_cost(*package)
      ))
    }
  };
  let running: Vec<String> = ship.software_running.iter().filter_map(describe).collect();
  let off: Vec<String> = ship
    .software
    .iter()
    .filter(|package| !ship.software_running.contains(package))
    .filter_map(describe)
    .collect();
  let spare = ship.bandwidth_spare();
  let _ = writeln!(
    out,
    "- Computer: {} bandwidth, {} in use, {spare} spare ({}). Running: {}. Aboard but not running: {}",
    ship.bandwidth_used() + spare,
    ship.bandwidth_used(),
    if spare >= 1 {
      "enough to send or receive a sensor hand-off"
    } else {
      "NONE: this ship can neither send nor receive a sensor hand-off until a program is stopped"
    },
    if running.is_empty() {
      "nothing beyond the basics".to_string()
    } else {
      running.join(", ")
    },
    if off.is_empty() {
      "nothing".to_string()
    } else {
      off.join(", ")
    }
  );
}

fn list(names: &[String]) -> String {
  if names.is_empty() {
    "none".to_string()
  } else {
    names.join(", ")
  }
}

fn allies(out: &mut String, view: &Observation) {
  if view.allies.is_empty() {
    return;
  }
  let _ = writeln!(out, "## Friendly ships not under your command\n");
  for ally in &view.allies {
    let (nearest, distance) = nearest_of(view, ally.get_position());
    let _ = writeln!(
      out,
      "- {} ({}): hull {}/{}, at {} km from {nearest}, {}",
      ally.get_name(),
      ally.design.name,
      ally.current_hull,
      ally.design.hull,
      km(distance),
      band_at(distance)
    );
  }
  out.push('\n');
}

/// Our nearest ship to a point, and how far.
fn nearest_of(view: &Observation, point: Vec3) -> (String, f64) {
  view
    .own
    .iter()
    .map(|ship| (ship.get_name().to_string(), (ship.get_position() - point).magnitude()))
    .min_by(|a, b| a.1.total_cmp(&b.1))
    .unwrap_or_else(|| ("nobody".to_string(), 0.0))
}

/// How to fight a stealthed ship, for a side that has one. Stealth is rare,
/// so this is only said when it applies, not in every prompt.
fn stealth(out: &mut String, view: &Observation) {
  let hidden: Vec<&str> = view
    .own
    .iter()
    .filter(|ship| ship.design.stealth.is_some())
    .map(Entity::get_name)
    .collect();
  if hidden.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## Stealth: {}\n\n{}",
    hidden.join(", "),
    include_str!("prompt/stealth.md")
  );
}

/// How to hunt a stealthed enemy, for a side that is likely facing one: a
/// stealthed contact, or a blip inside Very Long that nobody has found --
/// an ordinary hull that close would have been.
fn hunting(out: &mut String, view: &Observation) {
  let stealthed: Vec<&str> = view
    .enemies()
    .into_iter()
    .filter(|contact| contact.stealth.is_some())
    .map(|contact| contact.name.as_str())
    .collect();
  let unfound: Vec<&str> = view
    .blips
    .iter()
    // Nobody has looked yet in the opening round: the first detection pass
    // runs at its end.
    .filter(|blip| view.round > 1 && blip.distance < 50_000_000.0)
    .map(|blip| blip.name.as_str())
    .collect();
  if stealthed.is_empty() && unfound.is_empty() {
    return;
  }
  let mut who = Vec::new();
  if !stealthed.is_empty() {
    who.push(format!("{} (stealthed)", stealthed.join(", ")));
  }
  if !unfound.is_empty() {
    who.push(format!("{} (inside sensor range and still not found)", unfound.join(", ")));
  }
  let _ = writeln!(out, "## Hunting: {}\n\n{}", who.join("; "), include_str!("prompt/hunting.md"));
}

fn blips(out: &mut String, view: &Observation) {
  if view.blips.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## Blips: ships on the plot you have not detected\n\nYou can see that something is there and how it is moving, nothing more. You can steer toward or away from a blip (intercept, hold_range, open_range); you cannot shoot, lock or jam it until one of your ships detects it.\n"
  );
  for blip in &view.blips {
    let _ = writeln!(
      out,
      "- {}: {} km from {} ({}); position (km) {}; velocity (km/s) {}",
      blip.name,
      km(blip.distance),
      blip.nearest,
      band_at(blip.distance),
      vec_km(blip.position),
      vec_kms(blip.velocity)
    );
  }
  out.push('\n');
}

fn contacts(out: &mut String, view: &Observation) {
  let enemies = view.enemies();
  if enemies.is_empty() {
    let _ = writeln!(
      out,
      "## Enemy contacts\n\nNone. No enemy ship is in contact with any of yours.\n"
    );
    return;
  }
  let _ = writeln!(out, "## Enemy contacts\n");
  for contact in enemies {
    contact_block(out, view, contact);
  }
}

fn contact_block(out: &mut String, view: &Observation, contact: &Contact) {
  let team = contact
    .team
    .map_or_else(|| "no team".to_string(), |team| format!("{team:?} team"));
  let _ = writeln!(
    out,
    "### {} ({}, {} tons, TL {}, {team}; built with hull {}, armour {}{}{})\n",
    contact.name,
    contact.design,
    contact.displacement,
    contact.tl,
    contact.design_hull,
    contact.design_armor,
    contact
      .countermeasures
      .map_or_else(String::new, |cm| format!(", {cm:?} countermeasures")),
    contact
      .stealth
      .map_or_else(String::new, |stealth| format!(", stealth {stealth:?}"))
  );
  let nearest = view.own.iter().find(|ship| ship.get_name() == contact.nearest);
  let _ = writeln!(out, "{}", approach_line(contact, nearest));
  let _ = writeln!(
    out,
    "- Position (km) {}; velocity (km/s) {}; thrust {} G; sensors {:?}",
    vec_km(contact.position),
    vec_kms(contact.velocity),
    contact.current_maneuver,
    contact.sensors
  );
  let weapons: Vec<String> = contact
    .weapons
    .iter()
    .enumerate()
    .map(|(mount, weapon)| mount_line(mount, weapon, None))
    .collect();
  let _ = writeln!(out, "- Weapons: {}", weapons.join("; "));
  let _ = writeln!(
    out,
    "- Visible damage: {}{}",
    crits(&contact.crit_level),
    if contact.current_maneuver == 0 {
      "; DISABLED: no thrust, cannot manoeuvre or dodge"
    } else {
      ""
    }
  );
  let _ = writeln!(out, "- Held as a contact by: {}\n", contact.held_by.join(", "));
}

/// Range now, and where it goes if nobody changes course: from the relative
/// motion, so two ships that will pass each other are said to pass, with how
/// close, rather than to reach zero.
fn approach_line(contact: &Contact, nearest: Option<&Ship>) -> String {
  let Some(ship) = nearest else {
    return format!("- {} km, {}", km(contact.distance), contact.band);
  };
  let r = contact.position - ship.get_position();
  let v = contact.velocity - ship.get_velocity();
  let after = (r + v * DELTA_TIME_F64).magnitude();
  let speed = v.magnitude();
  let rate = -(r.dot(v)) / r.magnitude().max(1.0) * DELTA_TIME_F64;
  let motion = if rate.abs() < 500.0 {
    "range steady".to_string()
  } else if rate > 0.0 {
    format!("closing {} km per round", km(rate))
  } else {
    format!("opening {} km per round", km(-rate))
  };
  let closest_at = if speed > 1e-6 {
    -(r.dot(v)) / (speed * speed)
  } else {
    0.0
  };
  let pass = if closest_at > 0.0 && closest_at < DELTA_TIME_F64 {
    let closest = (r + v * closest_at).magnitude();
    format!(
      "; on present courses you pass within {} km ({}) this round and end it {} km apart ({})",
      km(closest),
      band_at(closest),
      km(after),
      band_at(after)
    )
  } else {
    format!("; next round about {} km ({}) if nobody burns", km(after), band_at(after))
  };
  let ours = ship.max_acceleration();
  let theirs = contact.current_maneuver;
  let race = match ours.cmp(&theirs) {
    std::cmp::Ordering::Greater => format!(
      "her thrust {theirs} G against {}'s {ours} G: you can hold the range open, or close it, as you choose",
      contact.nearest
    ),
    std::cmp::Ordering::Less => format!(
      "her thrust {theirs} G against {}'s {ours} G: she can close on you or run, and you cannot stop her",
      contact.nearest
    ),
    std::cmp::Ordering::Equal => format!("her thrust {theirs} G, the same as {}'s", contact.nearest),
  };
  format!(
    "- From {} (your nearest): {} km, {}; {motion}{pass}. {race}",
    contact.nearest,
    km(contact.distance),
    contact.band
  )
}

fn odds(out: &mut String, view: &Observation) {
  if view.engagements.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## Odds per round, flying straight\n\nEach cell is `you / them`: your best gun's chance to hit and the share of their built hull your guns take in a round, then theirs against your hull as it stands. `-` is out of reach; `m` means missiles only. Enemy gunners, whom you cannot see, are assumed skill 2.\n"
  );
  let _ = writeln!(out, "| You vs them | Short | Medium | Long | Very Long |");
  let _ = writeln!(out, "|---|---|---|---|---|");
  for engagement in &view.engagements {
    odds_row(out, view, engagement);
  }
  out.push('\n');
}

fn odds_row(out: &mut String, view: &Observation, engagement: &Engagement) {
  let ours = view.own.iter().find(|ship| ship.get_name() == engagement.ours);
  let contact = view.contacts.iter().find(|contact| contact.name == engagement.contact);
  let (Some(ours), Some(contact)) = (ours, contact) else {
    return;
  };
  let their_hull = f64::from(contact.design_hull.max(1));
  let our_hull = f64::from(ours.current_hull.max(1));
  let cells: Vec<String> = engagement
    .we_hit
    .iter()
    .zip(&engagement.they_hit)
    .filter(|(we, _)| we.band != callisto::ship::Range::Distant)
    .map(|(we, they)| format!("{} / {}", odds_cell(we, their_hull), odds_cell(they, our_hull)))
    .collect();
  let _ = writeln!(out, "| {} vs {} | {} |", ours.get_name(), contact.name, cells.join(" | "));
}

/// `72% 45%`: chance to hit, and the share of the hull taken per round.
fn odds_cell(odds: &BandOdds, hull: f64) -> String {
  if odds.mounts == 0 {
    "-".to_string()
  } else {
    format!(
      "{} {}{}",
      percent(odds.best_chance),
      percent(odds.expected_damage / hull),
      if odds.missiles_only { "m" } else { "" }
    )
  }
}

/// How findable each of our ships is, but only where it depends on what she
/// does: an ordinary hull is found whatever her posture, and a line saying so
/// for every pairing is a page of 100%.
fn exposures(out: &mut String, view: &Observation) {
  let informative: Vec<&Exposure> = view
    .exposures
    .iter()
    .filter(|exposure| found(exposure, Posture::Silent, false) < 0.95 || found(exposure, Posture::Active, true) < 0.95)
    .collect();
  if informative.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## How likely each contact is to find your ship this round, by posture\n\nThe second figure is if the ship also fires.\n"
  );
  for exposure in informative {
    exposure_line(out, exposure);
  }
  out.push('\n');
}

fn exposure_line(out: &mut String, exposure: &Exposure) {
  let find = |posture: Posture, firing: bool| {
    exposure
      .odds
      .iter()
      .find(|odds| odds.posture == posture && odds.firing == firing)
  };
  let cell = |posture: Posture| {
    let quiet = find(posture, false).map_or(0.0, |odds| odds.found);
    let loud = find(posture, true).map_or(0.0, |odds| odds.found);
    format!("{} ({})", percent(quiet), percent(loud))
  };
  let slip = find(Posture::Silent, false).map_or(0.0, |odds| odds.slip_by_opening);
  let _ = writeln!(
    out,
    "- {} as seen by {}: silent {}, sharing {}, active {}{}",
    exposure.ours,
    exposure.contact,
    cell(Posture::Silent),
    cell(Posture::Passive),
    cell(Posture::Active),
    if slip > 0.0 {
      format!("; opening the range by a band makes them lose contact {}", percent(slip))
    } else {
      String::new()
    }
  );
}

fn missiles(out: &mut String, view: &Observation) {
  if view.missiles.is_empty() {
    return;
  }
  let _ = writeln!(out, "## Missiles in flight\n");
  for missile in &view.missiles {
    let target = view.own.iter().find(|ship| ship.get_name() == missile.target);
    let inbound = target.is_some();
    let distance = target.map_or_else(
      || nearest_of(view, missile.get_position()).1,
      |ship| (ship.get_position() - missile.get_position()).magnitude(),
    );
    let _ = writeln!(
      out,
      "- {} from {} at {}: {} km {}, {} rounds of fuel left",
      if inbound { "Inbound" } else { "Outbound" },
      missile.source,
      missile.target,
      km(distance),
      if inbound {
        "from its target"
      } else {
        "from your nearest ship"
      },
      missile.burns
    );
  }
  out.push('\n');
}

fn planets(out: &mut String, view: &Observation) {
  if view.planets.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## Bodies\n\nNo ship can jump within 100 diameters of a body, which is 200 radii from its centre.\n"
  );
  for planet in &view.planets {
    let (nearest, distance) = nearest_of(view, planet.get_position());
    let limit = planet.radius * 200.0;
    let _ = writeln!(
      out,
      "- {}: radius {} km at {}; surface {} km from {nearest}; jump limit {} km from its centre, so {nearest} is {}",
      planet.get_name(),
      km(planet.radius),
      vec_km(planet.get_position()),
      km((distance - planet.radius).max(0.0)),
      km(limit),
      if distance > limit {
        "clear of it".to_string()
      } else {
        format!("{} km inside it", km(limit - distance))
      }
    );
  }
  out.push('\n');
}

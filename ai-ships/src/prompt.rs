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

/// This round's message: everything the side may know, for the model.
#[must_use]
pub fn situation(view: &Observation, plan: Option<&str>) -> String {
  let mut out = String::new();
  heading(&mut out, view);
  orders(&mut out, view);
  let _ = writeln!(out, "## Your plan from last round\n\n{}\n", plan.unwrap_or("(none yet)"));
  let _ = writeln!(out, "## Your ships\n");
  for ship in &view.own {
    own_ship(&mut out, ship);
  }
  allies(&mut out, view);
  contacts(&mut out, view);
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

fn orders(out: &mut String, view: &Observation) {
  let _ = writeln!(out, "## Standing orders\n");
  for ship in &view.own {
    let _ = writeln!(
      out,
      "- {}: {}",
      ship.get_name(),
      ship.orders.as_deref().unwrap_or("(none given)")
    );
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

fn own_ship(out: &mut String, ship: &Ship) {
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
    (false, true, _) | (false, false, true) => "Passive (sharing/transmitting)",
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
  let _ = writeln!(
    out,
    "- Computer: {} bandwidth, {} in use. Running: {}. Aboard but not running: {}",
    ship.bandwidth_used() + ship.bandwidth_spare(),
    ship.bandwidth_used(),
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
  let closing = nearest.map_or(0.0, |ship| {
    let line = contact.position - ship.get_position();
    if line.magnitude() < 1e-6 {
      0.0
    } else {
      -(contact.velocity - ship.get_velocity()).dot(line.normalize()) * DELTA_TIME_F64
    }
  });
  let next = (contact.distance - closing).max(0.0);
  let motion = if closing.abs() < 500.0 {
    "range steady".to_string()
  } else if closing > 0.0 {
    format!("closing {} km per round", km(closing))
  } else {
    format!("opening {} km per round", km(-closing))
  };
  let _ = writeln!(
    out,
    "- From {} (your nearest): {} km, {}; {motion}; next round about {} km ({}) if nobody burns",
    contact.nearest,
    km(contact.distance),
    contact.band,
    km(next),
    band_at(next)
  );
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
  let _ = writeln!(out, "- Visible damage: {}", crits(&contact.crit_level));
  let _ = writeln!(out, "- Held as a contact by: {}\n", contact.held_by.join(", "));
}

fn odds(out: &mut String, view: &Observation) {
  if view.engagements.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## Odds, per round, flying straight (from the rules' own arithmetic)\n\n\"Hit\" is the best gun's chance; \"damage\" is expected hull points per round from every gun that reaches, after armour, with the share of the target's hull that is. Enemy gunners, whom you cannot see, are assumed skill 2.\n"
  );
  for engagement in &view.engagements {
    odds_table(out, view, engagement);
  }
}

fn odds_table(out: &mut String, view: &Observation, engagement: &Engagement) {
  let ours = view.own.iter().find(|ship| ship.get_name() == engagement.ours);
  let contact = view.contacts.iter().find(|contact| contact.name == engagement.contact);
  let (Some(ours), Some(contact)) = (ours, contact) else {
    return;
  };
  let _ = writeln!(
    out,
    "### {} (hull {} left) against {} (hull {} as built)\n",
    ours.get_name(),
    ours.current_hull,
    contact.name,
    contact.design_hull
  );
  let _ = writeln!(
    out,
    "| Band | You hit | Your damage (share of their hull) | They hit | Their damage (share of yours) |"
  );
  let _ = writeln!(out, "|---|---|---|---|---|");
  let their_hull = f64::from(contact.design_hull.max(1));
  let our_hull = f64::from(ours.current_hull.max(1));
  for (we, they) in engagement.we_hit.iter().zip(&engagement.they_hit) {
    let _ = writeln!(
      out,
      "| {} | {} | {} | {} | {} |",
      we.band,
      hit_cell(we),
      damage_cell(we, their_hull),
      hit_cell(they),
      damage_cell(they, our_hull)
    );
  }
  out.push('\n');
}

fn hit_cell(odds: &BandOdds) -> String {
  if odds.mounts == 0 {
    "out of reach".to_string()
  } else {
    percent(odds.best_chance)
  }
}

fn damage_cell(odds: &BandOdds, hull: f64) -> String {
  if odds.mounts == 0 {
    "-".to_string()
  } else {
    format!(
      "{:.1} ({}; {} mount{}{})",
      odds.expected_damage,
      percent(odds.expected_damage / hull),
      odds.mounts,
      if odds.mounts == 1 { "" } else { "s" },
      if odds.missiles_only { ", missiles only" } else { "" }
    )
  }
}

fn exposures(out: &mut String, view: &Observation) {
  if view.exposures.is_empty() {
    return;
  }
  let _ = writeln!(
    out,
    "## How likely each contact is to find your ship this round, by posture\n\nThe second figure is if the ship also fires.\n"
  );
  for exposure in &view.exposures {
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
    "- {} as seen by {}: silent {}, passive {}, active {}{}",
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

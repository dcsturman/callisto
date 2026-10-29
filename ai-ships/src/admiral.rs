//! What the fleet commander -- a language model -- is told, and what it
//! answers with: the system prompt, the situation each call, and the one
//! tool it calls, `give_orders`, whose arguments are a [`Directive`].
//!
//! The model never sees a coordinate. Designs are described once each;
//! enemies come grouped by design and position; every distance is a range
//! band and kilometres; every place has a name the scripts can use.

use std::collections::BTreeMap;
use std::fmt::Write;
use std::sync::Arc;

use callisto::combat::battery_intercept_dice;
use callisto::entity::{Entity, Vec3};
use callisto::ship::{get_ship_template, Range, Ship, ShipDesignTemplate};
use callisto::tactics;
use cgmath::InnerSpace;
use serde_json::{json, Value};

use crate::fleet::{squadrons, Directive};
use crate::observation::{Contact, Observation};

pub const TOOL_NAME: &str = "give_orders";
pub const TOOL_DESCRIPTION: &str =
  "Give every squadron its script, say in a sentence or two what you ordered, and say when to be asked again.";

/// Enemies of one design within this of each other are one group (metres):
/// Medium range.
const GROUP_SPAN: f64 = 10_000_000.0;
/// Names listed for a group before the rest are counted.
const NAMES_SHOWN: usize = 6;

/// The fixed instructions: who the model is, what scripts can say, how
/// these fights go, and worked examples.
#[must_use]
pub fn system() -> &'static str {
  include_str!("prompt/admiral.md")
}

/// The orders the side's ships carry, read as its commanders gave them: the
/// fleet's, each squadron's where it differs, and each ship's where hers
/// differs from both.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Orders {
  pub fleet: Option<String>,
  pub squadrons: Vec<(String, String)>,
  pub ships: Vec<(String, String)>,
}

impl Orders {
  #[must_use]
  pub fn is_empty(&self) -> bool {
    self.fleet.is_none() && self.squadrons.is_empty() && self.ships.is_empty()
  }
}

/// Read `own`'s orders by scope: the fleet's and each squadron's as their
/// ships carry them, and each ship's own.
#[must_use]
pub fn orders_of(own: &[Ship]) -> Orders {
  let clean = |text: &Option<String>| text.as_deref().map(str::trim).filter(|t| !t.is_empty()).map(str::to_string);
  let fleet = own.iter().find_map(|ship| clean(&ship.fleet_orders));
  let squadrons = squadrons(own, &[])
    .into_iter()
    .filter(|(name, _)| !name.starts_with('('))
    .filter_map(|(name, members)| {
      let text = own
        .iter()
        .filter(|ship| members.iter().any(|m| m == ship.get_name()))
        .find_map(|ship| clean(&ship.squadron_orders))?;
      Some((name, text))
    })
    .collect();
  let ships = own
    .iter()
    .filter_map(|ship| Some((ship.get_name().to_string(), clean(&ship.orders)?)))
    .collect();
  Orders {
    fleet,
    squadrons,
    ships,
  }
}

fn km(metres: f64) -> String {
  #[allow(clippy::cast_possible_truncation)]
  let whole = (metres / 1000.0).round() as i64;
  let digits = whole.abs().to_string();
  let grouped: Vec<String> = digits
    .as_bytes()
    .rchunks(3)
    .rev()
    .map(|chunk| String::from_utf8_lossy(chunk).to_string())
    .collect();
  format!("{}{} km", if whole < 0 { "-" } else { "" }, grouped.join(","))
}

fn band_name(band: Range) -> &'static str {
  match band {
    Range::Short => "Short",
    Range::Medium => "Medium",
    Range::Long => "Long",
    Range::VeryLong => "VeryLong",
    Range::Distant => "Distant",
  }
}

/// "Medium (8,200 km)".
fn how_far(metres: f64) -> String {
  format!("{} ({})", band_name(tactics::band_at(metres)), km(metres))
}

/// "closing 3.1 km/s", "opening 0.4 km/s" or "holding range".
fn motion(km_per_second: f64) -> String {
  if km_per_second.abs() < 0.05 {
    "holding range".to_string()
  } else if km_per_second > 0.0 {
    format!("closing {km_per_second:.1} km/s")
  } else {
    format!("opening {:.1} km/s", -km_per_second)
  }
}

/// The closing speed of two things, km/s, positive when closing.
fn closing(from: Vec3, from_velocity: Vec3, to: Vec3, to_velocity: Vec3) -> f64 {
  let offset = to - from;
  let distance = offset.magnitude();
  if distance < 1.0 {
    return 0.0;
  }
  -(to_velocity - from_velocity).dot(offset / distance) / 1000.0
}

fn middle(points: impl Iterator<Item = Vec3>) -> Vec3 {
  let (sum, count) = points.fold((Vec3::new(0.0, 0.0, 0.0), 0.0), |(sum, count), p| (sum + p, count + 1.0));
  if count > 0.0 {
    sum / count
  } else {
    sum
  }
}

/// One line for a design: size, drive, protection, and every mount with the
/// furthest band it reaches.
fn describe_design(design: &Arc<ShipDesignTemplate>) -> String {
  let ship = Ship::new(
    design.name.clone(),
    Vec3::new(0.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, 0.0),
    design,
    None,
    None,
  );
  let mut mounts: BTreeMap<String, usize> = BTreeMap::new();
  for (index, weapon) in ship.weapons().iter().enumerate() {
    let label = if let Some(dice) = battery_intercept_dice(weapon) {
      format!("point defence battery ({dice}D a round)")
    } else {
      // The furthest band any of its guns reaches; launchers fly anywhere.
      let reach = [
        Range::Distant,
        Range::VeryLong,
        Range::Long,
        Range::Medium,
        Range::Short,
      ]
      .into_iter()
      .find_map(|band| {
        tactics::weapons_that_bear(&ship, band)
          .into_iter()
          .find(|(mount, _)| *mount == index)
          .map(|(_, kind)| (band, kind))
      });
      let kind = weapon.guns.first().map(|gun| gun.kind);
      let guns = if weapon.guns.len() > 1 {
        format!("{}-gun ", weapon.guns.len())
      } else {
        String::new()
      };
      match (reach, kind) {
        (Some((_, fires)), _) if tactics::launch_size(weapon, fires).is_some() => {
          format!("{guns}{fires:?} {:?} (any range)", weapon.mount)
        }
        (Some((band, fires)), _) => format!("{guns}{fires:?} {:?} (to {})", weapon.mount, band_name(band)),
        (None, Some(kind)) => format!("{guns}{kind:?} {:?} (defensive)", weapon.mount),
        (None, None) => continue,
      }
    };
    *mounts.entry(label).or_default() += 1;
  }
  let weapons: Vec<String> = mounts
    .into_iter()
    .map(|(label, count)| if count > 1 { format!("{count} x {label}") } else { label })
    .collect();
  let stealth = design
    .stealth
    .map(|stealth| format!(", {stealth:?} stealth"))
    .unwrap_or_default();
  format!(
    "{}: {} t, {} G, hull {}, armour {}{stealth}; {}",
    design.name,
    design.displacement,
    design.maneuver,
    design.hull,
    design.armor,
    if weapons.is_empty() {
      "unarmed".to_string()
    } else {
      weapons.join(", ")
    }
  )
}

/// One group of enemies: the same design, close together.
struct Group<'a> {
  name: String,
  design: &'a str,
  ships: Vec<&'a Contact>,
}

/// The enemy, grouped by design and position, nearest to `from` first.
fn enemy_groups<'a>(enemies: &[&'a Contact], from: Vec3) -> Vec<Group<'a>> {
  let mut groups: Vec<Group> = Vec::new();
  for contact in enemies {
    match groups.iter_mut().find(|group| {
      group.design == contact.design && (group.ships[0].position - contact.position).magnitude() <= GROUP_SPAN
    }) {
      Some(group) => group.ships.push(contact),
      None => groups.push(Group {
        name: String::new(),
        design: &contact.design,
        ships: vec![contact],
      }),
    }
  }
  groups.sort_by(|a, b| {
    let distance = |group: &Group| (middle(group.ships.iter().map(|c| c.position)) - from).magnitude();
    distance(a).total_cmp(&distance(b))
  });
  for (i, group) in groups.iter_mut().enumerate() {
    group.name = format!("E{}", i + 1);
  }
  groups
}

fn share(part: u64, whole: u64) -> u64 {
  100 * part / whole.max(1)
}

fn group_middle(group: &Group) -> Vec3 {
  middle(group.ships.iter().map(|c| c.position))
}

fn group_velocity(group: &Group) -> Vec3 {
  middle(group.ships.iter().map(|c| c.velocity))
}

/// The orders section: by scope, ships given the same orders on one line.
fn write_orders(out: &mut String, orders: &Orders) {
  let _ = writeln!(out, "\n## Orders");
  if orders.is_empty() {
    let _ = writeln!(out, "None given: fight under the scenario's goal.");
  }
  if let Some(fleet) = &orders.fleet {
    let _ = writeln!(out, "Fleet: {fleet}");
  }
  for (squadron, text) in &orders.squadrons {
    let _ = writeln!(out, "Squadron {squadron}: {text}");
  }
  // Ships given the same orders read as one line.
  let mut by_text: Vec<(&str, Vec<&str>)> = Vec::new();
  for (ship, text) in &orders.ships {
    match by_text.iter_mut().find(|(said, _)| *said == text.as_str()) {
      Some((_, ships)) => ships.push(ship),
      None => by_text.push((text, vec![ship])),
    }
  }
  for (text, ships) in by_text {
    let who = if ships.len() == 1 { "Ship" } else { "Ships" };
    let _ = writeln!(out, "{who} {}: {text}", ships.join(", "));
  }
}

/// Our squadrons: make-up, hull, speed, the nearest enemy, the script.
fn write_squadrons(
  out: &mut String, view: &Observation, groups: &[Group], steps: &dyn Fn(&str) -> Option<(usize, usize)>,
) {
  let _ = writeln!(out, "\n## Our squadrons");
  for (name, members) in squadrons(&view.own, &[]) {
    let ships: Vec<&Ship> = view
      .own
      .iter()
      .filter(|ship| members.iter().any(|m| m == ship.get_name()))
      .collect();
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for ship in &ships {
      *kinds.entry(ship.design.name.as_str()).or_default() += 1;
    }
    let make_up: Vec<String> = kinds.iter().map(|(design, n)| format!("{n} x {design}")).collect();
    let hull: u64 = ships.iter().map(|ship| u64::from(ship.current_hull)).sum();
    let built: u64 = ships.iter().map(|ship| u64::from(ship.design.hull)).sum();
    let at = middle(ships.iter().map(|ship| ship.get_position()));
    let velocity = middle(ships.iter().map(|ship| ship.get_velocity()));
    let nearest = groups
      .iter()
      .min_by(|a, b| {
        (group_middle(a) - at)
          .magnitude()
          .total_cmp(&(group_middle(b) - at).magnitude())
      })
      .map(|group| {
        format!(
          "; nearest enemy {} at {}, {}",
          group.name,
          how_far((group_middle(group) - at).magnitude()),
          motion(closing(at, velocity, group_middle(group), group_velocity(group)))
        )
      })
      .unwrap_or_default();
    let script = match steps(&name) {
      Some((step, of)) => format!("; on step {} of {of}", step + 1),
      None => "; no script: the crews fight freely".to_string(),
    };
    let _ = writeln!(
      out,
      "- {name}: {}; hull {}%; {:.1} km/s{nearest}{script}. Ships: {}",
      make_up.join(", "),
      share(hull, built),
      velocity.magnitude() / 1000.0,
      members.join(", ")
    );
  }
}

/// What the commander is shown this call: the round, the orders, the
/// designs, our squadrons and their scripts, the enemy, the places, and the
/// scripts as they stand. `steps` says where a squadron is in its script.
#[must_use]
pub fn situation(
  view: &Observation, orders: &Orders, directive: &Directive, steps: &dyn Fn(&str) -> Option<(usize, usize)>,
) -> String {
  let mut out = String::new();
  let _ = writeln!(out, "Round {}.", view.round);

  write_orders(&mut out, orders);

  let enemies = view.enemies();
  let mut designs: Vec<String> = view
    .own
    .iter()
    .map(|ship| ship.design.name.clone())
    .chain(enemies.iter().map(|contact| contact.design.clone()))
    .collect();
  designs.sort();
  designs.dedup();
  let _ = writeln!(out, "\n## Designs");
  for name in &designs {
    if let Some(design) = get_ship_template(name) {
      let _ = writeln!(out, "- {}", describe_design(&design));
    }
  }

  let centre = middle(view.own.iter().map(Entity::get_position));
  let groups = enemy_groups(&enemies, centre);

  write_squadrons(&mut out, view, &groups, steps);

  let _ = writeln!(out, "\n## Enemy");
  if groups.is_empty() && view.blips.is_empty() {
    let _ = writeln!(out, "None found yet.");
  }
  for group in &groups {
    let hull: u64 = group.ships.iter().map(|c| u64::from(c.current_hull)).sum();
    let built: u64 = group.ships.iter().map(|c| u64::from(c.design_hull)).sum();
    let names: Vec<&str> = group.ships.iter().take(NAMES_SHOWN).map(|c| c.name.as_str()).collect();
    let more = group.ships.len().saturating_sub(NAMES_SHOWN);
    let _ = writeln!(
      out,
      "- {}: {} x {}; hull {}%; {} from our middle, {}. Ships: {}{}",
      group.name,
      group.ships.len(),
      group.design,
      share(hull, built),
      how_far((group_middle(group) - centre).magnitude()),
      motion(closing(
        centre,
        middle(view.own.iter().map(Entity::get_velocity)),
        group_middle(group),
        group_velocity(group)
      )),
      names.join(", "),
      if more > 0 {
        format!(" and {more} more")
      } else {
        String::new()
      }
    );
  }
  if !view.blips.is_empty() {
    let nearest = view.blips.iter().map(|blip| blip.distance).fold(f64::INFINITY, f64::min);
    let _ = writeln!(
      out,
      "- Unidentified: {} {}, nearest {} from one of ours.",
      view.blips.len(),
      if view.blips.len() == 1 { "blip" } else { "blips" },
      how_far(nearest)
    );
  }

  if !view.planets.is_empty() {
    let _ = writeln!(out, "\n## Places");
    for planet in &view.planets {
      let _ = writeln!(
        out,
        "- {}: {} from our middle.",
        planet.get_name(),
        how_far((planet.get_position() - centre).magnitude())
      );
    }
  }

  let _ = writeln!(out, "\n## Scripts now");
  if directive.squadrons.is_empty() {
    let _ = writeln!(out, "None.");
  } else {
    let _ = writeln!(out, "{}", serde_json::to_string(directive).unwrap_or_default());
  }
  out
}

/// The same schema, with what it is for where it is used put first.
fn described(schema: &Value, what: &str) -> Value {
  let mut schema = schema.clone();
  let also = schema["description"].as_str().map(|d| format!(" {d}")).unwrap_or_default();
  schema["description"] = json!(format!("{what}{also}"));
  schema
}

/// The `give_orders` tool's parameters, as JSON Schema: a [`Directive`].
#[must_use]
pub fn schema() -> Value {
  let text = |what: &str| json!({"type": "string", "description": what});
  let names = |what: &str| json!({"type": "array", "items": {"type": "string"}, "description": what});
  let band = json!({"type": "string", "enum": ["Short", "Medium", "Long", "VeryLong", "Distant"]});
  let rule = json!({
    "type": "object",
    "description": "Which enemies: any of the designs or ships named (any at all if neither), within the tonnage limits.",
    "properties": {
      "designs": names("Enemy design names, exactly as listed."),
      "ships": names("Enemy ship names, exactly as listed under Enemy; never our own."),
      "max_tons": {"type": "integer"},
      "min_tons": {"type": "integer"}
    }
  });
  let place = json!({
    "type": "object",
    "description": "A place: give one of these.",
    "properties": {
      "planet": text("A planet, as listed under Places."),
      "ship": text("A ship, ours or an enemy contact."),
      "squadron": text("The middle of one of our squadrons."),
      "enemies": rule,
      "at_km": {"type": "array", "items": {"type": "number"}, "description": "x, y, z in km."}
    }
  });
  let until = json!({
    "type": "object",
    "description": "When the step ends: as soon as any one given is true.",
    "properties": {
      "round": {"type": "integer", "description": "This round has begun."},
      "within": {"type": "object", "properties": {"of": place, "band": band}, "required": ["of", "band"],
        "description": "Any of the squadron is within this band of the place."},
      "fired_on": {"type": "boolean", "description": "The squadron took damage last round."},
      "losses": {"type": "number", "description": "Share of the squadron's hull lost during the step, 0 to 1."}
    }
  });
  let movement = json!({
    "type": "object",
    "properties": {
      "mode": {"type": "string", "enum": ["network", "burn", "arrive"],
        "description": "network: the crews manoeuvre (default). burn: toward a place, or away from it, at up to thrust G. arrive: at a place, or at band of it, by round, together."},
      "toward": described(&place, "burn only: the place burned toward, or away from."),
      "away": {"type": "boolean", "description": "burn only: away from `toward` instead."},
      "thrust": {"type": "number", "description": "burn only: G, at most; leave it out for full thrust."},
      "at": described(&place, "arrive only: the place arrived at."),
      "band": described(&band, "arrive only: arrive at this range band of `at`, on this side, rather than on it."),
      "round": {"type": "integer", "description": "arrive only: the round to be there by."}
    },
    // Without a mode the rest is ignored: say which, every time.
    "required": ["mode"]
  });
  let step = json!({
    "type": "object",
    "description": "What the squadron does until `until`; the last step has no `until`.",
    "properties": {
      "goal": {"type": "string", "enum": ["destroy", "hold", "raid", "escape"]},
      "post": described(&place, "hold only: where a hold step holds; the squadron's starting point if left out. Without goal hold it does nothing."),
      "post_radius_km": {"type": "number"},
      "engage": rule,
      "threats": {"type": "integer", "description": "Enemies that can hit the squadron it may also engage. Default 8."},
      "movement": movement,
      "dodge": {"type": "integer", "description": "G of thrust each ship keeps back to dodge, at least; the rest of her drive is for moving. Leave it out to let the crews choose."},
      "until": until
    }
  });
  json!({
    "type": "object",
    "properties": {
      "say": text("One or two sentences to your commanders: what you ordered, and anything you could not do."),
      "squadrons": {
        "type": "array",
        "description": "Every squadron's script. `*` is every squadron without its own; `ship` instead of `squadron` gives one ship orders of her own.",
        "items": {
          "type": "object",
          "properties": {
            "squadron": text("A squadron's name exactly as listed under Our squadrons, or * for every squadron without a script of its own. For a ship's own script, the squadron she is in."),
            "ship": text("A ship's name, for a ship with orders of her own."),
            "steps": {"type": "array", "items": step}
          },
          "required": ["squadron", "steps"]
        }
      },
      "call_me_when": {
        "type": "array",
        "description": "When to be asked again: any one of these.",
        "items": {
          "type": "object",
          "properties": {
            "squadron": text("Whose losses; any squadron if absent."),
            "losses": {"type": "number", "description": "Share of hull lost since these orders, 0 to 1."},
            "new_enemies": {"type": "boolean", "description": "Enemies not yet seen are found."},
            "round": {"type": "integer"}
          }
        }
      }
    },
    "required": ["say", "squadrons"]
  })
}

/// A side commanded by a language model: it writes the squadrons' scripts,
/// and the [`FleetCommander`](crate::fleet::FleetCommander) carries them
/// out. The model is asked only when the orders the side's ships carry
/// change, or something it asked to hear about happens (`call_me_when`);
/// with no orders at all it is never asked, and the crews fight freely.
pub struct Admiral {
  fleet: crate::fleet::FleetCommander,
  vertex: Arc<crate::provider::Vertex>,
  spec: crate::provider::ModelSpec,
  /// Each ship's orders when the model last answered, by ship.
  answered_orders: Option<BTreeMap<String, String>>,
  /// Each squadron's hull, and the enemies known, when it last answered.
  hull_then: BTreeMap<String, u32>,
  seen: Vec<String>,
  answered: bool,
  spent: f64,
  /// Every call, as JSON lines, if kept.
  call_log: Option<Box<dyn std::io::Write + Send>>,
}

impl Admiral {
  #[must_use]
  pub fn new(
    net: Arc<crate::network::side_net::SideNet>, vertex: Arc<crate::provider::Vertex>, spec: crate::provider::ModelSpec,
  ) -> Self {
    Admiral {
      fleet: crate::fleet::FleetCommander::new(net, Directive::default()),
      vertex,
      spec,
      answered_orders: None,
      hull_then: BTreeMap::new(),
      seen: Vec::new(),
      answered: true,
      spent: 0.0,
      call_log: None,
    }
  }

  /// Keep every call -- situation, answer, cost -- as JSON lines.
  #[must_use]
  pub fn with_call_log(mut self, log: Box<dyn std::io::Write + Send>) -> Self {
    self.call_log = Some(log);
    self
  }

  /// What the model has spent so far, in US dollars.
  #[must_use]
  pub fn spent(&self) -> f64 {
    self.spent
  }

  fn squadron_hulls(view: &Observation) -> BTreeMap<String, u32> {
    squadrons(&view.own, &[])
      .into_iter()
      .map(|(name, members)| {
        let hull = view
          .own
          .iter()
          .filter(|ship| members.iter().any(|m| m == ship.get_name()))
          .map(|ship| ship.current_hull)
          .sum();
        (name, hull)
      })
      .collect()
  }

  /// Each ship's orders, by ship: what a change of orders is judged by.
  fn orders_by_ship(own: &[Ship]) -> BTreeMap<String, String> {
    own
      .iter()
      .filter(|ship| ship.orders.is_some() || ship.squadron_orders.is_some() || ship.fleet_orders.is_some())
      .map(|ship| {
        let text = |orders: &Option<String>| orders.as_deref().unwrap_or("").trim().to_string();
        (
          ship.get_name().to_string(),
          [
            text(&ship.fleet_orders),
            text(&ship.squadron_orders),
            text(&ship.orders),
          ]
          .join("\n"),
        )
      })
      .collect()
  }

  /// Why the model should be asked now, if it should.
  fn reason(&self, view: &Observation, orders: &Orders) -> Option<String> {
    let now = Self::orders_by_ship(&view.own);
    match &self.answered_orders {
      // Never asked: only once there are orders to give.
      None if !orders.is_empty() => return Some("new orders".to_string()),
      None => {}
      // A ship lost is not new orders; a living ship's orders changed are.
      Some(then) => {
        if let Some(changed) = view
          .own
          .iter()
          .map(Entity::get_name)
          .find(|ship| now.get(*ship) != then.get(*ship))
        {
          return Some(format!("new orders for {changed}"));
        }
      }
    }
    let hulls = Self::squadron_hulls(view);
    for trigger in &self.fleet.directive().call_me_when {
      if trigger.round.is_some_and(|round| view.round == round) {
        return Some(format!("round {}", view.round));
      }
      if trigger.new_enemies {
        if let Some(found) = view.enemies().iter().find(|c| !self.seen.contains(&c.name)) {
          return Some(format!("{} found", found.name));
        }
      }
      if let Some(share) = trigger.losses {
        let lost = self.hull_then.iter().find(|(name, then)| {
          trigger.squadron.as_ref().is_none_or(|wanted| wanted == *name) && {
            let now = hulls.get(*name).copied().unwrap_or(0);
            #[allow(clippy::cast_precision_loss)]
            let fraction = (**then - now.min(**then)) as f32 / (**then).max(1) as f32;
            fraction >= share
          }
        });
        if let Some((name, _)) = lost {
          return Some(format!("{name} has lost {:.0}% of its hull", share * 100.0));
        }
      }
    }
    None
  }

  async fn ask(&mut self, view: &Observation, orders: &Orders, reason: &str) {
    let fleet = &self.fleet;
    let situation = format!(
      "Asked because: {reason}.\n\n{}",
      situation(view, orders, fleet.directive(), &|group: &str| fleet.step_of(group))
    );
    let tool = crate::provider::Tool {
      name: TOOL_NAME,
      description: TOOL_DESCRIPTION,
      schema: schema(),
    };
    let started = std::time::Instant::now();
    let result = self.vertex.call(&self.spec, system(), &situation, &tool).await;
    let seconds = started.elapsed().as_secs_f64();
    let (record, directive) = match result {
      Ok(call) => {
        let cost = call.cost(&self.spec);
        self.spent += cost.unwrap_or(0.0);
        let directive: Result<Directive, _> = serde_json::from_value(call.args.clone());
        let record = json!({
          "round": view.round, "reason": reason, "situation": situation, "answer": call.args,
          "input_tokens": call.input_tokens, "cached_tokens": call.cached_tokens,
          "output_tokens": call.output_tokens, "cost_usd": cost, "seconds": seconds,
          "error": directive.as_ref().err().map(ToString::to_string),
        });
        (record, directive.ok())
      }
      Err(e) => (
        json!({"round": view.round, "reason": reason, "situation": situation, "error": e.to_string(), "seconds": seconds}),
        None,
      ),
    };
    if let Some(log) = self.call_log.as_mut() {
      let _ = writeln!(log, "{record}");
      let _ = log.flush();
    }
    if let Some(directive) = directive {
      tracing::info!(
        "(admiral) Round {}, {reason}: {}",
        view.round,
        directive.say.as_deref().unwrap_or("(nothing said)")
      );
      self.fleet.set_directive(directive);
      self.answered_orders = Some(Self::orders_by_ship(&view.own));
      self.hull_then = Self::squadron_hulls(view);
      self.seen = view.enemies().iter().map(|c| c.name.clone()).collect();
      self.answered = true;
    } else {
      // Asked again next round; the squadrons keep their scripts meanwhile.
      tracing::warn!("(admiral) Round {}: no orders from the model: {}", view.round, record["error"]);
      self.answered = false;
    }
  }
}

#[async_trait::async_trait]
impl crate::commander::Commander for Admiral {
  async fn decide(&mut self, view: &Observation) -> Vec<(String, tactics::Intent)> {
    let orders = orders_of(&view.own);
    if let Some(reason) = self.reason(view, &orders) {
      self.ask(view, &orders, &reason).await;
    }
    self.fleet.decide(view).await
  }

  fn plan(&self) -> Option<&str> {
    self.fleet.directive().say.as_deref()
  }

  fn answered(&self) -> bool {
    self.answered
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn ship(name: &str, squadron: Option<&str>, orders: Option<&str>) -> Ship {
    let mut ship = Ship::new(
      name.to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
      None,
    );
    ship.squadron = squadron.map(str::to_string);
    ship.orders = orders.map(str::to_string);
    ship
  }

  /// Orders read by scope: the fleet's and each squadron's as their ships
  /// carry them, and each ship's own.
  #[test]
  fn orders_are_read_by_scope() {
    let mut own = vec![
      ship("Lance 1", Some("Lance"), None),
      ship("Lance 2", Some("Lance"), Some("Stay by the flagship.")),
      ship("Hesperus", Some("Flag"), None),
    ];
    for ship in &mut own {
      ship.fleet_orders = Some("Hold the gate.".to_string());
      if ship.squadron.as_deref() == Some("Lance") {
        ship.squadron_orders = Some("Fighters first.".to_string());
      }
    }
    let orders = orders_of(&own);
    assert_eq!(orders.fleet.as_deref(), Some("Hold the gate."));
    assert_eq!(orders.squadrons, vec![("Lance".to_string(), "Fighters first.".to_string())]);
    assert_eq!(orders.ships, vec![("Lance 2".to_string(), "Stay by the flagship.".to_string())]);
  }

  /// Hull shares are summed and scaled in u64: a few billion-ton stations
  /// pass u32 hull points, and 100 x a single one's did.
  #[test]
  fn hull_shares_survive_station_sized_hulls() {
    assert_eq!(share(1, 2), 50);
    assert_eq!(share(0, 0), 0);
    let station = u64::from(666_666_667_u32);
    let fleet = [station; 10];
    let built: u64 = fleet.iter().sum();
    assert!(built > u64::from(u32::MAX));
    assert_eq!(share(built / 2, built), 50);
  }

  #[test]
  fn kilometres_are_grouped() {
    assert_eq!(km(8_204_400.0), "8,204 km");
    assert_eq!(km(950_000.0), "950 km");
    assert_eq!(km(52_000_000.0), "52,000 km");
  }

  /// The examples in the system prompt are orders the executor reads.
  #[test]
  fn the_prompts_examples_are_directives() {
    let examples: Vec<&str> = system()
      .split("```json")
      .skip(1)
      .filter_map(|rest| rest.split("```").next())
      .collect();
    assert_eq!(examples.len(), 3);
    for example in examples {
      let directive: Result<Directive, _> = serde_json::from_str(example);
      assert!(directive.is_ok(), "{example}: {directive:?}");
    }
  }

  #[test]
  fn the_schema_names_the_directive_fields() {
    let schema = schema();
    for field in ["say", "squadrons", "call_me_when"] {
      assert!(schema["properties"][field].is_object(), "{field}");
    }
  }
}

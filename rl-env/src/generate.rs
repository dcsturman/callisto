//! Scenarios drawn at random for training, so the network meets capital
//! ships, spinal mounts, batteries and carriers in many more fights than the
//! shipped scenario files give, and never the same one twice.
//!
//! A generated scenario is named in a pool as `gen:<kind>` beside the
//! scenario files, and each game draws its own fleets from the seed: two
//! sides, Blue and Red, each one or two capital ships from the Keelhaven
//! designs with a screen of escorts, and in a carrier fight a carrier with her
//! craft aboard (`aboard`, as a scenario file would have them). The fleets
//! open beyond Distant of each other with the attackers under way, so the
//! first shots come after a few rounds, as at Keelhaven; now and then Red
//! holds the point it starts at, as Keelhaven's defenders hold the moon.

use std::collections::HashMap;
use std::sync::Arc;

use callisto::crew::{Crew, Engineer, Skills};
use callisto::entity::{Entities, Entity, Vec3};
use callisto::ship::{Goal, Post, ShipDesignTemplate, Team, WeaponType};
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

/// The kinds of fight the generator draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
  /// Capital ships and escorts, no craft aboard: spinals and batteries.
  Capital,
  /// A carrier with her craft aboard on each side, and escorts.
  Carrier,
  /// Either, drawn game by game: capitals with their craft aboard when the
  /// design has berths, most of the time.
  Fleet,
}

impl Kind {
  /// The kind a pool entry names: `gen:capital`, `gen:carrier` or
  /// `gen:fleet`.
  #[must_use]
  pub fn parse(name: &str) -> Option<Kind> {
    match name.strip_prefix("gen:")? {
      "capital" => Some(Kind::Capital),
      "carrier" => Some(Kind::Carrier),
      "fleet" => Some(Kind::Fleet),
      _ => None,
    }
  }

  /// A pool entry `gen:<kind>:<seed>`: the kind and the seed it draws
  /// its fight from. A bare `gen:<kind>` is seed 0.
  #[must_use]
  pub fn parse_entry(entry: &str) -> Option<(Kind, u64)> {
    let rest = entry.strip_prefix("gen:")?;
    let (kind, seed) = rest.split_once(':').unwrap_or((rest, "0"));
    Some((Kind::parse(&format!("gen:{kind}"))?, seed.parse().ok()?))
  }

  /// The pool entry's name.
  #[must_use]
  pub fn name(self) -> &'static str {
    match self {
      Kind::Capital => "gen:capital",
      Kind::Carrier => "gen:carrier",
      Kind::Fleet => "gen:fleet",
    }
  }
}

/// The capital designs, as `docs/capital_ships_plan.md` step 7 built them.
pub const CAPITALS: [&str; 4] = [
  "Battle Rider - Hadrian",
  "Planetoid Monitor - Jaellemo",
  "Strike Carrier - Wind",
  "Light Cruiser - Valiant",
];
/// The carriers among them, the craft they carry and how many berths they
/// have for them.
pub const CARRIERS: [(&str, &str, u32); 2] = [
  ("Strike Carrier - Wind", "Light Fighter", 80),
  ("Planetoid Monitor - Jaellemo", "Heavy Fighter", 30),
];
/// The escorts a side's screen is drawn from.
pub const ESCORTS: [&str; 8] = [
  "Fleet Escort - P.F. Sloan",
  "Midu Agasham",
  "Destroyer Escort - Fer-de-Lance",
  "Destroyer Escort - Chrysanthemum",
  "Colonial Cruiser - Kinunir",
  "System Defence Boat - Dragon",
  "Gazelle",
  "Patrol Corvette - Type T",
];
/// The fewest craft a carrier sails with.
pub const LEAST_CRAFT: u32 = 6;
/// The most ships a side may have in a training game, craft aboard
/// included: the network's own-ship slots.
pub const MAX_SIDE: usize = ai_ships::network::encode::MAX_SHIPS;
/// How far apart the fleets open, in metres: beyond Distant, as at Keelhaven.
pub const APART: (f64, f64) = (6.0e7, 1.2e8);
/// The attackers' closing speed, metres a second.
pub const CLOSING: (f64, f64) = (5_000.0, 20_000.0);
/// How far a ship may sit from her fleet's middle, in metres.
pub const SPREAD: f64 = 2.0e6;
/// The share of games in which Red holds where it starts.
pub const HOLD_SHARE: f64 = 0.3;

/// A trained crew for `design`: skill 2 throughout, pilots 3 on craft,
/// a spare gunner for every launcher and sandcaster mount (so reloading is
/// free, as the capital designs have it), a second engineer and a sensor
/// operator on anything over 1,000 tons, leadership on a capital.
#[must_use]
pub fn trained_crew(design: &ShipDesignTemplate) -> Crew {
  let mut crew = Crew::new();
  let craft = design.displacement <= 100;
  crew.set_skill(Skills::Pilot, if craft { 3 } else { 2 });
  crew.add_sensor_operator(2);
  if design.displacement > 1_000 {
    crew.add_sensor_operator(2);
  }
  let engineer = Engineer {
    jump: 2,
    power: 2,
    maneuver: 2,
    mechanic: 2,
    remote_ops: 2,
    ..Engineer::default()
  };
  crew.add_engineer(engineer.clone());
  if design.displacement > 1_000 {
    crew.add_engineer(engineer);
  }
  let reloads = design
    .weapons
    .iter()
    .filter(|weapon| {
      weapon
        .kinds()
        .iter()
        .any(|kind| matches!(kind, WeaponType::Missile | WeaponType::Torpedo | WeaponType::Sand))
    })
    .count();
  for _ in 0..design.weapons.len() + reloads {
    crew.add_gunnery(2);
  }
  if design.displacement >= 10_000 {
    crew.set_skill(Skills::Leadership, 2);
  }
  crew
}

struct Fleet {
  team: Team,
  centre: Vec3,
  velocity: Vec3,
  hold: bool,
}

/// One side's ships: each design by name with the squadron it flies in, and
/// the craft aboard the carriers.
struct Draw {
  /// `(design, squadron)` for every ship on the plot.
  ships: Vec<(&'static str, String)>,
  /// `(carrier index in `ships`, craft design, how many)`.
  aboard: Vec<(usize, &'static str, u32)>,
}

fn draw(kind: Kind, rng: &mut SmallRng, team: Team) -> Draw {
  let mut ships: Vec<(&'static str, String)> = Vec::new();
  let mut aboard = Vec::new();
  let line = format!("{team:?} Line");
  let capitals = rng.gen_range(1..=2);
  for n in 0..capitals {
    let carrier = match kind {
      Kind::Carrier => n == 0,
      Kind::Fleet => rng.gen_bool(0.7),
      Kind::Capital => false,
    };
    let design = if carrier {
      CARRIERS[rng.gen_range(0..CARRIERS.len())].0
    } else {
      CAPITALS[rng.gen_range(0..CAPITALS.len())]
    };
    if let Some((_, craft, _)) = CARRIERS.iter().find(|(name, _, _)| *name == design && carrier) {
      aboard.push((ships.len(), *craft, 0));
    }
    ships.push((design, line.clone()));
  }
  let screen = format!("{team:?} Screen");
  let escorts = rng.gen_range(2..=if aboard.is_empty() { 6 } else { 4 });
  for _ in 0..escorts {
    ships.push((ESCORTS[rng.gen_range(0..ESCORTS.len())], screen.clone()));
  }
  // A training game has at most MAX_SIDE ships a side, the network's slots,
  // and the craft aboard are the side's ships too: the air groups share what
  // the ships on the plot leave, at least LEAST_CRAFT a carrier.
  let room = (MAX_SIDE - ships.len()) as u32;
  let carriers = aboard.len() as u32;
  for (_, _, count) in &mut aboard {
    let most = (room / carriers.max(1)).max(LEAST_CRAFT);
    *count = rng.gen_range(LEAST_CRAFT..=most);
  }
  Draw { ships, aboard }
}

fn place(rng: &mut SmallRng, fleet: &Fleet) -> Vec3 {
  let offset = Vec3::new(
    rng.gen_range(-SPREAD..=SPREAD),
    rng.gen_range(-SPREAD..=SPREAD),
    rng.gen_range(-SPREAD / 4.0..=SPREAD / 4.0),
  );
  fleet.centre + offset
}

/// A scenario of `kind` for `seed`, from `designs`.
///
/// # Panics
/// Panics if a design the generator needs is not in `designs`, or a craft
/// cannot be put aboard her carrier.
#[must_use]
pub fn generate<S: std::hash::BuildHasher>(
  kind: Kind, seed: u64, designs: &HashMap<String, Arc<ShipDesignTemplate>, S>,
) -> Entities {
  let mut rng = SmallRng::seed_from_u64(seed ^ 0x6745_2301_CAFE_F00D);
  let mut world = Entities::default();
  world.metadata.name = format!("{} #{seed}", kind.name());
  let apart = rng.gen_range(APART.0..=APART.1);
  let closing = rng.gen_range(CLOSING.0..=CLOSING.1);
  let hold = rng.gen_bool(HOLD_SHARE);
  let fleets = [
    Fleet {
      team: Team::Blue,
      centre: Vec3::new(0.0, 0.0, 0.0),
      velocity: Vec3::new(closing, 0.0, 0.0),
      hold: false,
    },
    Fleet {
      team: Team::Red,
      centre: Vec3::new(apart, 0.0, 0.0),
      velocity: Vec3::new(0.0, 0.0, 0.0),
      hold,
    },
  ];
  let design = |name: &str| {
    designs
      .get(name)
      .cloned()
      .unwrap_or_else(|| panic!("the generator needs the design {name}"))
  };
  for fleet in &fleets {
    let drawn = draw(kind, &mut rng, fleet.team);
    let mut names = Vec::new();
    for (n, (design_name, squadron)) in drawn.ships.iter().enumerate() {
      let template = design(design_name);
      let name = format!(
        "{:?} {} {}",
        fleet.team,
        if n < 2 && CAPITALS.contains(design_name) {
          "Capital"
        } else {
          "Escort"
        },
        n + 1
      );
      let at = place(&mut rng, fleet);
      world.add_ship(name.clone(), at, fleet.velocity, &template, Some(trained_crew(&template)), None);
      let mut ship = world.ships[&name].write().unwrap();
      ship.team = Some(fleet.team);
      ship.squadron = Some(squadron.clone());
      if fleet.hold {
        ship.goal = Some(Goal::Hold);
        ship.post = Some(Post::At([fleet.centre.x, fleet.centre.y, fleet.centre.z]));
      }
      drop(ship);
      names.push(name);
    }
    for (carrier, craft_design, count) in &drawn.aboard {
      let template = design(craft_design);
      let carrier_name = &names[*carrier];
      for k in 0..*count {
        let name = format!("{carrier_name} Wing {}-{:02}", k / 20 + 1, k % 20 + 1);
        let at = world.ships[carrier_name].read().unwrap().get_position();
        world.add_ship(name.clone(), at, fleet.velocity, &template, Some(trained_crew(&template)), None);
        {
          let mut craft = world.ships[&name].write().unwrap();
          craft.team = Some(fleet.team);
          craft.squadron = Some(format!("{carrier_name} Wing {}", k / 20 + 1));
          if fleet.hold {
            craft.goal = Some(Goal::Hold);
            craft.post = Some(Post::At([fleet.centre.x, fleet.centre.y, fleet.centre.z]));
          }
        }
        world
          .embark_ship(&name, carrier_name)
          .unwrap_or_else(|e| panic!("{name} aboard {carrier_name}: {e}"));
      }
    }
  }
  world
}

#[cfg(test)]
mod tests {
  use super::*;
  use cgmath::InnerSpace;

  fn designs() -> Arc<HashMap<String, Arc<ShipDesignTemplate>>> {
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    runtime.block_on(async {
      callisto::ship::replace_ship_templates(
        callisto::ship::load_ship_templates_from_dir("../callisto/ship_templates")
          .await
          .unwrap(),
      );
    });
    callisto::ship::get_ship_templates_snapshot()
  }

  #[test]
  fn entries_parse() {
    assert_eq!(Kind::parse_entry("gen:carrier:7"), Some((Kind::Carrier, 7)));
    assert_eq!(Kind::parse_entry("gen:capital"), Some((Kind::Capital, 0)));
    assert_eq!(Kind::parse_entry("gen:carrier:x"), None);
    assert_eq!(Kind::parse_entry("Thebus.json"), None);
  }

  #[test]
  fn names_parse() {
    assert_eq!(Kind::parse("gen:carrier"), Some(Kind::Carrier));
    assert_eq!(Kind::parse("gen:fleet"), Some(Kind::Fleet));
    assert_eq!(Kind::parse("gen:capital"), Some(Kind::Capital));
    assert_eq!(Kind::parse("gen:x"), None);
    assert_eq!(Kind::parse("../callisto/scenarios/Thebus.json"), None);
  }

  /// A carrier fight has a carrier a side with her craft aboard, at least
  /// ten and within her berths, the sides open beyond Distant, and the same
  /// seed draws the same fight.
  #[test]
  fn a_carrier_fight_has_craft_aboard_and_opens_at_distant() {
    let designs = designs();
    for seed in 0..6 {
      let world = generate(Kind::Carrier, seed, &designs);
      let sides = ai_ships::harness::sides(&world);
      assert_eq!(sides.len(), 2, "{seed}");
      for team in ["Blue", "Red"] {
        let on_plot: Vec<_> = world
          .ships
          .values()
          .map(|ship| ship.read().unwrap())
          .filter(|ship| format!("{:?}", ship.team.unwrap()) == team)
          .collect();
        assert!(on_plot.len() >= 3, "{seed}: {} on the plot", on_plot.len());
        assert!(sides[team].len() <= MAX_SIDE, "{seed}: {} ships on {team}", sides[team].len());
        let carriers: Vec<_> = on_plot.iter().filter(|ship| !ship.embarked.is_empty()).collect();
        assert!(!carriers.is_empty(), "{seed}: no carrier on {team}");
        for carrier in carriers {
          let aboard = carrier.embarked.len();
          assert!(
            aboard >= LEAST_CRAFT as usize && aboard <= carrier.berths().len(),
            "{seed}: {aboard}"
          );
          assert!(carrier.embarked.iter().all(|craft| craft.team == carrier.team));
          assert!(carrier.launch_per_round(carrier.embarked[0].design.displacement) > 0, "a tube");
        }
        assert!(sides[team].len() > on_plot.len(), "the craft aboard are the side's ships too");
        assert!(on_plot.iter().all(|ship| ship.get_crew().get_pilot() >= 2), "a trained crew");
      }
      let blue: Vec<Vec3> = world
        .ships
        .values()
        .map(|ship| ship.read().unwrap())
        .filter(|ship| ship.team == Some(Team::Blue))
        .map(|ship| ship.get_position())
        .collect();
      let red: Vec<Vec3> = world
        .ships
        .values()
        .map(|ship| ship.read().unwrap())
        .filter(|ship| ship.team == Some(Team::Red))
        .map(|ship| ship.get_position())
        .collect();
      let nearest = blue
        .iter()
        .flat_map(|a| red.iter().map(move |b| (a - b).magnitude()))
        .fold(f64::INFINITY, f64::min);
      assert!(nearest > 5.0e7, "{seed}: {nearest} m apart");
    }
    let again = generate(Kind::Carrier, 3, &designs);
    let once = generate(Kind::Carrier, 3, &designs);
    assert_eq!(
      ai_ships::harness::sides(&again),
      ai_ships::harness::sides(&once),
      "the same seed, the same fleets"
    );
  }

  /// A capital fight carries nothing aboard, and some seeds have Red hold
  /// its ground.
  #[test]
  fn a_capital_fight_has_capitals_and_sometimes_a_holder() {
    let designs = designs();
    let mut held = 0;
    for seed in 0..20 {
      let world = generate(Kind::Capital, seed, &designs);
      let ships: Vec<_> = world.ships.values().map(|ship| ship.read().unwrap()).collect();
      assert!(ships.iter().all(|ship| ship.embarked.is_empty()), "{seed}");
      assert!(
        ships
          .iter()
          .any(|ship| ship.team == Some(Team::Blue) && ship.design.displacement >= 30_000),
        "{seed}: Blue has a capital"
      );
      assert!(
        ships
          .iter()
          .any(|ship| ship.team == Some(Team::Red) && ship.design.displacement >= 30_000),
        "{seed}: Red has a capital"
      );
      let holders = ships.iter().filter(|ship| ship.goal == Some(Goal::Hold)).count();
      if holders > 0 {
        held += 1;
        assert!(ships
          .iter()
          .filter(|ship| ship.team == Some(Team::Red))
          .all(|ship| ship.goal == Some(Goal::Hold) && ship.post.is_some()));
      }
    }
    assert!(held > 0 && held < 20, "{held} of 20 held");
  }

  #[test]
  fn a_trained_crew_mans_every_mount_with_spares_for_the_launchers() {
    let designs = designs();
    let wind = &designs["Strike Carrier - Wind"];
    let crew = trained_crew(wind);
    assert!(crew.gunners() > wind.weapons.len(), "spares for the missile and sand batteries");
    assert_eq!(crew.get_pilot(), 2);
    assert_eq!(crew.sensor_operators(), 2);
    let fighter = &designs["Light Fighter"];
    assert_eq!(trained_crew(fighter).get_pilot(), 3);
  }
}

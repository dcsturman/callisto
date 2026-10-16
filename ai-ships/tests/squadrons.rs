//! Squadrons: ships told to fly together get courses that keep them together.

use std::sync::Arc;

use ai_ships::decision::{straggler_margin, to_intents, HelmOrder, HelmVerb, SquadronOrder, TeamDecision};
use ai_ships::observation::observe;
use callisto::entity::{Entities, Entity, Vec3};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates, Range, Team};
use callisto::tactics::{self, Helm};
use cgmath::Zero;

const KM: f64 = 1000.0;

/// Two Red ships side by side, and a Blue yacht they have found.
async fn pair() -> Entities {
  let templates = load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap();
  replace_ship_templates(templates.clone());
  let design = Arc::clone(&templates["Buccaneer"]);
  let mut world = Entities::new();
  for (name, x, team) in [
    ("Pirate", 0.0, Team::Red),
    ("Wingman", 300.0 * KM, Team::Red),
    ("Yacht", 400_000.0 * KM, Team::Blue),
  ] {
    world.add_ship(name.to_string(), Vec3::new(x, 0.0, 0.0), Vec3::zero(), &design, None, None);
    world.ships[name].write().unwrap().team = Some(team);
  }
  for name in ["Pirate", "Wingman"] {
    world.ships[name].write().unwrap().contacts = vec!["Yacht".to_string()];
  }
  world
}

fn squadron(ships: &[&str], guide: &str) -> TeamDecision {
  TeamDecision {
    plan: "Close together.".to_string(),
    ships: Vec::new(),
    squadrons: vec![SquadronOrder {
      ships: ships.iter().map(ToString::to_string).collect(),
      guide: guide.to_string(),
      helm: Some(HelmOrder {
        verb: HelmVerb::Intercept,
        target: Some("Yacht".to_string()),
        band: Some(Range::Medium),
        position_km: None,
        max_thrust: None,
      }),
    }],
  }
}

#[tokio::test]
async fn a_squadron_flies_together() {
  let world = pair().await;
  let side = ["Pirate".to_string(), "Wingman".to_string()];
  let view = observe(&world, &side);
  let conversion = to_intents(&squadron(&["Pirate", "Wingman"], "Pirate"), &view);
  assert!(conversion.notes.is_empty(), "{:?}", conversion.notes);

  let intent = |name: &str| &conversion.intents.iter().find(|(ship, _)| ship == name).unwrap().1;
  let drive = world.ships["Pirate"].read().unwrap().max_acceleration();
  assert!(matches!(intent("Pirate").helm, Helm::Intercept { .. }));
  assert_eq!(
    intent("Pirate").max_thrust,
    Some(f64::from(drive)),
    "in station, the guide flies at the slowest ship's full thrust"
  );
  assert!(matches!(&intent("Wingman").helm, Helm::Follow { guide, .. } if guide == "Pirate"));

  // Side by side, the follower flies the guide's own course.
  let plan = |name: &str| {
    tactics::orders(&world, name, intent(name))
      .unwrap()
      .into_iter()
      .find_map(|request| match request {
        callisto::payloads::RequestMsg::SetPlan(plan) => Some(plan.plan),
        _ => None,
      })
      .unwrap()
  };
  assert_eq!(plan("Wingman"), plan("Pirate"));
}

#[test]
fn the_margin_grows_with_what_rejoining_takes() {
  let km = |n: f64| n * KM;
  let close = |a: f64, b: f64| (a - b).abs() < 1e-9;
  assert!(close(straggler_margin(km(500.0), 10.0), 0.0), "settled: nothing");
  assert!(close(straggler_margin(km(500.0), 1_000.0), 0.25), "close but drifting");
  assert!(close(straggler_margin(km(5_000.0), 0.0), 0.75), "Medium, at rest");
  assert!(close(straggler_margin(km(20_000.0), 0.0), 1.0), "Long: the most there is");
  assert!(
    close(straggler_margin(km(2_667.0), 17_653.0), 1.0),
    "left 17.6 km/s behind in one round: a whole G, though only Medium away"
  );
}

#[tokio::test]
async fn a_straggler_costs_the_guide_thrust() {
  let world = pair().await;
  world.ships["Wingman"]
    .write()
    .unwrap()
    .set_position(Vec3::new(80_000.0 * KM, 0.0, 0.0));
  let side = ["Pirate".to_string(), "Wingman".to_string()];
  let view = observe(&world, &side);
  let conversion = to_intents(&squadron(&["Pirate", "Wingman"], "Pirate"), &view);
  let guide = &conversion.intents.iter().find(|(ship, _)| ship == "Pirate").unwrap().1;
  let drive = f64::from(world.ships["Pirate"].read().unwrap().max_acceleration());
  assert_eq!(guide.max_thrust, Some(drive - 1.0), "80,000 km behind is Distant: a whole G");
}

#[tokio::test]
async fn a_squadron_without_its_guide_is_not_formed() {
  let world = pair().await;
  let side = ["Pirate".to_string(), "Wingman".to_string()];
  let view = observe(&world, &side);
  let conversion = to_intents(&squadron(&["Pirate", "Wingman"], "Yacht"), &view);
  assert!(conversion.intents.is_empty(), "{:?}", conversion.intents);
  assert!(
    conversion.notes.iter().any(|note| note.contains("not formed")),
    "{:?}",
    conversion.notes
  );
}

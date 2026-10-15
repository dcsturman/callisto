//! Fog of war, and what the simple commanders make of it.

use std::sync::Arc;

use ai_ships::commander::{Close, Commander, Hold};
use ai_ships::observation::observe;
use callisto::entity::{Entities, Vec3};
use callisto::ship::{load_ship_templates_from_dir, replace_ship_templates, Range, Team};
use callisto::tactics::{self, Helm};
use cgmath::Zero;

const KM: f64 = 1000.0;

/// Red pirates (one ours, one a human team-mate) against a Blue yacht they
/// have found and a Blue escort they have not.
async fn ambush() -> Entities {
  let templates = load_ship_templates_from_dir("../callisto/ship_templates").await.unwrap();
  replace_ship_templates(templates.clone());
  let design = Arc::clone(&templates["Buccaneer"]);
  let mut world = Entities::new();
  for (name, x, team) in [
    ("Pirate", 0.0, Team::Red),
    ("Wingman", 100.0 * KM, Team::Red),
    ("Yacht", 20_000.0 * KM, Team::Blue),
    ("Escort", 8_000.0 * KM, Team::Blue),
  ] {
    world.add_ship(name.to_string(), Vec3::new(x, 0.0, 0.0), Vec3::zero(), &design, None, None);
    world.ships[name].write().unwrap().team = Some(team);
  }
  world.ships["Pirate"].write().unwrap().contacts = vec!["Yacht".to_string()];
  world
}

#[tokio::test]
async fn a_side_sees_only_what_its_consoles_would() {
  let world = ambush().await;
  let view = observe(&world, &["Pirate".to_string()]);

  let names = |ships: &[callisto::ship::Ship]| -> Vec<String> {
    use callisto::entity::Entity;
    ships.iter().map(|ship| ship.get_name().to_string()).collect()
  };
  assert_eq!(names(&view.own), vec!["Pirate"]);
  assert_eq!(names(&view.allies), vec!["Wingman"], "team-mates are always known");
  let contacts: Vec<&str> = view.contacts.iter().map(|contact| contact.name.as_str()).collect();
  assert_eq!(contacts, vec!["Yacht"], "the undetected escort, nearer, is not there");

  let yacht = &view.contacts[0];
  assert_eq!(yacht.design, "Buccaneer");
  assert_eq!(yacht.held_by, vec!["Pirate"]);
  assert_eq!(yacht.band, Range::Long);

  // A contact carries what the sensor station shows, and not the rest.
  let wire = serde_json::to_value(yacht).unwrap();
  for hidden in ["current_hull", "current_power", "crew", "orders", "plan"] {
    assert!(wire.get(hidden).is_none(), "a contact must not reveal {hidden}");
  }
  let whole = serde_json::to_string(&view).unwrap();
  assert!(!whole.contains("Escort"), "nothing anywhere in the view names the escort");
}

#[tokio::test]
async fn hold_fights_where_it_is_and_close_goes_to_its_guns_range() {
  let world = ambush().await;
  let view = observe(&world, &["Pirate".to_string()]);

  let held = Hold.decide(&view);
  assert_eq!(held.len(), 1);
  assert_eq!(held[0].1.helm, Helm::Hold);
  assert_eq!(held[0].1.fire[0].target, "Yacht");

  let closed = Close.decide(&view);
  assert_eq!(
    closed[0].1.helm,
    Helm::Intercept {
      target: "Yacht".to_string(),
      band: Range::Long
    },
    "pulse lasers all reach Long, so that is where to fight"
  );

  // And the orders are ones a console could send: a plan, and the fire,
  // lock, defence and repair batch. At Long the pulse lasers bear.
  let orders = tactics::orders(&world, "Pirate", &closed[0].1).unwrap();
  assert!(orders.iter().any(|o| matches!(o, callisto::payloads::RequestMsg::SetPlan(_))));
  assert!(orders
    .iter()
    .any(|o| matches!(o, callisto::payloads::RequestMsg::ModifyActions(_))));
}

#[tokio::test]
async fn with_no_enemy_in_contact_a_commander_holds() {
  let world = ambush().await;
  world.ships["Pirate"].write().unwrap().contacts.clear();
  let view = observe(&world, &["Pirate".to_string()]);
  let decided = Close.decide(&view);
  assert_eq!(decided[0].1.helm, Helm::Hold);
  assert_eq!(decided[0].1.fire, vec![]);
}

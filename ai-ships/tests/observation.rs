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

  // A contact carries what the player's screen shows -- including the hull
  // she has left, which the ship list gives for any contact -- and not the
  // rest.
  assert_eq!(
    yacht.current_hull, yacht.design_hull,
    "an undamaged contact shows her whole hull"
  );
  let wire = serde_json::to_value(yacht).unwrap();
  for hidden in ["current_power", "crew", "orders", "plan"] {
    assert!(wire.get(hidden).is_none(), "a contact must not reveal {hidden}");
  }
  // The undetected escort is a blip on the plot: where it is and how it moves,
  // nothing more, as the console shows it dimmed.
  let blips: Vec<&str> = view.blips.iter().map(|blip| blip.name.as_str()).collect();
  assert_eq!(blips, vec!["Escort"]);
  let blip = serde_json::to_value(&view.blips[0]).unwrap();
  for hidden in ["design", "weapons", "sensors", "current_hull", "crit_level", "team"] {
    assert!(blip.get(hidden).is_none(), "a blip must not reveal {hidden}");
  }
  assert!(
    view.engagements.iter().all(|e| e.contact != "Escort") && view.exposures.iter().all(|e| e.contact != "Escort"),
    "nothing can be worked out about a ship nobody has found"
  );
}

#[tokio::test]
async fn hold_fights_where_it_is_and_close_goes_to_its_guns_range() {
  let world = ambush().await;
  let view = observe(&world, &["Pirate".to_string()]);

  let held = Hold.decide(&view).await;
  assert_eq!(held.len(), 1);
  assert_eq!(held[0].1.helm, Helm::Hold);
  assert_eq!(held[0].1.fire[0].target, "Yacht");

  let closed = Close.decide(&view).await;
  let Helm::Intercept { target, band } = &closed[0].1.helm else {
    panic!("expected an intercept, got {:?}", closed[0].1.helm);
  };
  assert_eq!(target, "Yacht");
  assert!(
    *band <= Range::Long,
    "a pulse-laser ship fights where its lasers reach, not at {band:?}"
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
  let decided = Close.decide(&view).await;
  assert_eq!(decided[0].1.helm, Helm::Hold);
  assert_eq!(decided[0].1.fire, vec![]);
  assert_eq!(
    decided[0].1.posture,
    Some(callisto::tactics::Posture::Active),
    "with nobody in contact, sweep for someone"
  );
}

/// A ship whose particle barbette out-reaches the enemy's guns stands off at
/// Very Long, where it hits and cannot be hit back -- even though at Short it
/// would do twice the damage, because there it would take a bigger share of
/// its own, smaller, hull.
#[tokio::test]
async fn a_ship_that_out_reaches_its_enemy_stands_off() {
  let world = ambush().await;
  let executor = callisto::ship::get_ship_template("HMS Executor").unwrap();
  let mut world = world;
  world.add_ship(
    "Executor".to_string(),
    Vec3::new(5_000.0 * KM, 0.0, 0.0),
    Vec3::zero(),
    &executor,
    None,
    None,
  );
  {
    let mut ship = world.ships["Executor"].write().unwrap();
    ship.team = Some(Team::Gold);
    ship.contacts = vec!["Pirate".to_string()];
  }
  let view = observe(&world, &["Executor".to_string()]);

  let table = view.engagement("Executor", "Pirate").unwrap();
  let very_long =
    |odds: &[callisto::tactics::BandOdds]| odds.iter().find(|o| o.band == Range::VeryLong).unwrap().clone();
  assert!(very_long(&table.we_hit).expected_damage > 0.0);
  assert_eq!(
    very_long(&table.they_hit).mounts,
    0,
    "the pirate's lasers cannot reach Very Long"
  );

  // And it can see how quietly it has to fly to stay hidden from her.
  let exposure = view.exposures.iter().find(|e| e.contact == "Pirate").unwrap();
  let silent = exposure
    .odds
    .iter()
    .find(|o| o.posture == callisto::tactics::Posture::Silent && !o.firing)
    .unwrap();
  let loud = exposure
    .odds
    .iter()
    .find(|o| o.posture == callisto::tactics::Posture::Active && o.firing)
    .unwrap();
  assert!(silent.found < loud.found);

  assert_eq!(
    Close.decide(&view).await[0].1.helm,
    Helm::Intercept {
      target: "Pirate".to_string(),
      band: Range::VeryLong
    }
  );
}

/// "Eyes on us": a side knows which enemies hold it, found or not, as the
/// sensor station's panel shows -- even an enemy it has not found itself.
#[tokio::test]
async fn a_side_knows_who_has_found_it() {
  let world = ambush().await;
  {
    let mut escort = world.ships["Escort"].write().unwrap();
    escort.contacts = vec!["Pirate".to_string()];
    escort.sensor_locks = vec!["Pirate".to_string()];
  }
  world.ships["Yacht"].write().unwrap().contacts = vec!["Pirate".to_string()];
  let view = observe(&world, &["Pirate".to_string()]);
  let mut watching: Vec<(&str, bool)> = view.watchers.iter().map(|w| (w.by.as_str(), w.lock)).collect();
  watching.sort_unstable();
  assert_eq!(watching, vec![("Escort", true), ("Yacht", false)]);
  assert!(
    view.contacts.iter().all(|contact| contact.name != "Escort"),
    "knowing the escort has found us does not find the escort"
  );
}

//! The orders that take several steps, walked end to end through the engine:
//! jumping out of a gravity well, keeping a point of computer free for a
//! hand-off, the captain's ranking, and jamming.

use std::sync::Arc;

use callisto::entity::Entity;
use callisto::software::SoftwareKind;
use callisto::tactics::{Boost, SensorVerb};
use callisto_rl::action::{side_intents, Choice, Head, HEADS, MASK_LEN};
use callisto_rl::encode::OBS_LEN;
use callisto_rl::game::{Control, Game, Scenario};
use callisto_rl::orders;

const DESIGNS: &str = "../callisto/ship_templates";

/// A Gazelle 120,000 km from a moon of 1,000 km radius, so 80,000 km inside
/// the 200,000 km she must reach to jump; a Threshing Oar far off as the
/// other side.
const WELL: &str = r#"{
  "metadata": {"name": "Well", "description": "A test."},
  "planets": [{"name": "Rock", "position": [0.0, 0.0, 0.0], "velocity": [0.0, 0.0, 0.0],
               "color": "!moon", "radius": 1000000.0, "mass": 1e21}],
  "ships": [
    {"name": "Leaver", "position": [120000000.0, 0.0, 0.0], "velocity": [0, 0, 0],
     "plan": [[[0, 0, 0], 50000]], "design": "Gazelle", "team": "Blue", "goal": "escape",
     "crew": {"pilot": 2, "sensors": [2], "engineers": [{"jump": 2, "power": 2, "maneuver": 2, "mechanic": 1}],
              "gunnery": [2, 2, 2, 1], "leadership": 2}},
    {"name": "Picket", "position": [0.0, 120000000.0, 0.0], "velocity": [0, 0, 0],
     "plan": [[[0, 0, 0], 50000]], "design": "Fleet Picket - Watchdog", "team": "Blue", "goal": "escape",
     "crew": {"pilot": 2, "sensors": [2], "engineers": [{"jump": 2, "power": 2, "maneuver": 2, "mechanic": 1}],
              "gunnery": [2, 2, 2, 2, 2], "leadership": 1}},
    {"name": "Oar", "position": [0.0, 10000000000.0, 0.0], "velocity": [0, 0, 0],
     "plan": [[[0, 0, 0], 50000]], "design": "Threshing Oar", "team": "Red"}
  ]
}"#;

fn well_game() -> Game {
  let path = std::env::temp_dir().join(format!("callisto-rl-well-{}.json", std::process::id()));
  std::fs::write(&path, WELL).unwrap();
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let scenario = runtime.block_on(async {
    callisto::ship::replace_ship_templates(callisto::ship::load_ship_templates_from_dir(DESIGNS).await.unwrap());
    Arc::new(Scenario::load(path.to_str().unwrap()).await.unwrap())
  });
  Game::new(scenario, &[Control::Agent, Control::Agent], 1, 40)
}

/// Every slot's choice: nothing, but for these heads on these ships.
fn choices(game: &Game, wanted: &[(&str, Head, i64)]) -> Vec<Choice> {
  game
    .slots()
    .iter()
    .map(|slot| {
      let mut choice: Choice = [0; HEADS];
      for (ship, head, value) in wanted {
        if slot.ship == *ship {
          choice[*head as usize] = *value;
        }
      }
      choice
    })
    .collect()
}

fn observe(game: &mut Game) {
  let slots = game.slots().len();
  let mut obs = vec![0.0; slots * OBS_LEN];
  let mut masks = vec![false; slots * MASK_LEN];
  game.observe_into(&mut obs, &mut masks);
}

/// Order `name` out every round and report the round she was clear and the
/// round she jumped.
fn withdraw(name: &str) -> (usize, usize) {
  let mut game = well_game();
  let mut cleared = None;
  for round in 0..40 {
    observe(&mut game);
    let view = game.view(0).clone();
    let leaver = view.own.iter().find(|ship| ship.get_name() == name).cloned();
    let Some(leaver) = leaver else {
      return (cleared.expect("she was clear before she jumped"), round);
    };
    let short = orders::well(&leaver, &view).map(|(short, _)| short);
    if short.is_none() && cleared.is_none() {
      cleared = Some(round);
    }
    println!(
      "{name} round {round}: {} km to clear, {:.0} m/s, offline {:?}, jump control {}, powered {}",
      short.map_or(0.0, |s| s / 1000.0).round(),
      leaver.get_velocity().x.hypot(leaver.get_velocity().y),
      leaver.offline,
      leaver.running_level(SoftwareKind::JumpControl).is_some(),
      leaver.jump_powered()
    );
    game.step(&choices(&game, &[(name, Head::Jump, 1)]));
  }
  panic!("{name} never jumped");
}

#[test]
fn a_ship_ordered_out_climbs_clear_and_jumps() {
  let (cleared, jumped) = withdraw("Leaver");
  assert!(
    jumped <= cleared + 2,
    "a Gazelle has power to spare: she jumps as soon as she is clear"
  );
}

#[test]
fn a_ship_short_of_power_shuts_down_on_the_way_out() {
  // A Watchdog must shut down five weapons, her sensors and her drive, one
  // power check a round, to power the jump: started on the way out, so she
  // is ready soon after she is clear.
  let (cleared, jumped) = withdraw("Picket");
  assert!(jumped <= cleared + 4, "cleared in round {cleared}, jumped in round {jumped}");
}

#[test]
fn link_frees_a_point_and_fight_takes_it_back() {
  let mut game = well_game();
  let spare = |game: &Game| game.world().ships["Oar"].read().unwrap().bandwidth_spare();
  observe(&mut game);
  assert_eq!(spare(&game), 0, "a Threshing Oar comes up with her computer full");
  game.step(&choices(&game, &[("Oar", Head::Computer, 1)]));
  observe(&mut game);
  assert!(spare(&game) > 0, "Link keeps a point free");
  game.step(&choices(&game, &[("Oar", Head::Computer, 0)]));
  observe(&mut game);
  assert_eq!(spare(&game), 0, "Fight loads her back up");
}

#[test]
fn the_captain_boosts_in_the_order_ranked() {
  let mut game = well_game();
  observe(&mut game);
  let view = game.view(0).clone();
  let slot = game.slots().iter().find(|slot| slot.ship == "Leaver").unwrap();
  let ship = view.own.iter().find(|ship| ship.get_name() == "Leaver").unwrap();
  // Detection first: a search for the Oar, not yet found, leads the list.
  let detection_first = orders::RANKINGS
    .iter()
    .position(|ranking| ranking[0] == orders::Focus::Detection)
    .unwrap();
  let mut choice: Choice = [0; HEADS];
  choice[Head::Captain as usize] = i64::try_from(detection_first).unwrap();
  let intents = side_intents(&[(ship, &choice, slot.picture())], &view);
  let boosts = &intents[0].1.boosts;
  assert!(
    matches!(boosts.first(), Some(Boost::Detection { target }) if target == "Oar"),
    "{boosts:?}"
  );
  assert!(boosts.iter().any(|boost| matches!(boost, Boost::Fire { .. })), "guns follow");
}

#[test]
fn the_first_operator_can_jam_a_contact() {
  let mut game = well_game();
  observe(&mut game);
  let view = game.view(0).clone();
  let slot = game.slots().iter().find(|slot| slot.ship == "Leaver").unwrap();
  let ship = view.own.iter().find(|ship| ship.get_name() == "Leaver").unwrap();
  let mut choice: Choice = [0; HEADS];
  choice[Head::Sensor1 as usize] = 1; // jam comms
  choice[Head::Lock as usize] = 1; // on the first enemy slot
  let intents = side_intents(&[(ship, &choice, slot.picture())], &view);
  assert_eq!(
    intents[0].1.sensors,
    vec![SensorVerb::JamComms {
      target: "Oar".to_string()
    }]
  );
}

#[test]
#[ignore = "a survey, for picking test designs"]
fn survey_jump_power() {
  let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
  let templates = runtime.block_on(callisto::ship::load_ship_templates_from_dir(DESIGNS)).unwrap();
  let mut names: Vec<_> = templates.keys().collect();
  names.sort();
  for name in names {
    let design = &templates[name];
    if design.jump == 0 {
      continue;
    }
    let ship = callisto::ship::Ship::new(
      name.clone(),
      callisto::entity::Vec3::new(0.0, 0.0, 0.0),
      callisto::entity::Vec3::new(0.0, 0.0, 0.0),
      design,
      None,
      None,
    );
    if !ship.jump_powered() {
      println!("{name}: shut down {:?}", orders::shutdowns(&ship));
    }
  }
}

#[test]
fn a_withdrawal_called_off_brings_the_ship_back_up() {
  let mut game = well_game();
  let offline = |game: &Game| game.world().ships["Picket"].read().unwrap().offline.clone();
  let programs = |game: &Game| {
    let mut kinds: Vec<String> = game.world().ships["Picket"]
      .read()
      .unwrap()
      .software_running
      .iter()
      .map(|package| format!("{:?}", package.kind))
      .collect();
    kinds.sort();
    kinds
  };
  let loadout = programs(&game);
  for _ in 0..5 {
    observe(&mut game);
    game.step(&choices(&game, &[("Picket", Head::Jump, 1)]));
  }
  assert!(!offline(&game).is_empty(), "she has started shutting down");
  for _ in 0..10 {
    observe(&mut game);
    game.step(&choices(&game, &[("Picket", Head::Jump, 0)]));
  }
  assert!(
    offline(&game).is_empty(),
    "called off, everything is back: {:?}",
    offline(&game)
  );
  let picket = game.world().ships["Picket"].read().unwrap().clone();
  assert!(
    picket.running_level(SoftwareKind::JumpControl).is_none(),
    "Jump Control stopped"
  );
  drop(picket);
  assert_eq!(programs(&game), loadout, "her programs as she started");
}

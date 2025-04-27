/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */

use pretty_env_logger;

use cgmath::{assert_relative_eq, assert_ulps_eq, Zero};
use std::sync::Arc;
use test_log::test;

use assert_json_diff::assert_json_eq;
use serde_json::json;

use crate::authentication::Authenticator;
use crate::authentication::MockAuthenticator;
use crate::entity::G;
use crate::entity::{Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME_F64};
use crate::list_local_or_cloud_dir;
use crate::payloads::{AddPlanetMsg, AddShipMsg, EffectMsg, SetPilotActions, EMPTY_FIRE_ACTIONS_MSG};
use crate::player::PlayerManager;
use crate::server::Server;
use crate::ship::{ShipDesignTemplate, ShipSystem};

fn setup_authenticator() -> Box<dyn Authenticator> {
  Box::new(MockAuthenticator::new("http://test.com"))
}

async fn setup_test_with_server(authenticator: Box<dyn Authenticator>) -> PlayerManager {
  let _ = pretty_env_logger::try_init();
  crate::ship::config_test_ship_templates().await;

  let basic_server = Server::new("test", "").await;
  PlayerManager::new(0, Some(Arc::new(basic_server)), authenticator, true)
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[test(tokio::test)]
async fn test_simple_get() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;
  let body = server.get_entities_json();
  assert_eq!(body, r#"{"ships":[],"missiles":[],"planets":[],"actions":[]}"#);
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test_log::test(tokio::test)]
async fn test_add_ship() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;
  let ship = r#"{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"design":"Buccaneer","current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }"#;

  let message: AddShipMsg = serde_json::from_str(ship).unwrap();
  let response = server.add_ship(message).unwrap();

  assert_eq!(response, "Add ship action executed");

  let response = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
  let compare = json!({"ships":[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],50000]],
        "design":"Buccaneer", "current_hull":160, "current_armor":5, "current_power":300, 
        "current_maneuver":3, "current_jump":2, "current_fuel":81, "current_crew":11, 
        "current_sensors": "Improved", "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        "can_jump":false,
        "sensor_locks": [],
        }],
        "missiles":[],"planets":[],"actions":[]});

  assert_json_eq!(entities, compare);
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test(tokio::test)]
async fn test_add_planet_ship() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship =
    r#"{"name":"ship1","position":[0,2000,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let ship = r#"{"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let response = server.get_entities_json();

  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
  let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":false,
         "sensor_locks": []
        }, 
        {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":false,
         "sensor_locks": []
        }],
          "missiles":[],
          "planets":[],
          "actions":[]});

  assert_json_eq!(entities, compare);

  let planet = r#"{"name":"planet1","position":[0,0,0],"color":"red","radius":1.5e6,"mass":3e24}"#;
  let response = server.add_planet(serde_json::from_str(planet).unwrap()).unwrap();
  assert_eq!(response, "Add planet action executed");

  let response = server.get_entities_json();
  let result = serde_json::from_str::<Entities>(response.as_str()).unwrap();

  let compare = json!({"planets":[
  {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
    "color":"red","radius":1.5e6,"mass":3e24,
    "gravity_radius_1":4_518_410.048_543_495,
    "gravity_radius_05":6_389_996.771_013_086,
    "gravity_radius_025": 9_036_820.097_086_99,
    "gravity_radius_2": 3_194_998.385_506_543}],
  "missiles":[],
  "actions":[],
  "ships":[
      {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
       "current_hull":160,
       "current_armor":5,
       "current_power":300,
       "current_maneuver":3,
       "current_jump":2,
       "current_fuel":81,
       "current_crew":11,
       "current_sensors": "Improved",
       "active_weapons": [true, true, true, true],
       "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
       "dodge_thrust":0,
       "assist_gunners":false,
       "can_jump":false,
       "sensor_locks": []
      },
      {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
       "current_hull":160,
       "current_armor":5,
       "current_power":300,
       "current_maneuver":3,
       "current_jump":2,
       "current_fuel":81,
       "current_crew":11,
       "current_sensors": "Improved",
       "active_weapons": [true, true, true, true],
       "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
       "dodge_thrust":0,
       "assist_gunners":false,
       "can_jump":false,
       "sensor_locks": []
      }]});

  assert_json_eq!(result, compare);

  let planet =
    r#"{"name":"planet2","position":[1000000,0,0],"primary":"planet1", "color":"red","radius":1.5e6,"mass":1e23}"#;
  let response = server.add_planet(serde_json::from_str(planet).unwrap()).unwrap();
  assert_eq!(response, "Add planet action executed");

  let entities = server.get_entities_json();

  let start = serde_json::from_str::<Entities>(entities.as_str()).unwrap();
  let compare = json!({"missiles":[],
  "actions":[],
  "planets":[
  {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
      "color":"red","radius":1.5e6,"mass":3e24,
      "gravity_radius_1":4_518_410.048_543_495,
      "gravity_radius_05":6_389_996.771_013_086,
      "gravity_radius_025": 9_036_820.097_086_99,
      "gravity_radius_2": 3_194_998.385_506_543},
  {"name":"planet2","position":[1_000_000.0,0.0,0.0],"velocity":[0.0,0.0,14_148.851_543_499_915],
      "color":"red","radius":1.5e6,"mass":1e23,"primary":"planet1",
      "gravity_radius_025":1_649_890.071_763_523_2}],
  "ships":[
      {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
       "current_hull":160,
       "current_armor":5,
       "current_power":300,
       "current_maneuver":3,
       "current_jump":2,
       "current_fuel":81,
       "current_crew":11,
       "current_sensors": "Improved",
       "active_weapons": [true, true, true, true],
       "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
       "dodge_thrust":0,
       "assist_gunners":false,
       "can_jump":false,
       "sensor_locks": []
      },
      {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
       "plan":[[[0.0,0.0,0.0],50000]],"design":"Buccaneer",
       "current_hull":160,
       "current_armor":5,
       "current_power":300,
       "current_maneuver":3,
       "current_jump":2,
       "current_fuel":81,
       "current_crew":11,
       "current_sensors": "Improved",
       "active_weapons": [true, true, true, true],
       "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
       "dodge_thrust":0,
       "assist_gunners":false,
       "can_jump":false,
       "sensor_locks": []
      }]});

  assert_json_eq!(&start, &compare);
}

/*
 * Test that creates a ship and then updates its position.
 */
#[test(tokio::test)]
async fn test_update_ship() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship =
    r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  server.merge_actions(EMPTY_FIRE_ACTIONS_MSG);
  let response = server.update();
  assert_eq!(response, Vec::new());

  let response = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
  let ship = entities.ships.get("ship1").unwrap().read().unwrap();
  assert_eq!(ship.get_position(), Vec3::new(1000.0 * DELTA_TIME_F64, 0.0, 0.0));
  assert_eq!(ship.get_velocity(), Vec3::new(1000.0, 0.0, 0.0));
}

/*
 * Test to create two ships, launch a missile, and advance the round and see the missile move.
 *
 */
#[test(tokio::test)]
async fn test_update_missile() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let fire_missile = json!([["ship1", [{"FireAction" :{"weapon_id": 1, "target": "ship2"}}]]]).to_string();
  server.merge_actions(serde_json::from_str(&fire_missile).unwrap());
  let response = server.update();

  let compare = json!([
      {"kind": "ShipImpact","target": "ship2","position": [5000.0, 0.0, 5000.0]}
  ]);

  assert_json_eq!(
    response
      .iter()
      .filter(|e| !matches!(e, EffectMsg::Message { .. }))
      .collect::<Vec<_>>(),
    compare
  );

  let entities = server.get_entities_json();
  let compare = json!(
        {"ships":[
            {"name":"ship1","position":[360_000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"System Defense Boat",
             "current_hull":88,
             "current_armor":13,
             "current_power":240,
             "current_maneuver":9,
             "current_jump":0,
             "current_fuel":6,
             "current_crew":13,
             "current_sensors": "Improved",
             "active_weapons": [true, true],
             "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
             "dodge_thrust":0,
             "assist_gunners":false,
             "can_jump":false,
             "sensor_locks": []
            },
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],50000]],"design":"System Defense Boat",
             "current_hull":82,
             "current_armor":13,
             "current_power":240,
             "current_maneuver":9,
             "current_jump":0,
             "current_fuel":6,
             "current_crew":13,
             "current_sensors": "Improved",
             "active_weapons": [true, true],
             "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
             "dodge_thrust":0,
             "assist_gunners":false,
             "can_jump":false,
             "sensor_locks": []
            }],
             "missiles":[],"planets":[],"actions":[]});

  assert_json_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(), compare);
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[test(tokio::test)]
async fn test_remove_ship() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let response = server.remove(&"ship1".to_string()).unwrap();
  assert_eq!(response, "Remove action executed");

  let entities = server.get_entities_json();

  assert_eq!(entities, r#"{"ships":[],"missiles":[],"planets":[],"actions":[]}"#);

  // Try remove with non-existent ship
  let response = server.remove(&"ship2".to_string());
  assert!(response.is_err());
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[test(tokio::test)]
async fn test_set_acceleration() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let response = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

  let ship = entities.ships.get("ship1").unwrap().read().unwrap();
  let flight_plan = &ship.plan;
  assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
  assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
  assert!(!flight_plan.has_second());

  let response = server.set_plan(&serde_json::from_str(r#"{"name":"ship1","plan":[[[1,2,2],50000]]}"#).unwrap());
  assert!(response.is_ok());

  let response = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
  let ship = entities.ships.get("ship1").unwrap().read().unwrap();
  let flight_plan = &ship.plan;
  assert_eq!(flight_plan.0 .0, [1.0, 2.0, 2.0].into());
  assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
  assert!(!flight_plan.has_second());
}

/**
 * Test that will compute a simple path and return it, checking if the simple computation is correct.
 */
#[test(tokio::test)]
async fn test_compute_path_basic() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let path_request = r#"{"entity_name":"ship1","end_pos":[29430000,0,0],"end_vel":[0,0,0],"standoff_distance" : 0}"#;
  let plan = server.compute_path(&serde_json::from_str(path_request).unwrap()).unwrap();

  assert_eq!(plan.path.len(), 8);
  assert_eq!(plan.path[0], Vec3::zero());
  assert_ulps_eq!(
    plan.path[1],
    Vec3 {
      x: 1_906_480.8,
      y: 0.0,
      z: 0.0
    },
    epsilon = 1e-4
  );
  assert_ulps_eq!(
    plan.path[2],
    Vec3 {
      x: 7_625_923.2,
      y: 0.0,
      z: 0.0
    },
    epsilon = 1e-4
  );
  assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-7);
  let (a, t) = plan.plan.0.into();
  assert_ulps_eq!(a, Vec3 { x: 3.0, y: 0.0, z: 0.0 } * G, epsilon = 1e-7);
  assert_eq!(t, 1000);

  if let Some(accel) = plan.plan.1 {
    let (a, _t) = accel.into();
    assert_ulps_eq!(
      a,
      Vec3 {
        x: -3.0,
        y: 0.0,
        z: 0.0
      } * G,
      epsilon = 1e-7
    );
  } else {
    panic!("Expecting second acceleration.")
  }
  assert_eq!(t, 1000);
}

#[test(tokio::test)]
async fn test_compute_path_with_standoff() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let plan = server
    .compute_path(
      &serde_json::from_str(
        r#"{"entity_name":"ship1","end_pos":[58842000,0,0],"end_vel":[0,0,0],"standoff_distance" : 60000}"#,
      )
      .unwrap(),
    )
    .unwrap();

  assert_eq!(plan.path.len(), 10);
  assert_eq!(plan.path[0], Vec3::zero());
  assert_relative_eq!(
    plan.path[1],
    Vec3 {
      x: 1_906_480.8,
      y: 0.0,
      z: 0.0
    },
    epsilon = 1e-5
  );
  assert_ulps_eq!(
    plan.path[2],
    Vec3 {
      x: 7_625_923.2,
      y: 0.0,
      z: 0.0
    },
    epsilon = 1e-4
  );
  assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-5);
  let (a, t) = plan.plan.0.into();
  assert_ulps_eq!(a, Vec3 { x: 3.0, y: 0.0, z: 0.0 } * G, epsilon = 1e-5);
  assert_eq!(t, 1413);

  if let Some(accel) = plan.plan.1 {
    let (a, _t) = accel.into();
    assert_ulps_eq!(
      a,
      Vec3 {
        x: -3.0,
        y: 0.0,
        z: 0.0
      } * G,
      epsilon = 1e-7
    );
  } else {
    panic!("Expecting second acceleration.")
  }
  assert_eq!(t, 1413);
}

#[test(tokio::test)]
async fn test_exhausted_missile() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Create two ships with one to fire at the other.
  let ship =
    r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Put second ship far way (out of range of a missile)
  let ship2 =
    r#"{"name":"ship2","position":[1e10,0,1e10],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Fire a missile
  let fire_actions = json!([["ship1", [{"FireAction" : {"weapon_id": 1, "target": "ship2"}}] ]]).to_string();
  server.merge_actions(serde_json::from_str(&fire_actions).unwrap());
  let response = server.update();

  // First round 3 missiles are launched due to triple turret
  assert_eq!(response.len(), 1);
  assert!(
    matches!(&response[0], EffectMsg::Message { content } if content == "ship1 launches 3 missile(s) at ship2."),
    "Round 0"
  );

  // Second to 8th round nothing happens.
  for round in 0..9 {
    server.merge_actions(EMPTY_FIRE_ACTIONS_MSG);
    let response = server.update();
    assert_eq!(response, Vec::new(), "Round {round}");
  }

  // 9th round missile should exhaust itself.
  let response = server.update();
  assert_eq!(
    response
      .iter()
      .filter(|e| matches!(e, EffectMsg::ExhaustedMissile { .. }))
      .count(),
    3,
    "Round 9"
  );
}

#[test(tokio::test)]
async fn test_destroy_ship() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Make this a very weak ship
  let ship2 =
    r#"{"name":"ship2","position":[5e4,0,5e4],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Scout/Courier"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Pummel the weak ship.
  let fire_actions = json!([["ship1", [
      {"FireAction" : {"weapon_id": 0, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 1, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 2, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 3, "target": "ship2"}},
  ]]])
  .to_string();

  server.merge_actions(serde_json::from_str(&fire_actions).unwrap());
  let effects = server.update();

  // For this test we don't worry about all the specific damage effects, but just check for messages related to
  // ship destruction.
  assert!(effects.contains(&EffectMsg::ShipDestroyed {
    position: Vec3::new(50000.0, 0.0, 50000.0)
  }));
  assert!(effects.contains(&EffectMsg::Message {
    content: "ship2 destroyed.".to_string()
  }));
}

#[test(tokio::test)]
async fn test_called_shot() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Gazelle class is a good test for this as it has 2 Particle Barbettes (likely to cause a crit) and 2 triple beams (also capable of called shots)
  // Give it a good gunner (skill 4) and sensor lock on ship2
  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle", "sensor_locks":["ship2"], "crew":{"gunnery":[7, 6, 6, 6]}}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Make this a big ship to reduce sustained damage crits.
  let ship2 =
    r#"{"name":"ship2","position":[5e4,0,5e4],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Midu Agasham"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  
  // Pummel the weak ship.
  let fire_actions = json!([["ship1", [
      {"FireAction" : {"weapon_id": 0, "target": "ship2", "called_shot_system": Some(ShipSystem::Maneuver)}},
      {"FireAction" : {"weapon_id": 1, "target": "ship2", "called_shot_system": Some(ShipSystem::Maneuver)}},
      {"FireAction" : {"weapon_id": 2, "target": "ship2", "called_shot_system": Some(ShipSystem::Maneuver)}},
      {"FireAction" : {"weapon_id": 3, "target": "ship2", "called_shot_system": Some(ShipSystem::Maneuver)}}
  ]]])
  .to_string();

  server.merge_actions(serde_json::from_str(&fire_actions).unwrap());
  let effects = server.update();

  // First ensure there is at least one critical hit that matches.
  assert!(
    effects.iter().any(
      |e| matches!(e, EffectMsg::Message { content } if content.contains("maneuver") && content.contains("critical"))
    ),
    "No critical hits to called shot area: maneuver"
  );

  // Second ensure 6 critical hits to maneuver and the rest to hull.
  // This means we find all messages with the word "critical" but not the word "caused" (the latter are damage effects)
  let crits = effects
    .iter()
    .filter(
      |e| matches!(e, EffectMsg::Message { content } if content.contains("critical") && !content.contains("caused")),
    )
    .collect::<Vec<_>>();
  assert_eq!(
    crits
      .iter()
      .filter(|e| matches!(e, EffectMsg::Message { content } if content.contains("maneuver")))
      .count(),
    4,
    "Expected 4 critical hits to maneuver: {crits:#?}"
  );
}

#[test(tokio::test)]
async fn test_big_fight() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let ship2 =
    r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let fire_actions = json!([["ship1", [
      {"FireAction" : {"weapon_id": 0, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 1, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 2, "target": "ship2"}},
      {"FireAction" :{"weapon_id": 3, "target": "ship2"}},
  ]],
  ["ship2", [
      {"FireAction" : {"weapon_id": 0, "target": "ship1"}},
      {"FireAction" : {"weapon_id": 1, "target": "ship1"}},
      {"FireAction" : {"weapon_id": 2, "target": "ship1"}},
      {"FireAction" :{"weapon_id": 3, "target": "ship1"}},
  ]]]);

  server.merge_actions(serde_json::from_str(&fire_actions.to_string()).unwrap());
  let mut effects = server.update();

  let compare = json!([
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]}
  ]);
  effects = effects
    .iter()
    .filter_map(|e| {
      if matches!(e, EffectMsg::Message { .. }) {
        None
      } else {
        Some(e.clone())
      }
    })
    .collect::<Vec<EffectMsg>>();

  effects.sort_by_key(|a| serde_json::to_string(a).unwrap());

  assert_json_eq!(
    effects
      .iter()
      .filter(|e| !matches!(e, EffectMsg::Message { .. }))
      .collect::<Vec<_>>(),
    compare
  );

  let entities = server.get_entities_json();
  let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Gazelle",
         "current_hull":96,"current_armor":3,
         "current_power":540,"current_maneuver":4,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":true,
         "sensor_locks": []
        },
        {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Gazelle",
         "current_hull":135,"current_armor":3,
         "current_power":540,"current_maneuver":6,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":true,
         "sensor_locks": []
        }],
          "missiles":[],
          "planets":[],
          "actions":[]});
  assert_json_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(), compare);
}

#[test(tokio::test)]
async fn test_fight_with_crew() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Ship 1 has a capable crew.
  let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle", 
        "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 1, 1]}}"#;

  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Now have that capable crew do something.
  let crew_actions = r#"{"ship_name":"ship1","dodge_thrust":3,"assist_gunners":true}"#;
  let response = server.set_pilot_actions(&serde_json::from_str(crew_actions).unwrap()).unwrap();

  assert_eq!(response, "Set crew action executed");

  // Ship 2 has no crew skills
  let ship2 =
    r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
  let response = server.add_ship(serde_json::from_str(ship2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let fire_actions = json!([["ship1", [
      {"FireAction" : {"weapon_id": 0, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 1, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 2, "target": "ship2"}},
      {"FireAction" : {"weapon_id": 3, "target": "ship2"}},
  ]],
  ["ship2", [
      {"FireAction" : {"weapon_id": 0, "target": "ship1"}},
      {"FireAction" : {"weapon_id": 1, "target": "ship1"}},
      {"FireAction" : {"weapon_id": 2, "target": "ship1"}},
      {"FireAction" : {"weapon_id": 3, "target": "ship1"}},
  ]]]);

  server.merge_actions(serde_json::from_str(&fire_actions.to_string()).unwrap());
  let mut effects = server.update();

  let compare = json!([
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
      {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]}
  ]);

  effects = effects
    .iter()
    .filter_map(|e| {
      if matches!(e, EffectMsg::Message { .. }) {
        None
      } else {
        Some(e.clone())
      }
    })
    .collect::<Vec<EffectMsg>>();

  effects.sort_by_key(|a| serde_json::to_string(a).unwrap());

  assert_json_eq!(
    effects
      .iter()
      .filter(|e| !matches!(e, EffectMsg::Message { .. }))
      .collect::<Vec<_>>(),
    compare
  );

  let entities = server.get_entities_json();
  let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Gazelle",
         "current_hull":166,"current_armor":3,
         "current_power":540,"current_maneuver":6,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 1, 1]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":true,
         "sensor_locks": []
        },
        {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],"design":"Gazelle",
         "current_hull":61,"current_armor":3,
         "current_power":540,"current_maneuver":5,
         "current_jump":4,"current_fuel":126,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,false,true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
         "can_jump":true,
         "sensor_locks": []
        }],
          "missiles":[],
          "planets":[],
          "actions":[]});
  assert_json_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(), compare);
}

#[test(tokio::test)]
async fn test_slugfest() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Destroyer also has a professional crew! Though deployed nonsensically as missiles don't get benefit from gunner skill.
  // Boost weapon #10 as its firing a pules laser at the harrier.
  let destroyer = r#"{"name":"Evil Destroyer","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Midu Agasham",
        "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 4, 1, 1, 1, 1]}}"#;

  let response = server.add_ship(serde_json::from_str(destroyer).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Destroyer pilot will aid gunners
  let crew_actions = r#"{"ship_name":"Evil Destroyer","assist_gunners":true}"#;
  let response = server.set_pilot_actions(&serde_json::from_str(crew_actions).unwrap()).unwrap();
  assert_eq!(response, "Set crew action executed");

  let harrier =
    r#"{"name":"Harrier","position":[5000,0,4000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Harrier"}"#;
  let response = server.add_ship(serde_json::from_str(harrier).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let buc1 =
    r#"{"name":"Buc1","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(buc1).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let buc2 =
    r#"{"name":"Buc2","position":[4000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(buc2).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let fire_actions = json!([["Evil Destroyer", [
      {"FireAction" : {"weapon_id": 0, "target": "Harrier"}},
      {"FireAction" : {"weapon_id": 1, "target": "Harrier"}},
      {"FireAction" : {"weapon_id": 2, "target": "Buc1"}},
      {"FireAction" : {"weapon_id": 3, "target": "Buc1"}},
      {"FireAction" : {"weapon_id": 4, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 5, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 6, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 7, "target": "Buc1"}},
      {"FireAction" : {"weapon_id": 8, "target": "Buc1"}},
      {"FireAction" : {"weapon_id": 9, "target": "Buc1"}},
      {"FireAction" : {"weapon_id": 10, "target": "Harrier"}},
      {"FireAction" : {"weapon_id": 11, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 12, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 13, "target": "Buc2"}},
      {"FireAction" : {"weapon_id": 14, "target": "Harrier"}},
      ]],
  ["Harrier", [
      {"FireAction" : {"weapon_id": 0, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 1, "target": "Evil Destroyer"}}]],
  ["Buc1", [
      {"FireAction" : {"weapon_id": 0, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 1, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 2, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 3, "target": "Evil Destroyer"}},
      ]],
  ["Buc2", [
      {"FireAction" : {"weapon_id": 0, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 1, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 2, "target": "Evil Destroyer"}},
      {"FireAction" : {"weapon_id": 3, "target": "Evil Destroyer"}},
      ]]
  ]);

  server.merge_actions(serde_json::from_str(&fire_actions.to_string()).unwrap());
  let _response = server.update();

  let response = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

  // Should only have 3 ships now as the Harrier should have been destroyed
  assert_eq!(
    entities.ships.len(),
    3,
    "Was expecting only 3 ships to survive instead of {}",
    entities.ships.len()
  );
  assert!(!entities.ships.contains_key("Harrier"), "Harrier should have been destroyed.");
}

#[test(tokio::test)]
async fn test_get_entities() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Test getting entities from an empty server
  let empty_entities = server.get_entities();
  assert!(empty_entities.ships.is_empty());
  assert!(empty_entities.planets.is_empty());
  assert!(empty_entities.missiles.is_empty());

  // Add a ship to the server
  let ship_name = "TestShip".to_string();
  let ship_position = Vec3::new(1.0, 2.0, 3.0);
  let ship_velocity = Vec3::new(4.0, 5.0, 6.0);
  server
    .add_ship(AddShipMsg {
      name: ship_name.clone(),
      position: ship_position,
      velocity: ship_velocity,
      design: ShipDesignTemplate::default().name.to_string(),
      crew: None,
    })
    .unwrap();

  // Add a planet to the server
  let planet_name = "TestPlanet".to_string();
  let planet_position = Vec3::new(10.0, 20.0, 30.0);
  let planet_color = "blue".to_string();
  server
    .add_planet(AddPlanetMsg {
      name: planet_name.clone(),
      position: planet_position,
      color: planet_color.clone(),
      primary: None,
      radius: 6371e3,
      mass: 5.97e24,
    })
    .unwrap();

  // Test getting entities after adding a ship and a planet
  let entities = server.get_entities();

  // Check the ship
  assert_eq!(entities.ships.len(), 1);
  let ship = entities.ships.get(&ship_name).unwrap().read().unwrap();
  assert_eq!(ship.get_name(), ship_name);
  assert_eq!(ship.get_position(), ship_position);
  assert_eq!(ship.get_velocity(), ship_velocity);

  // Check the planet
  assert_eq!(entities.planets.len(), 1);
  let planet = entities.planets.get(&planet_name).unwrap().read().unwrap();
  assert_eq!(planet.get_name(), planet_name);
  assert_eq!(planet.get_position(), planet_position);
  assert_eq!(planet.color, planet_color);

  // Check that there are no missiles
  assert!(entities.missiles.is_empty());
}

// Test for get_designs in server.
#[test(tokio::test)]
async fn test_get_designs() {
  let authenticator = setup_authenticator();
  let _ = setup_test_with_server(authenticator).await;
  let designs = PlayerManager::get_designs();
  assert!(!designs.is_empty());
  assert!(designs.contains_key("Buccaneer"));
}

#[test(tokio::test)]
async fn test_missile_impact_close() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Add the firing ship
  let firing_ship =
    r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(firing_ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Add the target ship very close to the firing ship
  let target_ship = r#"{"name":"ship2","position":[1000,1000,1000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(target_ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Fire a missile within impact range.
  let fire_missile = json!([["ship1", [{"FireAction" : {"weapon_id": 1, "target": "ship2"}}]]]).to_string();
  server.merge_actions(serde_json::from_str(&fire_missile).unwrap());
  let effects = server.update();

  // Check for impact effect
  assert!(
    effects.iter().any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
    "Expected ShipImpact effect, but got: {effects:?}"
  );

  // Ensure no ExhaustedMissile effect
  assert!(
    !effects.iter().any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
    "Unexpected ExhaustedMissile effect"
  );

  // Check that the target ship took damage
  let entities = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(&entities).unwrap();
  let target_ship = entities.ships.get("ship2").unwrap().read().unwrap();
  assert!(
    target_ship.get_current_hull_points() < target_ship.get_max_hull_points(),
    "Target ship should have taken damage"
  );

  // Add the target ship very close to the firing ship but not in impact range.
  let target_ship = r#"{"name":"ship2","position":[4000000,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
  let response = server.add_ship(serde_json::from_str(target_ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Fire a missile that should get there in one round.
  let fire_missile = json!([["ship1", [{"FireAction" : {"weapon_id": 1, "target": "ship2"}}]]]).to_string();
  server.merge_actions(serde_json::from_str(&fire_missile).unwrap());
  let effects = server.update();

  // Check for impact effect
  assert!(
    effects.iter().any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
    "Expected ShipImpact effect, but got: {effects:?}"
  );

  // Ensure no ExhaustedMissile effect
  assert!(
    !effects.iter().any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
    "Unexpected ExhaustedMissile effect"
  );

  // Check that the target ship took damage
  let entities = server.get_entities_json();
  let entities = serde_json::from_str::<Entities>(&entities).unwrap();
  let target_ship = entities.ships.get("ship2").unwrap().read().unwrap();
  assert!(
    target_ship.get_current_hull_points() < target_ship.get_max_hull_points(),
    "Target ship should have taken damage"
  );
}

#[test(tokio::test)]
async fn test_set_agility() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Add a ship to the server
  let ship =
    r#"{"name":"agile_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Set agility for the ship
  let mut agility_request = SetPilotActions::new("agile_ship");
  agility_request.dodge_thrust = Some(1);

  let result = server.set_pilot_actions(&agility_request);
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Set crew action executed");

  // Verify the ship's agility has been updated
  let entities = server.get_entities();
  let ship = entities.ships.get("agile_ship").unwrap().read().unwrap();
  assert_eq!(ship.get_dodge_thrust(), 1);

  // Test setting agility with an invalid (too high) value
  let mut invalid_agility_request = SetPilotActions::new("agile_ship");
  invalid_agility_request.dodge_thrust = Some(11);

  let result = server.set_pilot_actions(&invalid_agility_request);
  assert!(result.is_err());
  assert_eq!(result.unwrap_err(), "Thrust 11 exceeds max acceleration 3.".to_string());

  // Test setting agility for a non-existent ship
  let mut non_existent_ship_request = SetPilotActions::new("non_existent_ship");
  non_existent_ship_request.dodge_thrust = Some(1);

  let result = server.set_pilot_actions(&non_existent_ship_request);
  assert!(result.is_err());
}

#[test(tokio::test)]
async fn test_set_pilot_actions_aid_gunner() {
  let authenticator = setup_authenticator();
  let server = setup_test_with_server(authenticator).await;

  // Add a ship to the server
  let ship =
    r#"{"name":"test_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  // Set crew actions for the ship, enabling aid_gunner
  let mut crew_actions = SetPilotActions::new("test_ship");
  crew_actions.assist_gunners = Some(true);

  let result = server.set_pilot_actions(&crew_actions);
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Set crew action executed");

  // Verify the ship's crew actions have been updated
  let entities = server.get_entities();
  let ship = entities.ships.get("test_ship").unwrap().read().unwrap();
  assert!(ship.get_assist_gunners());

  // Now disable aid_gunner
  let mut crew_actions = SetPilotActions::new("test_ship");
  crew_actions.assist_gunners = Some(false);

  let result = server.set_pilot_actions(&crew_actions);
  assert!(result.is_ok());
  assert_eq!(result.unwrap(), "Set crew action executed");

  // Verify the ship's crew actions have been updated
  let entities = server.get_entities();
  let ship = entities.ships.get("test_ship").unwrap().read().unwrap();
  assert!(!ship.get_assist_gunners());

  // Verify you cannot assist_gunner if there isn't enough thrust
  let ship =
    r#"{"name":"slow_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Free Trader"}"#;
  let response = server.add_ship(serde_json::from_str(ship).unwrap()).unwrap();
  assert_eq!(response, "Add ship action executed");

  let mut crew_actions = SetPilotActions::new("slow_ship");
  crew_actions.assist_gunners = Some(true);
  crew_actions.dodge_thrust = Some(1);

  let result = server.set_pilot_actions(&crew_actions);
  assert!(result.is_err());
  assert_eq!(
    result.unwrap_err(),
    "No thrust available to reserve for assisting gunners".to_string()
  );

  // Test setting crew actions for a non-existent ship
  let mut non_existent_ship_actions = SetPilotActions::new("non_existent_ship");
  non_existent_ship_actions.assist_gunners = Some(true);

  let result = server.set_pilot_actions(&non_existent_ship_actions);
  assert!(result.is_err());
}

#[test(tokio::test)]
async fn test_list_local_dir() {
  let directory_path = "./scenarios";
  // Call the function
  let result = list_local_or_cloud_dir(directory_path).await.unwrap();

  // Verify results
  assert!(result.len() > 2);
}

#[test(tokio::test)]
#[cfg_attr(feature = "ci", ignore)]
async fn test_list_gcs_dir() {
  // This test requires actual GCS credentials
  // Skip in CI environments

  // Create a test bucket name and directory
  let test_bucket = "callisto-scenarios";
  let test_dir = format!("gs://{test_bucket}");

  // Call the function
  let result = list_local_or_cloud_dir(&test_dir).await;

  // Just verify that the function runs without error
  // We can't predict the actual contents
  assert!(result.is_ok(), "Failed to list GCS directory: {:?}", result.unwrap_err());
  assert!(result.unwrap().len() > 2, "Expected at least 3 files in the GCS directory");
}

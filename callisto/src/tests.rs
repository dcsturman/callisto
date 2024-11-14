/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */

use pretty_env_logger;

use cgmath::{assert_ulps_eq, Zero};
use rand::SeedableRng;
use std::sync::{Arc, Mutex};

use assert_json_diff::assert_json_eq;
use serde_json::json;

use crate::entity::{Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME};
use crate::payloads::{AddPlanetMsg, AddShipMsg, EffectMsg, FlightPathMsg, EMPTY_FIRE_ACTIONS_MSG};
use crate::server::Server;
use crate::ship::ShipDesignTemplate;

fn setup_test_with_server() -> Server {
    let _ = pretty_env_logger::try_init();
    crate::ship::config_test_ship_templates();

    Server::new(
        Arc::new(Mutex::new(Entities::new())),
        Box::new(rand::rngs::SmallRng::seed_from_u64(0)),
    )
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[test]
fn test_simple_get() {
    let server = setup_test_with_server();
    let body = server.get().unwrap();
    assert_eq!(body, r#"{"ships":[],"missiles":[],"planets":[]}"#);
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test]
fn test_add_ship() {
    let server = setup_test_with_server();
    let ship = r#"{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"design":"Buccaneer","current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
    let compare = json!({"ships":[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer", "current_hull":160, "current_armor":5, "current_power":300, "current_maneuver":3, "current_jump":2, "current_fuel":81, "current_crew":11, "current_sensors": "Improved", "active_weapons": [true, true, true, true]}],"missiles":[],"planets":[]});

    assert_json_eq!(entities, compare);
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test]
fn test_add_planet_ship() {
    let server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,2000,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let ship = r#"{"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.get().unwrap();

    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }, 
        {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }],
          "missiles":[],
          "planets":[]});

    assert_json_eq!(entities, compare);

    let planet =
        r#"{"name":"planet1","position":[0,0,0],"color":"red","radius":1.5e6,"mass":3e24}"#;
    let response = server
        .add_planet(serde_json::from_str(planet).unwrap())
        .unwrap();
    assert_eq!(response, "Add planet action executed");

    let response = server.get().unwrap();
    let result = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    let compare = json!({"planets":[
    {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
      "color":"red","radius":1.5e6,"mass":3e24,
      "gravity_radius_1":4518410.048543495,
      "gravity_radius_05":6389996.771013086,
      "gravity_radius_025": 9036820.09708699,
      "gravity_radius_2": 3194998.385506543}],
    "missiles":[],
    "ships":[
        {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        },
        {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }]});

    assert_json_eq!(result, compare);

    let planet = r#"{"name":"planet2","position":[1000000,0,0],"primary":"planet1", "color":"red","radius":1.5e6,"mass":1e23}"#;
    let response = server
        .add_planet(serde_json::from_str(planet).unwrap())
        .unwrap();
    assert_eq!(response, "Add planet action executed");

    let entities = server.get().unwrap();

    let start = serde_json::from_str::<Entities>(entities.as_str()).unwrap();
    let compare = json!({"missiles":[],
    "planets":[
    {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
        "color":"red","radius":1.5e6,"mass":3e24,
        "gravity_radius_1":4518410.048543495,
        "gravity_radius_05":6389996.771013086,
        "gravity_radius_025": 9036820.09708699,
        "gravity_radius_2": 3194998.385506543},
    {"name":"planet2","position":[1000000.0,0.0,0.0],"velocity":[0.0,0.0,14148.851543499915],
        "color":"red","radius":1.5e6,"mass":1e23,"primary":"planet1",
        "gravity_radius_025":1649890.0717635232}],
    "ships":[
        {"name":"ship1","position":[0.0,2000.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        },
        {"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Buccaneer",
         "current_hull":160,
         "current_armor":5,
         "current_power":300,
         "current_maneuver":3,
         "current_jump":2,
         "current_fuel":81,
         "current_crew":11,
         "current_sensors": "Improved",
         "active_weapons": [true, true, true, true]
        }]});

    assert_json_eq!(&start, &compare);
}

/*
 * Test that creates a ship and then updates its position.
 */
#[test]
fn test_update_ship() {
    let mut server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.update(EMPTY_FIRE_ACTIONS_MSG).unwrap();
    assert_eq!(response, r#"[]"#);

    let response = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    assert_eq!(
        ship.get_position(),
        Vec3::new(1000.0 * DELTA_TIME as f64, 0.0, 0.0)
    );
    assert_eq!(ship.get_velocity(), Vec3::new(1000.0, 0.0, 0.0));
}

/*
 * Test to create two ships, launch a missile, and advance the round and see the missile move.
 *
 */
#[test]
fn test_update_missile() {
    let mut server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, r"Add ship action executed");

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]).to_string();
    let response = server
        .update(serde_json::from_str(&fire_missile).unwrap())
        .unwrap();

    let compare = json!([
        {"kind": "ShipImpact","position":[5000.0,0.0,5000.0]}
    ]);

    assert_json_eq!(
        serde_json::from_str::<Vec<EffectMsg>>(response.as_str())
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, EffectMsg::Message { .. }))
            .collect::<Vec<_>>(),
        compare
    );

    let entities = server.get().unwrap();
    let compare = json!(
        {"ships":[
            {"name":"ship1","position":[360000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"System Defense Boat",
             "current_hull":88,
             "current_armor":13,
             "current_power":240,
             "current_maneuver":9,
             "current_jump":0,
             "current_fuel":6,
             "current_crew":13,
             "current_sensors": "Improved",
             "active_weapons": [true, true]},
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"System Defense Boat",
             "current_hull":82,
             "current_armor":13,
             "current_power":240,
             "current_maneuver":9,
             "current_jump":0,
             "current_fuel":6,
             "current_crew":13,
             "current_sensors": "Improved",
             "active_weapons": [true, true]}],
             "missiles":[],"planets":[]});

    assert_json_eq!(
        serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        compare
    );
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[test]
fn test_remove_ship() {
    let server = setup_test_with_server();
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.remove("ship1".to_string()).unwrap();
    assert_eq!(response, "Remove action executed");

    let entities = server.get().unwrap();

    assert_eq!(entities, r#"{"ships":[],"missiles":[],"planets":[]}"#);

    // Try remove with non-existent ship
    let response = server.remove("ship2".to_string());
    assert!(response.is_err());
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[test]
fn test_set_acceleration() {
    let server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    let flight_plan = &ship.plan;
    assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
    assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
    assert!(!flight_plan.has_second());

    let response = server
        .set_plan(serde_json::from_str(r#"{"name":"ship1","plan":[[[1,2,2],10000]]}"#).unwrap());
    assert!(response.is_ok());

    let response = server.get().unwrap();
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
#[test]
fn test_compute_path_basic() {
    let server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let path_request = r#"{"entity_name":"ship1","end_pos":[29430000,0,0],"end_vel":[0,0,0],"standoff_distance" : 0}"#;
    let response = server
        .compute_path(serde_json::from_str(&path_request).unwrap())
        .unwrap();
    let plan = serde_json::from_str::<FlightPathMsg>(response.as_str()).unwrap();

    assert_eq!(plan.path.len(), 7);
    assert_eq!(plan.path[0], Vec3::zero());
    assert_ulps_eq!(
        plan.path[1],
        Vec3 {
            x: 1906480.8,
            y: 0.0,
            z: 0.0
        }
    );
    assert_ulps_eq!(
        plan.path[2],
        Vec3 {
            x: 7625923.2,
            y: 0.0,
            z: 0.0
        }
    );
    assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-7);
    let (a, t) = plan.plan.0.into();
    assert_ulps_eq!(
        a,
        Vec3 {
            x: 3.0,
            y: 0.0,
            z: 0.0
        }
    );
    assert_eq!(t, 1000);

    if let Some(accel) = plan.plan.1 {
        let (a, _t) = accel.into();
        assert_ulps_eq!(
            a,
            Vec3 {
                x: -3.0,
                y: 0.0,
                z: 0.0
            }
        );
    } else {
        panic!("Expecting second acceleration.")
    }
    assert_eq!(t, 1000);
}

#[test]
fn test_compute_path_with_standoff() {
    let server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let response = server.compute_path(serde_json::from_str(r#"{"entity_name":"ship1","end_pos":[58842000,0,0],"end_vel":[0,0,0],"standoff_distance" : 60000}"#).unwrap()).unwrap();
    let plan = serde_json::from_str::<FlightPathMsg>(response.as_str()).unwrap();

    assert_eq!(plan.path.len(), 9);
    assert_eq!(plan.path[0], Vec3::zero());
    assert_ulps_eq!(
        plan.path[1],
        Vec3 {
            x: 1906480.8,
            y: 0.0,
            z: 0.0
        }
    );
    assert_ulps_eq!(
        plan.path[2],
        Vec3 {
            x: 7625923.2,
            y: 0.0,
            z: 0.0
        }
    );
    assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-7);
    let (a, t) = plan.plan.0.into();
    assert_ulps_eq!(
        a,
        Vec3 {
            x: 3.0,
            y: 0.0,
            z: 0.0
        }
    );
    assert_eq!(t, 1413);

    if let Some(accel) = plan.plan.1 {
        let (a, _t) = accel.into();
        assert_ulps_eq!(
            a,
            Vec3 {
                x: -3.0,
                y: 0.0,
                z: 0.0
            }
        );
    } else {
        panic!("Expecting second acceleration.")
    }
    assert_eq!(t, 1413);
}

#[test]
fn test_exhausted_missile() {
    let mut server = setup_test_with_server();

    // Create two ships with one to fire at the other.
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Put second ship far way (out of range of a missile)
    let ship2 = r#"{"name":"ship2","position":[1e10,0,1e10],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Fire a missile
    let fire_actions = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]).to_string();
    let response = server
        .update(serde_json::from_str(&fire_actions).unwrap())
        .unwrap();

    // First round 3 missiles are launched due to triple turret
    assert_eq!(
        response, "[{\"kind\":\"Message\",\"content\":\"ship1 launches 3 missile(s) at ship2.\"}]",
        "Round 0"
    );

    // Second to 9th round nothing happens.
    for round in 0..9 {
        let response = server.update(EMPTY_FIRE_ACTIONS_MSG).unwrap();
        assert_eq!(response, "[]", "Round {}", round);
    }

    // 10th round missile should exhaust itself.
    let response = server.update(EMPTY_FIRE_ACTIONS_MSG).unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<EffectMsg>>(response.as_str())
            .unwrap()
            .iter()
            .filter(|e| matches!(e, EffectMsg::ExhaustedMissile { .. }))
            .count(),
        3,
        "Round 9"
    );
}

#[test]
fn test_destroy_ship() {
    let mut server = setup_test_with_server();
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Make this a very weak ship
    let ship2 = r#"{"name":"ship2","position":[5e4,0,5e4],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Scout/Courier"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Pummel the weak ship.
    let fire_actions = json!([["ship1", [
        {"weapon_id": 0, "target": "ship2"},
        {"weapon_id": 1, "target": "ship2"},
        {"weapon_id": 2, "target": "ship2"},
        {"weapon_id": 3, "target": "ship2"},
    ]]])
    .to_string();

    let response = server
        .update(serde_json::from_str(&fire_actions).unwrap())
        .unwrap();

    // For this test we don't worry about all the specific damage effects, but just check for messages related to
    // ship destruction.
    let effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();

    assert!(effects.contains(&EffectMsg::ShipDestroyed {
        position: Vec3::new(50000.0, 0.0, 50000.0)
    }));
    assert!(effects.contains(&EffectMsg::Message {
        content: "ship2 destroyed.".to_string()
    }));
}

#[test]
fn test_big_fight() {
    let mut server = setup_test_with_server();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let fire_actions = json!([["ship1", [
        {"weapon_id": 0, "target": "ship2"},
        {"weapon_id": 1, "target": "ship2"},
        {"weapon_id": 2, "target": "ship2"},
        {"weapon_id": 3, "target": "ship2"},
    ]],
    ["ship2", [
        {"weapon_id": 0, "target": "ship1"},
        {"weapon_id": 1, "target": "ship1"},
        {"weapon_id": 2, "target": "ship1"},
        {"weapon_id": 3, "target": "ship1"},
    ]]]);

    let response = server
        .update(serde_json::from_str(&fire_actions.to_string()).unwrap())
        .unwrap();

    let compare = json!([
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
        {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
        {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]},
        {"kind":"BeamHit","origin":[5000.0,0.0,5000.0],"position":[0.0,0.0,0.0]}
    ]);
    let mut effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();
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

    effects.sort_by(|a, b| {
        serde_json::to_string(a)
            .unwrap()
            .cmp(&serde_json::to_string(b).unwrap())
    });

    assert_json_eq!(
        effects
            .iter()
            .filter(|e| !matches!(e, EffectMsg::Message { .. }))
            .collect::<Vec<_>>(),
        compare
    );

    let entities = server.get().unwrap();
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":96,"current_armor":3,
         "current_power":540,"current_maneuver":4,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true]},
        {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":135,"current_armor":3,
         "current_power":540,"current_maneuver":6,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true]},
         ],
          "missiles":[],
          "planets":[]});
    assert_json_eq!(
        serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        compare
    );
}

#[test_log::test]
fn test_slugfest() {
    let mut server = setup_test_with_server();

    let destroyer = r#"{"name":"destroyer","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Midu Agasham"}"#;
    let response = server
        .add_ship(serde_json::from_str(destroyer).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let harrier = r#"{"name":"harrier","position":[5000,0,4000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Harrier"}"#;
    let response = server
        .add_ship(serde_json::from_str(harrier).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let buc1 = r#"{"name":"buc1","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(buc1).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let buc2 = r#"{"name":"buc2","position":[4000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(buc2).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    let fire_actions = json!([["destroyer", [
        {"weapon_id": 0, "target": "harrier"},
        {"weapon_id": 1, "target": "buc1"},
        {"weapon_id": 2, "target": "buc1"},
        {"weapon_id": 3, "target": "buc1"},
        {"weapon_id": 4, "target": "buc2"},
        {"weapon_id": 5, "target": "buc2"},
        {"weapon_id": 6, "target": "buc2"},
        {"weapon_id": 7, "target": "buc1"},
        {"weapon_id": 8, "target": "buc1"},
        {"weapon_id": 9, "target": "buc1"},
        {"weapon_id": 10, "target": "harrier"},
        {"weapon_id": 11, "target": "buc2"},
        {"weapon_id": 12, "target": "buc2"},
        {"weapon_id": 13, "target": "buc2"},
        {"weapon_id": 14, "target": "harrier"},
        ]],
    ["harrier", [
        {"weapon_id": 0, "target": "destroyer"},
        {"weapon_id": 1, "target": "destroyer"}]],
    ["buc1", [
        {"weapon_id": 0, "target": "destroyer"},
        {"weapon_id": 1, "target": "destroyer"},
        {"weapon_id": 2, "target": "destroyer"},
        {"weapon_id": 3, "target": "destroyer"},
        ]],
    ["buc2", [
        {"weapon_id": 0, "target": "destroyer"},
        {"weapon_id": 1, "target": "destroyer"},
        {"weapon_id": 2, "target": "destroyer"},
        {"weapon_id": 3, "target": "destroyer"},
        ]]
    ]);

    let _response = server
        .update(serde_json::from_str(&fire_actions.to_string()).unwrap())
        .unwrap();

    let response = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    // Should only have 3 ships now as the Harrier should have been destroyed
    assert_eq!(entities.ships.len(), 3);
}

#[test]
fn test_get_entities() {
    let server = setup_test_with_server();

    // Test getting entities from an empty server
    let result = server.get_entities();
    assert!(result.is_ok());
    let empty_entities = result.unwrap();
    assert!(empty_entities.ships.is_empty());
    assert!(empty_entities.planets.is_empty());
    assert!(empty_entities.missiles.is_empty());

    // Add a ship to the server
    let ship_name = "TestShip".to_string();
    let ship_position = Vec3::new(1.0, 2.0, 3.0);
    let ship_velocity = Vec3::new(4.0, 5.0, 6.0);
    let ship_acceleration = Vec3::new(0.0, 0.0, 0.0);
    server
        .add_ship(AddShipMsg {
            name: ship_name.clone(),
            position: ship_position,
            velocity: ship_velocity,
            acceleration: ship_acceleration,
            design: ShipDesignTemplate::default().name.to_string(),
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
    let result = server.get_entities();
    assert!(result.is_ok());
    let entities = result.unwrap();

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
#[test]
fn test_get_designs() {
    let server = setup_test_with_server();
    let result = server.get_designs();
    assert!(result.is_ok());
    let designs = result.unwrap();
    assert!(designs.len() > 0);
    assert!(designs.contains("Buccaneer"));
}

#[test]
fn test_missile_impact_close() {
    let mut server = setup_test_with_server();

    // Add the firing ship
    let firing_ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(firing_ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Add the target ship very close to the firing ship
    let target_ship = r#"{"name":"ship2","position":[1000,1000,1000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(target_ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Fire a missile within impact range.
    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}]]]).to_string();
    let response = server
        .update(serde_json::from_str(&fire_missile).unwrap())
        .unwrap();

    // Check for impact effect
    let effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
        "Expected ShipImpact effect, but got: {:?}",
        effects
    );

    // Ensure no ExhaustedMissile effect
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
        "Unexpected ExhaustedMissile effect"
    );

    // Check that the target ship took damage
    let entities = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(&entities).unwrap();
    let target_ship = entities.ships.get("ship2").unwrap().read().unwrap();
    assert!(
        target_ship.get_current_hull_points() < target_ship.get_max_hull_points(),
        "Target ship should have taken damage"
    );

    // Add the target ship very close to the firing ship but not in impact range.
    let target_ship = r#"{"name":"ship2","position":[4000000,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(target_ship).unwrap())
        .unwrap();
    assert_eq!(response, "Add ship action executed");

    // Fire a missile that should get there in one round.
    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}]]]).to_string();
    let response = server
        .update(serde_json::from_str(&fire_missile).unwrap())
        .unwrap();

    // Check for impact effect
    let effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
        "Expected ShipImpact effect, but got: {:?}",
        effects
    );

    // Ensure no ExhaustedMissile effect
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
        "Unexpected ExhaustedMissile effect"
    );

    // Check that the target ship took damage
    let entities = server.get().unwrap();
    let entities = serde_json::from_str::<Entities>(&entities).unwrap();
    let target_ship = entities.ships.get("ship2").unwrap().read().unwrap();
    assert!(
        target_ship.get_current_hull_points() < target_ship.get_max_hull_points(),
        "Target ship should have taken damage"
    );
}

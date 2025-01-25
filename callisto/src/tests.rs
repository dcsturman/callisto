/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */

use pretty_env_logger;

use cgmath::{assert_ulps_eq, Zero};
use std::sync::{Arc, Mutex};
use test_log::test;

use assert_json_diff::assert_json_eq;
use serde_json::json;

use crate::authentication::Authenticator;
use crate::authentication::MockAuthenticator;
use crate::entity::{Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME};
use crate::payloads::{
    AddPlanetMsg, AddShipMsg, EffectMsg, FlightPathMsg, LoadScenarioMsg, SetCrewActions,
    EMPTY_FIRE_ACTIONS_MSG,
};
use crate::server::{msg_json, Server};
use crate::ship::ShipDesignTemplate;

async fn setup_test_with_server() -> Server {
    let _ = pretty_env_logger::try_init();
    crate::ship::config_test_ship_templates().await;

    let mock_auth = MockAuthenticator::new(
        "http://test.com",
        "secret".to_string(),
        "users.txt",
        "http://web.test.com".to_string(),
    );
    let authenticator = Arc::new(Box::new(mock_auth) as Box<dyn Authenticator>);

    Server::new(Arc::new(Mutex::new(Entities::new())), authenticator, true)
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[test(tokio::test)]
async fn test_simple_get() {
    let server = setup_test_with_server().await;
    let body = server.get_entities_json();
    assert_eq!(body, r#"{"ships":[],"missiles":[],"planets":[]}"#);
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test(tokio::test)]
async fn test_add_ship() {
    let server = setup_test_with_server().await;
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
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.get_entities_json();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
    let compare = json!({"ships":[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"plan":[[[0.0,0.0,0.0],10000]],
        "design":"Buccaneer", "current_hull":160, "current_armor":5, "current_power":300, 
        "current_maneuver":3, "current_jump":2, "current_fuel":81, "current_crew":11, 
        "current_sensors": "Improved", "active_weapons": [true, true, true, true],
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
        "dodge_thrust":0,
        "assist_gunners":false,
        }],
        "missiles":[],"planets":[]});

    assert_json_eq!(entities, compare);
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test(tokio::test)]
async fn test_add_planet_ship() {
    let server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,2000,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let ship = r#"{"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.get_entities_json();

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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }],
          "missiles":[],
          "planets":[]});

    assert_json_eq!(entities, compare);

    let planet =
        r#"{"name":"planet1","position":[0,0,0],"color":"red","radius":1.5e6,"mass":3e24}"#;
    let response = server
        .add_planet(serde_json::from_str(planet).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add planet action executed"));

    let response = server.get_entities_json();
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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }]});

    assert_json_eq!(result, compare);

    let planet = r#"{"name":"planet2","position":[1000000,0,0],"primary":"planet1", "color":"red","radius":1.5e6,"mass":1e23}"#;
    let response = server
        .add_planet(serde_json::from_str(planet).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add planet action executed"));

    let entities = server.get_entities_json();

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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }]});

    assert_json_eq!(&start, &compare);
}

/*
 * Test that creates a ship and then updates its position.
 */
#[test(tokio::test)]
async fn test_update_ship() {
    let mut server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.update(EMPTY_FIRE_ACTIONS_MSG);
    assert_eq!(response, r#"[]"#);

    let response = server.get_entities_json();
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
#[test(tokio::test)]
async fn test_update_missile() {
    let mut server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]).to_string();
    let response = server.update(serde_json::from_str(&fire_missile).unwrap());

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

    let entities = server.get_entities_json();
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
             "active_weapons": [true, true],
             "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
             "dodge_thrust":0,
             "assist_gunners":false,
            },
            {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
             "plan":[[[0.0,0.0,0.0],10000]],"design":"System Defense Boat",
             "current_hull":83,
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
            }],
             "missiles":[],"planets":[]});

    assert_json_eq!(
        serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        compare
    );
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[test(tokio::test)]
async fn test_remove_ship() {
    let server = setup_test_with_server().await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.remove(&"ship1".to_string()).unwrap();
    assert_eq!(response, msg_json("Remove action executed"));

    let entities = server.get_entities_json();

    assert_eq!(entities, r#"{"ships":[],"missiles":[],"planets":[]}"#);

    // Try remove with non-existent ship
    let response = server.remove(&"ship2".to_string());
    assert!(response.is_err());
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[test(tokio::test)]
async fn test_set_acceleration() {
    let server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.get_entities_json();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    let flight_plan = &ship.plan;
    assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
    assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
    assert!(!flight_plan.has_second());

    let response = server
        .set_plan(&serde_json::from_str(r#"{"name":"ship1","plan":[[[1,2,2],10000]]}"#).unwrap());
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
    let server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let path_request = r#"{"entity_name":"ship1","end_pos":[29430000,0,0],"end_vel":[0,0,0],"standoff_distance" : 0}"#;
    let response = server
        .compute_path(&serde_json::from_str(path_request).unwrap())
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

#[test(tokio::test)]
async fn test_compute_path_with_standoff() {
    let server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let response = server.compute_path(&serde_json::from_str(r#"{"entity_name":"ship1","end_pos":[58842000,0,0],"end_vel":[0,0,0],"standoff_distance" : 60000}"#).unwrap()).unwrap();
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

#[test(tokio::test)]
async fn test_exhausted_missile() {
    let mut server = setup_test_with_server().await;

    // Create two ships with one to fire at the other.
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Put second ship far way (out of range of a missile)
    let ship2 = r#"{"name":"ship2","position":[1e10,0,1e10],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Fire a missile
    let fire_actions = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]).to_string();
    let response = server.update(serde_json::from_str(&fire_actions).unwrap());

    // First round 3 missiles are launched due to triple turret
    assert_eq!(
        response, "[{\"kind\":\"Message\",\"content\":\"ship1 launches 3 missile(s) at ship2.\"}]",
        "Round 0"
    );

    // Second to 9th round nothing happens.
    for round in 0..9 {
        let response = server.update(EMPTY_FIRE_ACTIONS_MSG);
        assert_eq!(response, "[]", "Round {round}");
    }

    // 10th round missile should exhaust itself.
    let response = server.update(EMPTY_FIRE_ACTIONS_MSG);
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

#[test(tokio::test)]
async fn test_destroy_ship() {
    let mut server = setup_test_with_server().await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Make this a very weak ship
    let ship2 = r#"{"name":"ship2","position":[5e4,0,5e4],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Scout/Courier"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Pummel the weak ship.
    let fire_actions = json!([["ship1", [
        {"weapon_id": 0, "target": "ship2"},
        {"weapon_id": 1, "target": "ship2"},
        {"weapon_id": 2, "target": "ship2"},
        {"weapon_id": 3, "target": "ship2"},
    ]]])
    .to_string();

    let response = server.update(serde_json::from_str(&fire_actions).unwrap());

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

#[test(tokio::test)]
async fn test_big_fight() {
    let mut server = setup_test_with_server().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

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

    let response = server.update(serde_json::from_str(&fire_actions.to_string()).unwrap());

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

    let entities = server.get_entities_json();
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":96,"current_armor":3,
         "current_power":540,"current_maneuver":4,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        },
        {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":135,"current_armor":3,
         "current_power":540,"current_maneuver":6,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }],
          "missiles":[],
          "planets":[]});
    assert_json_eq!(
        serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        compare
    );
}

#[test(tokio::test)]
async fn test_fight_with_crew() {
    let mut server = setup_test_with_server().await;

    // Ship 1 has a capable crew.
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle", 
        "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 1, 1]}}"#;

    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Now have that capable crew do something.
    let crew_actions = r#"{"ship_name":"ship1","dodge_thrust":3,"assist_gunners":true}"#;
    let response = server
        .set_crew_actions(&serde_json::from_str(crew_actions).unwrap())
        .unwrap();

    assert_eq!(response, msg_json("Set crew action executed"));

    // Ship 2 has no crew skills
    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Gazelle"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

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

    let response = server.update(serde_json::from_str(&fire_actions.to_string()).unwrap());

    let compare = json!([
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
        {"kind":"BeamHit","origin":[0.0,0.0,0.0],"position":[5000.0,0.0,5000.0]},
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

    let entities = server.get_entities_json();
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":169,"current_armor":3,
         "current_power":540,"current_maneuver":6,
         "current_jump":5,"current_fuel":128,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,true,true],
         "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 1, 1]},
         "dodge_thrust":0,
         "assist_gunners":false,
        },
        {"name":"ship2","position":[5000.0,0.0,5000.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],10000]],"design":"Gazelle",
         "current_hull":47,"current_armor":3,
         "current_power":540,"current_maneuver":5,
         "current_jump":4,"current_fuel":126,
         "current_crew":21,"current_sensors":"Military",
         "active_weapons":[true,true,false,false],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }],
          "missiles":[],
          "planets":[]});
    assert_json_eq!(
        serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        compare
    );
}

#[test(tokio::test)]
async fn test_slugfest() {
    let mut server = setup_test_with_server().await;

    // Destroyer also has a professional crew! Though deployed nonsensically as missiles don't get benefit from gunner skill.
    // Boost weapon #10 as its firing a pules laser at the harrier.
    let destroyer = r#"{"name":"Evil Destroyer","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Midu Agasham",
        "crew":{"pilot":3,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[2, 2, 2, 2, 2, 1, 1, 1, 1, 1, 4, 1, 1, 1, 1]}}"#;

    let response = server
        .add_ship(serde_json::from_str(destroyer).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Destroyer pilot will aid gunners
    let crew_actions = r#"{"ship_name":"Evil Destroyer","assist_gunners":true}"#;
    let response = server
        .set_crew_actions(&serde_json::from_str(crew_actions).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Set crew action executed"));

    let harrier = r#"{"name":"Harrier","position":[5000,0,4000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Harrier"}"#;
    let response = server
        .add_ship(serde_json::from_str(harrier).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let buc1 = r#"{"name":"Buc1","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(buc1).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let buc2 = r#"{"name":"Buc2","position":[4000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(buc2).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let fire_actions = json!([["Evil Destroyer", [
        {"weapon_id": 0, "target": "Harrier"},
        {"weapon_id": 1, "target": "Harrier"},
        {"weapon_id": 2, "target": "Buc1"},
        {"weapon_id": 3, "target": "Buc1"},
        {"weapon_id": 4, "target": "Buc2"},
        {"weapon_id": 5, "target": "Buc2"},
        {"weapon_id": 6, "target": "Buc2"},
        {"weapon_id": 7, "target": "Buc1"},
        {"weapon_id": 8, "target": "Buc1"},
        {"weapon_id": 9, "target": "Buc1"},
        {"weapon_id": 10, "target": "Harrier"},
        {"weapon_id": 11, "target": "Buc2"},
        {"weapon_id": 12, "target": "Buc2"},
        {"weapon_id": 13, "target": "Buc2"},
        {"weapon_id": 14, "target": "Harrier"},
        ]],
    ["Harrier", [
        {"weapon_id": 0, "target": "Evil Destroyer"},
        {"weapon_id": 1, "target": "Evil Destroyer"}]],
    ["Buc1", [
        {"weapon_id": 0, "target": "Evil Destroyer"},
        {"weapon_id": 1, "target": "Evil Destroyer"},
        {"weapon_id": 2, "target": "Evil Destroyer"},
        {"weapon_id": 3, "target": "Evil Destroyer"},
        ]],
    ["Buc2", [
        {"weapon_id": 0, "target": "Evil Destroyer"},
        {"weapon_id": 1, "target": "Evil Destroyer"},
        {"weapon_id": 2, "target": "Evil Destroyer"},
        {"weapon_id": 3, "target": "Evil Destroyer"},
        ]]
    ]);

    let _response = server.update(serde_json::from_str(&fire_actions.to_string()).unwrap());

    let response = server.get_entities_json();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    // Should only have 3 ships now as the Harrier should have been destroyed
    assert_eq!(
        entities.ships.len(),
        3,
        "Was expecting only 3 ships to surive instead of {}",
        entities.ships.len()
    );
    assert!(
        !entities.ships.contains_key("Harrier"),
        "Harrier should have been destroyed."
    );
}

#[test(tokio::test)]
async fn test_get_entities() {
    let server = setup_test_with_server().await;

    // Test getting entities from an empty server
    let empty_entities = server.get_entities();
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
    let server = setup_test_with_server().await;
    let designs = server.get_designs();
    assert!(!designs.is_empty());
    assert!(designs.contains("Buccaneer"));
}

#[test(tokio::test)]
async fn test_missile_impact_close() {
    let mut server = setup_test_with_server().await;

    // Add the firing ship
    let firing_ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(firing_ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Add the target ship very close to the firing ship
    let target_ship = r#"{"name":"ship2","position":[1000,1000,1000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = server
        .add_ship(serde_json::from_str(target_ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Fire a missile within impact range.
    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}]]]).to_string();
    let response = server.update(serde_json::from_str(&fire_missile).unwrap());

    // Check for impact effect
    let effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
        "Expected ShipImpact effect, but got: {effects:?}"
    );

    // Ensure no ExhaustedMissile effect
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
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
    let response = server
        .add_ship(serde_json::from_str(target_ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Fire a missile that should get there in one round.
    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}]]]).to_string();
    let response = server.update(serde_json::from_str(&fire_missile).unwrap());

    // Check for impact effect
    let effects = serde_json::from_str::<Vec<EffectMsg>>(response.as_str()).unwrap();
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ShipImpact { .. })),
        "Expected ShipImpact effect, but got: {effects:?}"
    );

    // Ensure no ExhaustedMissile effect
    assert!(
        !effects
            .iter()
            .any(|e| matches!(e, EffectMsg::ExhaustedMissile { .. })),
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
    let server = setup_test_with_server().await;

    // Add a ship to the server
    let ship = r#"{"name":"agile_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Set agility for the ship
    let mut agility_request = SetCrewActions::new("agile_ship");
    agility_request.dodge_thrust = Some(1);

    let result = server.set_crew_actions(&agility_request);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), msg_json("Set crew action executed"));

    // Verify the ship's agility has been updated
    let entities = server.get_entities();
    let ship = entities.ships.get("agile_ship").unwrap().read().unwrap();
    assert_eq!(ship.get_dodge_thrust(), 1);

    // Test setting agility with an invalid (too high) value
    let mut invalid_agility_request = SetCrewActions::new("agile_ship");
    invalid_agility_request.dodge_thrust = Some(11);

    let result = server.set_crew_actions(&invalid_agility_request);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "Thrust 11 exceeds max acceleration 3.".to_string()
    );

    // Test setting agility for a non-existent ship
    let mut non_existent_ship_request = SetCrewActions::new("non_existent_ship");
    non_existent_ship_request.dodge_thrust = Some(1);

    let result = server.set_crew_actions(&non_existent_ship_request);
    assert!(result.is_err());
}

#[test(tokio::test)]
async fn test_set_crew_actions_aid_gunner() {
    let server = setup_test_with_server().await;

    // Add a ship to the server
    let ship = r#"{"name":"test_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    // Set crew actions for the ship, enabling aid_gunner
    let mut crew_actions = SetCrewActions::new("test_ship");
    crew_actions.assist_gunners = Some(true);

    let result = server.set_crew_actions(&crew_actions);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), msg_json("Set crew action executed"));

    // Verify the ship's crew actions have been updated
    let entities = server.get_entities();
    let ship = entities.ships.get("test_ship").unwrap().read().unwrap();
    assert!(ship.get_assist_gunners());

    // Now disable aid_gunner
    let mut crew_actions = SetCrewActions::new("test_ship");
    crew_actions.assist_gunners = Some(false);

    let result = server.set_crew_actions(&crew_actions);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), msg_json("Set crew action executed"));

    // Verify the ship's crew actions have been updated
    let entities = server.get_entities();
    let ship = entities.ships.get("test_ship").unwrap().read().unwrap();
    assert!(!ship.get_assist_gunners());

    // Verify you cannot assist_gunner if there isn't enough thrust
    let ship = r#"{"name":"slow_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Free Trader"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let mut crew_actions = SetCrewActions::new("slow_ship");
    crew_actions.assist_gunners = Some(true);
    crew_actions.dodge_thrust = Some(1);

    let result = server.set_crew_actions(&crew_actions);
    assert!(result.is_err());
    assert_eq!(
        result.unwrap_err(),
        "No thrust available to reserve for assisting gunners".to_string()
    );

    // Test setting crew actions for a non-existent ship
    let mut non_existent_ship_actions = SetCrewActions::new("non_existent_ship");
    non_existent_ship_actions.assist_gunners = Some(true);

    let result = server.set_crew_actions(&non_existent_ship_actions);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_load_scenario() {
    // Create server and configure ship templates
    let server = setup_test_with_server().await;

    // First add some ships and planets to the server
    let ship = r#"{"name":"test_ship","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = server
        .add_ship(serde_json::from_str(ship).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add ship action executed"));

    let planet = r#"{"name":"test_planet","position":[1000000,0,0],"color":"red","radius":1.5e6,"mass":1e23}"#;
    let response = server
        .add_planet(serde_json::from_str(planet).unwrap())
        .unwrap();
    assert_eq!(response, msg_json("Add planet action executed"));

    // Verify initial entities exist
    let initial_entities = server.get_entities();
    assert!(initial_entities.ships.contains_key("test_ship"));
    assert!(initial_entities.planets.contains_key("test_planet"));
    assert_eq!(initial_entities.ships.len(), 1);
    assert_eq!(initial_entities.planets.len(), 1);

    // Load the sol scenario
    let load_msg = LoadScenarioMsg {
        scenario_name: "./scenarios/sol.json".to_string(),
    };

    let result = server.load_scenario(&load_msg).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), msg_json("Load scenario action executed"));

    // Verify the scenario was loaded correctly and previous entities are gone
    let entities = server.get_entities();

    // Verify previous entities are gone
    assert!(!entities.ships.contains_key("test_ship"));
    assert!(!entities.planets.contains_key("test_planet"));

    // Verify sol scenario planets are present
    assert!(entities.planets.contains_key("Sun"));
    assert!(entities.planets.contains_key("Earth"));
    assert!(entities.planets.contains_key("Mars"));

    {
        // Check some specific properties of the Sun
        let sun = entities.planets.get("Sun").unwrap().read().unwrap();
        assert_eq!(sun.get_name(), "Sun");
        assert_eq!(sun.get_position(), Vec3::new(-149.6e9, 0.0, 0.0));
    }

    {
        // Check Earth's properties
        let earth = entities.planets.get("Earth").unwrap().read().unwrap();
        assert_eq!(earth.get_name(), "Earth");
        assert_eq!(earth.get_position(), Vec3::new(0.0, 0.0, 0.0));
    }

    // Test loading non-existent scenario
    let invalid_msg = LoadScenarioMsg {
        scenario_name: "non_existent_scenario.json".to_string(),
    };
    let result = server.load_scenario(&invalid_msg).await;
    assert!(result.is_err());
}

/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */
extern crate callisto;

use std::collections::HashMap;

use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

use assert_json_diff::assert_json_eq;
use serde_json::json;

use callisto::entity::{Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME};
use callisto::payloads::{FlightPathMsg, EMPTY_FIRE_ACTIONS_MSG};
use callisto::ship::ShipDesignTemplate;
use cgmath::{assert_ulps_eq, Zero};
use pretty_env_logger;

const SERVER_ADDRESS: &str = "127.0.0.1";
const SERVER_PATH: &str = "./target/debug/callisto";
const GET_ENTITIES_PATH: &str = "";
const GET_DESIGNS_PATH: &str = "designs";
const UPDATE_ENTITIES_PATH: &str = "update";
const COMPUTE_PATH_PATH: &str = "compute_path";
const ADD_SHIP_PATH: &str = "add_ship";
const ADD_PLANET_PATH: &str = "add_planet";
//const LAUNCH_MISSILE_PATH: &str = "launch_missile";
const REMOVE_ENTITY_PATH: &str = "remove";
const SET_ACCELERATION_PATH: &str = "set_plan";
const INVALID_PATH: &str = "unknown";

/**
 * Spawns a callisto server and returns a handle to it.  Used across tests to get a server up and running.
 * @param port The port to run the server on.
 * @return A handle to the running server.  This is critical as otherwise with kill_on_drop the server will be killed before the tests complete.
 */
async fn spawn_test_server(port: u16) -> Child {
    let handle = Command::new(SERVER_PATH)
        .arg("-t")
        .arg("-p")
        .arg(port.to_string())
        .arg("-n")
        .kill_on_drop(true)
        .spawn()
        .expect("Daemon failed to start.");

    let _ = pretty_env_logger::try_init();

    sleep(Duration::from_millis(500)).await;

    handle
}

fn path(port: u16, verb: &str) -> String {
    format!("http://{}:{}/{}", SERVER_ADDRESS, port, verb)
}

/**
 * Test for get_designs in server.
 */
#[tokio::test]
async fn integration_get_designs() {
    const PORT: u16 = 3010;
    let _server = spawn_test_server(PORT).await;

    let body = reqwest::get(path(PORT, GET_DESIGNS_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(body.len() > 0);
    let result = serde_json::from_str::<HashMap<String, ShipDesignTemplate>>(body.as_str());
    assert!(
        result.is_ok(),
        "Unable to deserialize designs with body: {} , Error {:?}",
        body,
        result.unwrap_err()
    );
    let designs = result.unwrap();
    assert!(designs.len() > 0, "Received empty design list.");
    assert!(
        designs.contains_key("Buccaneer"),
        "Buccaneer not found in designs."
    );
    assert!(
        designs.get("Buccaneer").unwrap().name == "Buccaneer",
        "Buccaneer body malformed in design file."
    );
}
/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[tokio::test]
async fn integration_simple_get() {
    const PORT: u16 = 3011;
    let _server = spawn_test_server(PORT).await;

    let body = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body, r#"{"ships":[],"missiles":[],"planets":[]}"#);
}

/**
 * Test that we get a 404 response when we request a path that doesn't exist.
 */
#[tokio::test]
async fn integration_simple_unknown() {
    const PORT: u16 = 3012;
    let _server = spawn_test_server(PORT).await;

    let response = reqwest::get(path(PORT, INVALID_PATH)).await.unwrap();

    assert!(
        response.status().is_client_error(),
        "Instead of expected 404 got {:?}",
        response
    );
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[tokio::test]
async fn integration_add_ship() {
    const PORT: u16 = 3013;
    let _server = spawn_test_server(PORT).await;
    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates();

    let ship = r#"{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
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
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[tokio::test]
async fn integration_add_planet_ship() {
    const PORT: u16 = 3014;
    let _server = spawn_test_server(PORT).await;
    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates();

    let ship = r#"{"name":"ship1","position":[0,2000,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let ship = r#"{"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_PLANET_PATH))
        .body(planet)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add planet action executed" }"#);

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let result = serde_json::from_str::<Entities>(entities.as_str()).unwrap();

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
             "active_weapons": [true, true, true, true]},
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
             "active_weapons": [true, true, true, true]}]});

    assert_json_eq!(result, compare);

    let planet = r#"{"name":"planet2","position":[1000000,0,0],"primary":"planet1", "color":"red","radius":1.5e6,"mass":1e23}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_PLANET_PATH))
        .body(planet)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add planet action executed" }"#);

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
         "active_weapons": [true, true, true, true]},
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
         "active_weapons": [true, true, true, true]}]});

    assert_json_eq!(&start, &compare);
}

/*
 * Test that creates a ship and then updates its position.
 */
#[tokio::test]
async fn integration_update_ship() {
    const PORT: u16 = 3015;
    let _server = spawn_test_server(PORT).await;
    callisto::ship::config_test_ship_templates();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::Client::new()
        .post(path(PORT, UPDATE_ENTITIES_PATH))
        .body(serde_json::to_string(&EMPTY_FIRE_ACTIONS_MSG).unwrap())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"[]"#);

    let response = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
#[tokio::test]
async fn integration_update_missile() {
    const PORT: u16 = 3016;
    let _server = spawn_test_server(PORT).await;
    callisto::ship::config_test_ship_templates();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship2)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]);
    let body = serde_json::to_string(&fire_missile).unwrap();
    //let missile = r#"{"source":"ship1","target":"ship2"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, UPDATE_ENTITIES_PATH))
        .body(body)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let compare = json!([
        {"kind": "ShipImpact","position":[5000.0,0.0,5000.0]}
    ]);

    assert_json_eq!(
        serde_json::from_str::<Vec<callisto::payloads::EffectMsg>>(response.as_str())
            .unwrap()
            .iter()
            .filter(|e| !matches!(e, callisto::payloads::EffectMsg::Message { .. }))
            .collect::<Vec<_>>(),
        compare
    );

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
#[tokio::test]
async fn integration_remove_ship() {
    const PORT: u16 = 3017;
    let _server = spawn_test_server(PORT).await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::Client::new()
        .post(path(PORT, REMOVE_ENTITY_PATH))
        .body(r#""ship1""#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Remove action executed" }"#);

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(entities, r#"{"ships":[],"missiles":[],"planets":[]}"#);
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[tokio::test]
async fn integration_set_acceleration() {
    const PORT: u16 = 3018;
    let _server = spawn_test_server(PORT).await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    callisto::ship::config_test_ship_templates();
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    let flight_plan = &ship.plan;
    assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
    assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
    assert!(!flight_plan.has_second());

    let response = reqwest::Client::new()
        .post(path(PORT, SET_ACCELERATION_PATH))
        .body(r#"{"name":"ship1","plan":[[[1,2,2],10000]]}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(
        response,
        r#"{ "msg" : "Set acceleration action executed" }"#
    );
    let response = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
#[tokio::test]
async fn integration_compute_path_basic() {
    const PORT: u16 = 3019;
    let _server = spawn_test_server(PORT).await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::Client::new()
        .post(path(PORT, COMPUTE_PATH_PATH))
        .body(r#"{"entity_name":"ship1","end_pos":[58842000,0,0],"end_vel":[0,0,0],"standoff_distance" : 0}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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
    assert_eq!(t, 1414);

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
    assert_eq!(t, 1414);
}

#[tokio::test]
async fn integration_compute_path_with_standoff() {
    const PORT: u16 = 3020;
    let _server = spawn_test_server(PORT).await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_SHIP_PATH))
        .body(ship)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(response, r#"{ "msg" : "Add ship action executed" }"#);

    let response = reqwest::Client::new()
        .post(path(PORT, COMPUTE_PATH_PATH))
        .body(r#"{"entity_name":"ship1","end_pos":[58842000,0,0],"end_vel":[0,0,0],"standoff_distance": 60000}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

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

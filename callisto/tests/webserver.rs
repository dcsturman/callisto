/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */
extern crate callisto;

use std::collections::HashMap;
use std::env::var;
use std::io;

use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

use assert_json_diff::assert_json_eq;
use hyper::header::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
};

use reqwest::Client;
use reqwest::Method;
use reqwest::StatusCode;
use serde_json::json;

use callisto::debug;
use callisto::entity::{Entities, Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME};
use callisto::payloads::{FlightPathMsg, SimpleMsg, EMPTY_FIRE_ACTIONS_MSG};
use callisto::ship::ShipDesignTemplate;

use cgmath::{assert_ulps_eq, Zero};

const SERVER_ADDRESS: &str = "127.0.0.1";
const SERVER_PATH: &str = "target/debug/callisto";

// GET verbs
const GET_ENTITIES_PATH: &str = "entities";
const GET_DESIGNS_PATH: &str = "designs";
// POST verbs
const LOGIN_PATH: &str = "login";
const UPDATE_ENTITIES_PATH: &str = "update";
const COMPUTE_PATH_PATH: &str = "compute_path";
const ADD_SHIP_PATH: &str = "add_ship";
const ADD_PLANET_PATH: &str = "add_planet";
const REMOVE_ENTITY_PATH: &str = "remove";
const SET_ACCELERATION_PATH: &str = "set_plan";
const SET_CREW_ACTIONS_PATH: &str = "set_crew_actions";
const INVALID_PATH: &str = "unknown";
const LOAD_SCENARIO_PATH: &str = "load_scenario";
const QUIT_PATH: &str = "quit";

async fn spawn_server(
    port: u16,
    test_mode: bool,
    scenario: Option<String>,
    design_file: Option<String>,
    auto_kill: bool,
) -> Result<Child, io::Error> {
    let mut handle = Command::new(SERVER_PATH);
    let mut handle = handle
        .env(
            "RUST_LOG",
            var("RUST_LOG").unwrap_or_else(|_| "".to_string()),
        )
        .env(
            "RUSTFLAGS",
            var("RUSTFLAGS").unwrap_or_else(|_| "".to_string()),
        )
        .env(
            "CARGO_LLVM_COV",
            var("CARGO_LLVM_COV").unwrap_or_else(|_| "".to_string()),
        )
        .env(
            "CARGO_LLVM_COV_SHOW_ENV",
            var("CARGO_LLVM_COV_SHOW_ENV").unwrap_or_else(|_| "".to_string()),
        )
        .env(
            "CARGO_LLVM_COV_TARGET_DIR",
            var("CARGO_LLVM_COV_TARGET_DIR").unwrap_or_else(|_| "".to_string()),
        )
        .arg("-p")
        .arg(port.to_string())
        .kill_on_drop(auto_kill);
    if test_mode {
        handle = handle.arg("-t");
    }
    if let Some(scenario) = scenario {
        handle = handle.arg("-f").arg(scenario);
    }
    if let Some(design_file) = design_file {
        handle = handle.arg("-d").arg(design_file);
    }

    let handle = handle.spawn()?;
    let _ = pretty_env_logger::try_init();

    sleep(Duration::from_millis(1000)).await;

    Ok(handle)
}
/**
 * Spawns a callisto server and returns a handle to it.  Used across tests to get a server up and running.
 * @param port The port to run the server on.
 * @return A handle to the running server.  This is critical as otherwise with kill_on_drop the server will be killed before the tests complete.
 */
async fn spawn_test_server(port: u16) -> Result<Child, io::Error> {
    spawn_server(port, true, None, None, false).await
}

/**
 * Send a quit message to cleanly end a test.
 */
async fn send_quit(port: u16, cookie: &str) {
    let client = Client::new();

    // We ignore the error back from the reqwest as its expected.
    #[allow(unused_must_use)]
    client
        .get(path(port, QUIT_PATH))
        .header("Cookie", cookie)
        .send()
        .await;
}

fn path(port: u16, verb: &str) -> String {
    format!("http://{}:{}/{}", SERVER_ADDRESS, port, verb)
}

/**
 * Do authentication with the test server
 * Return the user name and the key from SetCookie
 */
async fn test_authenticate(port: u16) -> Result<(String, String), String> {
    let client = Client::new();
    let response = client
        .post(path(port, LOGIN_PATH))
        .body(r#"{"code":"test_code"}"#)
        .send()
        .await
        .unwrap();
    let cookie = response
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let email = response.text().await.unwrap();
    Ok((email, cookie))
}

/**
 * Test for get_designs in server.
 */
#[tokio::test]
async fn integration_get_designs() {
    const PORT: u16 = 3010;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = Client::new();
    let body = client
        .get(path(PORT, GET_DESIGNS_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(!body.is_empty());
    let result = serde_json::from_str::<HashMap<String, ShipDesignTemplate>>(body.as_str());
    assert!(
        result.is_ok(),
        "Unable to deserialize designs with body: {} , Error {:?}",
        body,
        result.unwrap_err()
    );
    let designs = result.unwrap();
    assert!(!designs.is_empty(), "Received empty design list.");
    assert!(
        designs.contains_key("Buccaneer"),
        "Buccaneer not found in designs."
    );
    assert!(
        designs.get("Buccaneer").unwrap().name == "Buccaneer",
        "Buccaneer body malformed in design file."
    );

    send_quit(PORT, &cookie).await;
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[tokio::test]
async fn integration_simple_get() {
    const PORT: u16 = 3011;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = Client::new();
    let body = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body, r#"{"ships":[],"missiles":[],"planets":[]}"#);

    send_quit(PORT, &cookie).await;
}

/**
 * Test that we get a 404 response when we request a path that doesn't exist.
 */
#[tokio::test]
async fn integration_simple_unknown() {
    const PORT: u16 = 3012;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = Client::new();
    let response = client
        .get(path(PORT, INVALID_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();

    assert!(
        response.status().is_client_error(),
        "Instead of expected 404 got {:?}",
        response
    );

    send_quit(PORT, &cookie).await;
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test_log::test(tokio::test)]
async fn integration_add_ship() {
    const PORT: u16 = 3013;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates().await;

    let ship = r#"{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"design":"Buccaneer",
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]}}"#;

    let client = Client::new();

    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap_or_else(|e| panic!("Unable to get response from server: {:?}", e))
        .json()
        .await
        .unwrap();

    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
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
         "active_weapons": [true, true, true, true],
         "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]},
         "dodge_thrust":0,
         "assist_gunners":false,
        }],
        "missiles":[],
        "planets":[]});

    assert_json_eq!(entities, compare);

    send_quit(PORT, &cookie).await;
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test_log::test(tokio::test)]
async fn integration_add_planet_ship() {
    const PORT: u16 = 3014;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates().await;

    let ship = r#"{"name":"ship1","position":[0,2000,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer",
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]}}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let ship = r#"{"name":"ship2","position":[10000.0,10000.0,10000.0],"velocity":[10000.0,0.0,0.0], "acceleration":[0,0,0], "design":"Buccaneer",
        "crew":{"pilot":0,"engineering_jump":0,"engineering_power":0,"engineering_maneuver":0,"sensors":0,"gunnery":[]}}"#;
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
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
    let response: SimpleMsg = client
        .post(path(PORT, ADD_PLANET_PATH))
        .header("Cookie", &cookie)
        .body(planet)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add planet action executed".to_string()
        }
    );

    let entities = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
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
    let response: SimpleMsg = client
        .post(path(PORT, ADD_PLANET_PATH))
        .header("Cookie", &cookie)
        .body(planet)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add planet action executed".to_string()
        }
    );

    let entities = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
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

    send_quit(PORT, &cookie).await;
}

/*
 * Test that creates a ship and then updates its position.
 */
#[tokio::test]
async fn integration_update_ship() {
    const PORT: u16 = 3015;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .post(path(PORT, UPDATE_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .body(serde_json::to_string(&EMPTY_FIRE_ACTIONS_MSG).unwrap())
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"[]"#);

    let response = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    {
        let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        assert_eq!(
            ship.get_position(),
            Vec3::new(1000.0 * DELTA_TIME as f64, 0.0, 0.0)
        );
        assert_eq!(ship.get_velocity(), Vec3::new(1000.0, 0.0, 0.0));
    }
    send_quit(PORT, &cookie).await;
}

/*
 * Test to create two ships, launch a missile, and advance the round and see the missile move.
 *
 */
#[tokio::test]
async fn integration_update_missile() {
    const PORT: u16 = 3016;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let ship2 = r#"{"name":"ship2","position":[5000,0,5000],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"System Defense Boat"}"#;
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship2)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let fire_missile = json!([["ship1", [{"weapon_id": 1, "target": "ship2"}] ]]);
    let body = serde_json::to_string(&fire_missile).unwrap();
    //let missile = r#"{"source":"ship1","target":"ship2"}"#;
    let response = client
        .post(path(PORT, UPDATE_ENTITIES_PATH))
        .header("Cookie", &cookie)
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

    let entities = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
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

    send_quit(PORT, &cookie).await;
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[tokio::test]
async fn integration_remove_ship() {
    const PORT: u16 = 3017;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response: SimpleMsg = client
        .post(path(PORT, REMOVE_ENTITY_PATH))
        .header("Cookie", &cookie)
        .body(r#""ship1""#)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Remove action executed".to_string()
        }
    );

    let entities = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(entities, r#"{"ships":[],"missiles":[],"planets":[]}"#);

    send_quit(PORT, &cookie).await;
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[tokio::test]
async fn integration_set_acceleration() {
    const PORT: u16 = 3018;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();
    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    callisto::ship::config_test_ship_templates().await;
    let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();

    {
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        let flight_plan = &ship.plan;
        assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
        assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
        assert!(!flight_plan.has_second());
    }

    let response: SimpleMsg = client
        .post(path(PORT, SET_ACCELERATION_PATH))
        .header("Cookie", &cookie)
        .body(r#"{"name":"ship1","plan":[[[1,2,2],10000]]}"#)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Set acceleration action executed".to_string()
        }
    );

    let response = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    {
        let entities = serde_json::from_str::<Entities>(response.as_str()).unwrap();
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        let flight_plan = &ship.plan;
        assert_eq!(flight_plan.0 .0, [1.0, 2.0, 2.0].into());
        assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
        assert!(!flight_plan.has_second());
    }
    send_quit(PORT, &cookie).await;
}

/**
 * Test that will compute a simple path and return it, checking if the simple computation is correct.
 */
#[tokio::test]
async fn integration_compute_path_basic() {
    const PORT: u16 = 3019;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .post(path(PORT, COMPUTE_PATH_PATH))
        .header("Cookie", &cookie)
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

    send_quit(PORT, &cookie).await;
}

#[tokio::test]
async fn integration_compute_path_with_standoff() {
    const PORT: u16 = 3020;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0], "design":"Buccaneer"}"#;
    let client = Client::new();
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_json_eq!(
        response,
        SimpleMsg {
            msg: "Add ship action executed".to_string()
        }
    );

    let response = client
        .post(path(PORT, COMPUTE_PATH_PATH))
        .header("Cookie", &cookie)
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

    send_quit(PORT, &cookie).await;
}

/// Test malformed requests return appropriate error responses
#[tokio::test]
async fn integration_malformed_requests() {
    const PORT: u16 = 3021;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = reqwest::Client::new();

    // Test cases with invalid JSON for each endpoint
    let test_cases = vec![
        (UPDATE_ENTITIES_PATH, r#"{"not": "valid_update"}"#),
        (COMPUTE_PATH_PATH, r#"{"missing": "path_data"}"#),
        (ADD_SHIP_PATH, r#"{"invalid": "json"}"#),
        (ADD_PLANET_PATH, r#"{"name": "missing_required_fields"}"#),
        (REMOVE_ENTITY_PATH, r#"{"invalid": "remove_request"}"#),
        (SET_ACCELERATION_PATH, r#"{"not": "valid_plan"}"#),
    ];

    for (op, invalid_json) in test_cases {
        let response = client
            .post(path(PORT, op))
            .header("Cookie", &cookie)
            .body(invalid_json.to_string())
            .send()
            .await
            .unwrap_or_else(|e| panic!("Request to {} failed: {:?}", op, e));

        assert_eq!(
            response.status(),
            StatusCode::BAD_REQUEST,
            "Expected BAD_REQUEST for malformed {} request, got {:?}",
            op,
            response.status()
        );

        let error_text = response.text().await.unwrap();
        assert!(
            error_text.contains("Invalid JSON"),
            "Expected 'Invalid JSON' error for {}, got: {}",
            op,
            error_text
        );
    }

    // Test oversized request body (larger than 64KB)
    let large_body = "x".repeat(65 * 1024); // 65KB
    let response = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(large_body)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::PAYLOAD_TOO_LARGE,
        "Expected PAYLOAD_TOO_LARGE for oversized request"
    );

    // Test completely empty body
    let response = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body("")
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for empty body"
    );

    // Test malformed URL parameters
    let response = client
        .get(format!(
            "{}/{}",
            path(PORT, GET_ENTITIES_PATH),
            "invalid_param"
        ))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected NOT_FOUND for invalid URL parameters"
    );

    // Test invalid HTTP method
    let response = client
        .put(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body("{}")
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::NOT_FOUND,
        "Expected NOT_FOUND for invalid HTTP method"
    );

    // Test invalid content type
    let response = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body("not json")
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid content type"
    );

    // Test malformed ship design
    let invalid_ship = r#"{
        "name": "bad_ship",
        "position": [0,0,0],
        "velocity": [0,0,0],
        "acceleration": [0,0,0],
        "design": "NonexistentDesign",
        "crew": {
            "pilot": 0,
            "engineering_jump": 0,
            "engineering_power": 0,
            "engineering_maneuver": 0,
            "sensors": 0,
            "gunnery": []
        }
    }"#;

    let response = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(invalid_ship)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid ship design"
    );

    // Test malformed compute path request
    let invalid_path = r#"{
        "start": [0,0,0],
        "invalid_field": "value"
    }"#;

    let response = client
        .post(path(PORT, COMPUTE_PATH_PATH))
        .header("Cookie", &cookie)
        .body(invalid_path)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid compute path request"
    );

    send_quit(PORT, &cookie).await;
}

#[tokio::test]
async fn integration_bad_requests() {
    const PORT: u16 = 3024;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = reqwest::Client::new();

    // A bad set_crew request where we name a ship that doesn't exit.
    let response = client
        .post(path(PORT, SET_CREW_ACTIONS_PATH))
        .header("Cookie", &cookie)
        .body(r#"{"ship_name":"ship1","dodge_thrust":3,"assist_gunners":true}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid set_crew request"
    );

    // A planet with an invalid primary
    let response = client
        .post(path(PORT, ADD_PLANET_PATH))
        .header("Cookie", &cookie)
        .body(r#"{"name":"planet1","position":[0,0,0],"velocity":[0,0,0],"color":"red","primary":"InvalidPlanet","radius":1.5e6,"mass":3e24}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid planet primary"
    );

    // Remove a ship that doesn't exist
    let response = client
        .post(path(PORT, REMOVE_ENTITY_PATH))
        .header("Cookie", &cookie)
        .body(r#""ship1""#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid remove request"
    );

    // Set a flight plan for a non-existent ship
    let response = client
        .post(path(PORT, SET_ACCELERATION_PATH))
        .header("Cookie", &cookie)
        .body(r#"{"name":"ship1","plan":[[[1,2,2],10000]]}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid set acceleration request"
    );

    // Call fire_action with a weapon_id that is NaN
    let response = client
        .post(path(PORT, UPDATE_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .body(r#"[[["ship1", [{"weapon_id": NaN, "target": "ship2"}]]]]"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected BAD_REQUEST for invalid fire_action request"
    );

    send_quit(PORT, &cookie).await;
}

#[tokio::test]
async fn integration_options_request() {
    const PORT: u16 = 3022;
    let _server = spawn_test_server(PORT).await;

    let client = reqwest::Client::new();
    let web_backend = "https://test.example.com".to_string();

    let response = client
        .request(Method::OPTIONS, path(PORT, ""))
        .body("".as_bytes())
        .header("Origin", web_backend.clone())
        .send()
        .await
        .unwrap();

    // Verify response headers
    let headers = response.headers();

    // Check Allow-Credentials
    let allow_credentials = headers.get(ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap();
    assert_eq!(allow_credentials, "true");

    // Check Allow-Methods
    let allow_methods = headers.get(ACCESS_CONTROL_ALLOW_METHODS).unwrap();
    assert!(allow_methods.to_str().unwrap().contains("POST"));
    assert!(allow_methods.to_str().unwrap().contains("GET"));
    assert!(allow_methods.to_str().unwrap().contains("OPTIONS"));

    // Check Allow-Headers
    let allow_headers = headers.get(ACCESS_CONTROL_ALLOW_HEADERS).unwrap();
    let headers_str = allow_headers.to_str().unwrap();
    assert!(headers_str.contains("content-type"));
    assert!(headers_str.contains("authorization"));
    assert!(headers_str.contains("cookie"));
    assert!(headers_str.contains("access-control-allow-credentials"));

    // Verify empty response body
    let body = response.text().await.unwrap();
    assert_eq!(body, "");

    let (_, cookie) = test_authenticate(PORT).await.unwrap();
    send_quit(PORT, &cookie).await;
}

/**
 * Test loading scenarios through the web server
 */
#[tokio::test]
async fn integration_load_scenario() {
    const PORT: u16 = 3023;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let client = reqwest::Client::new();

    // Test successful scenario load
    let valid_scenario = r#"{"scenario_name": "./tests/test-scenario.json"}"#;
    let response = client
        .post(path(PORT, LOAD_SCENARIO_PATH))
        .header("Cookie", &cookie)
        .body(valid_scenario)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Failed to load valid scenario"
    );

    // Verify the scenario was loaded by checking entities
    let entities = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let entities_json: Entities = serde_json::from_str(&entities).unwrap();
    assert!(
        !entities_json.ships.is_empty() || !entities_json.planets.is_empty(),
        "Expected scenario to load some entities"
    );

    // Test loading non-existent scenario
    let invalid_scenario = r#"{"scenario_name": "./scenarios/nonexistent.json"}"#;
    let response = client
        .post(path(PORT, LOAD_SCENARIO_PATH))
        .header("Cookie", &cookie)
        .body(invalid_scenario)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error when loading non-existent scenario"
    );

    // Test malformed request
    let malformed_request = r#"{"wrong_field": "value"}"#;
    let response = client
        .post(path(PORT, LOAD_SCENARIO_PATH))
        .header("Cookie", &cookie)
        .body(malformed_request)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error for malformed request"
    );

    // Test empty scenario name
    let empty_scenario = r#"{"scenario_name": ""}"#;
    let response = client
        .post(path(PORT, LOAD_SCENARIO_PATH))
        .header("Cookie", &cookie)
        .body(empty_scenario)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error for empty scenario name"
    );

    send_quit(PORT, &cookie).await;
}

#[cfg_attr(feature = "ci", ignore)]
#[test_log::test(tokio::test)]
async fn integration_load_cloud_scenario() {
    const PORT: u16 = 3027;
    let _server = spawn_test_server(PORT).await;
    let client = reqwest::Client::new();
    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    let invalid_scenario = r#"{"scenario_name": "gs://nobucket/nonexistent.json"}"#;
    let response = client
        .post(path(PORT, LOAD_SCENARIO_PATH))
        .header("Cookie", &cookie)
        .body(invalid_scenario)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error when loading non-existent scenario"
    );

    send_quit(PORT, &cookie).await;
}

#[tokio::test]
async fn integration_fail_login() {
    const PORT: u16 = 3025;
    let _server = spawn_test_server(PORT).await;

    let client = reqwest::Client::new();

    // First an unauthenticated action
    let response = client
        .post(path(PORT, ADD_SHIP_PATH))
        .body(r#"{}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected UNAUTHORIZED for unauthenticated request"
    );

    // Now log in but without a valid code
    let response = client
        .post(path(PORT, LOGIN_PATH))
        .body(r#"{}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::UNAUTHORIZED,
        "Expected UNAUTHORIZED for unauthenticated request"
    );

    let (_, cookie) = test_authenticate(PORT).await.unwrap();
    send_quit(PORT, &cookie).await;
}

/**
 * Test setting crew actions through the web server
 */
#[tokio::test]
async fn integration_set_crew_actions() {
    const PORT: u16 = 3026;
    let _server = spawn_test_server(PORT).await;

    let (_, cookie) = test_authenticate(PORT).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    // First add a ship to test with
    let ship = r#"{
        "name": "test_ship",
        "position": [0.0, 0.0, 0.0],
        "velocity": [0.0, 0.0, 0.0],
        "acceleration": [0.0, 0.0, 0.0],
        "design": "Buccaneer",
        "crew": {
            "pilot": 2,
            "engineering_jump": 1,
            "engineering_power": 3,
            "engineering_maneuver": 2,
            "sensors": 1,
            "gunnery": [0,1,2]
        }
    }"#;

    let client = Client::new();

    // Add the ship
    let response: SimpleMsg = client
        .post(path(PORT, ADD_SHIP_PATH))
        .header("Cookie", &cookie)
        .body(ship)
        .send()
        .await
        .unwrap()
        .json()
        .await
        .unwrap();

    assert_eq!(response.msg, "Add ship action executed");

    // Verify the crew skills were set by checking entities
    let entities: String = client
        .get(path(PORT, GET_ENTITIES_PATH))
        .header("Cookie", &cookie)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    {
        let entities_json: Entities = serde_json::from_str(&entities).unwrap();
        let ship = entities_json
            .ships
            .get("test_ship")
            .unwrap()
            .read()
            .unwrap();
        let crew = ship.get_crew();
        debug!("(integration_set_crew_actions) Crew: ({:?})", crew);
        assert_eq!(crew.get_pilot(), 2);
        assert_eq!(crew.get_engineering_jump(), 1);
        assert_eq!(crew.get_engineering_power(), 3);
        assert_eq!(crew.get_engineering_maneuver(), 2);
        assert_eq!(crew.get_gunnery(0), 0);
        assert_eq!(crew.get_gunnery(1), 1);

        assert_eq!(crew.get_gunnery(2), 2);
    }
    // Test successful crew actions set
    let valid_actions = r#"{
                    "ship_name": "test_ship",
                    "actions": {
                        "dodge_thrust": 1,
                        "assist_gunners": true
                    }
                }"#;

    let response = client
        .post(path(PORT, SET_CREW_ACTIONS_PATH))
        .header("Cookie", &cookie)
        .body(valid_actions)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::OK,
        "Failed to set crew actions."
    );

    // Test setting crew actions for non-existent ship
    let invalid_ship_actions = r#"{
        "ship_name": "nonexistent_ship",
        "actions": {
            "dodge_thrust": 1,
            "assist_gunners": true
        }
    }"#;

    let response = client
        .post(path(PORT, SET_CREW_ACTIONS_PATH))
        .header("Cookie", &cookie)
        .body(invalid_ship_actions)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error when setting crew actions for non-existent ship"
    );

    // Test malformed crew actions
    let malformed_actions = r#"{
        "ship_name": "test_ship",
        "actions": {
            "dodge_thrust": true,
            "assist_gunners": 1,            
        }
    }"#;

    let response = client
        .post(path(PORT, SET_CREW_ACTIONS_PATH))
        .header("Cookie", &cookie)
        .body(malformed_actions)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error for malformed crew actions"
    );

    // Test invalid crew action values (out of range)
    let invalid_values = r#"{
        "ship_name": "test_ship",
        "actions": {
            "dodge_thrust": 999,
        }
    }"#;

    let response = client
        .post(path(PORT, SET_CREW_ACTIONS_PATH))
        .header("Cookie", &cookie)
        .body(invalid_values)
        .send()
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        StatusCode::BAD_REQUEST,
        "Expected error for invalid crew action values"
    );

    send_quit(PORT, &cookie).await;
}

#[tokio::test]
async fn integration_create_regular_server() {
    const PORT_1: u16 = 3028;
    const PORT_2: u16 = 3029;
    const PORT_3: u16 = 3030;

    // Spawn a regular server
    let exit_status = spawn_server(PORT_1, false, None, None, true)
        .await
        .unwrap()
        .try_wait()
        .unwrap();

    assert!(exit_status.is_none());

    // Spawn one that loads a scenario
    let exit_status = spawn_server(
        PORT_2,
        false,
        Some("./tests/test-scenario.json".to_string()),
        None,
        true,
    )
    .await
    .unwrap()
    .try_wait()
    .unwrap();

    assert!(exit_status.is_none());

    // Spawn one that loads a design file
    let exit_status = spawn_server(
        PORT_3,
        false,
        None,
        Some("./tests/test_templates.json".to_string()),
        true,
    )
    .await
    .unwrap()
    .try_wait()
    .unwrap();

    assert!(exit_status.is_none());
}

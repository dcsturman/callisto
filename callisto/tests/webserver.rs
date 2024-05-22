/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

extern crate callisto;
extern crate log;
extern crate pretty_env_logger;

use cgmath::{ assert_ulps_eq, Zero};

use callisto::entity::{ Entities, Vec3 };
use callisto::payloads::FlightPathMsg;

const SERVER_ADDRESS: &str = "127.0.0.1";
const SERVER_PATH: &str = "./target/debug/callisto";
const GET_ENTITIES_PATH: &str = "";
const UPDATE_ENTITIES_PATH: &str = "update";
const COMPUTE_PATH_PATH: &str = "compute_path";
const ADD_SHIP_PATH: &str = "add_ship";
const ADD_PLANET_PATH: &str = "add_planet";
const ADD_MISSILE_PATH: &str = "add_missile";
const REMOVE_ENTITY_PATH: &str = "remove";
const SET_ACCELERATION_PATH: &str = "set_accel";
const INVALID_PATH: &str = "unknown";

/**
 * Spawns a callisto server and returns a handle to it.  Used across tests to get a server up and running.
 * @param port The port to run the server on.
 * @return A handle to the running server.  This is critical as otherwise with kill_on_drop the server will be killed before the tests complete.
 */
async fn spawn_test_server(port: u16) -> Child {
    let handle = Command::new(SERVER_PATH)
        .arg("-p")
        .arg(port.to_string())
        .kill_on_drop(true)
        .spawn()
        .expect("Daemon failed to start.");

    let _ = pretty_env_logger::try_init();

    sleep(Duration::from_millis(2000)).await;

    handle
}

fn path(port: u16, verb: &str) -> String {
    format!("http://{}:{}/{}", SERVER_ADDRESS, port, verb)
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[tokio::test]
async fn test_simple_get() {
    const PORT: u16 = 3010;
    let _server = spawn_test_server(PORT).await;

    let body = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(body, "[]");
}

/**
 * Test that we get a 404 response when we request a path that doesn't exist.
 */
#[tokio::test]
async fn test_simple_unknown() {
    const PORT: u16 = 3011;
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
async fn test_add_ship() {
    const PORT: u16 = 3012;
    let _server = spawn_test_server(PORT).await;

    let ship = r#"{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0]}"#;
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

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(
        entities,
        r#"[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}]"#
    );
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[tokio::test]
async fn test_add_missile_planet_ship() {
    const PORT: u16 = 3013;
    let _server = spawn_test_server(PORT).await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0]}"#;
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
    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(r#"[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}]"#).unwrap());

    let planet =
        r#"{"name":"planet1","position":[0,0,0],"color":"red","primary":[0,0,0],"mass":100}"#;
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
    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(
        r#"[
            {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],
              "kind":{"Planet":{"color":"red","primary":[0.0,0.0,0.0],"mass":100.0}}},
            {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}
            ]"#).unwrap());

    let missile = r#"{"name":"missile1","position":[0,0,0],"target":"ship1","burns":3,"acceleration":[0,0,0]}"#;
    let response = reqwest::Client::new()
        .post(path(PORT, ADD_MISSILE_PATH))
        .body(missile)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Add missile action executed" }"#);

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(r#"[
        {"name":"missile1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],
            "kind":{"Missile":{"target":"ship1","burns":3}}},
        {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],
            "kind":{"Planet":{"color":"red","primary":[0.0,0.0,0.0],"mass":100.0}}},
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}
        ]"#).unwrap());
}

/*
 * Test that creates a ship and then updates its position.
 */
#[tokio::test]
async fn test_update_ship() {
    const PORT: u16 = 3014;
    let _server = spawn_test_server(PORT).await;

    let ship =
        r#"{"name":"ship1","position":[0,0,0],"velocity":[1000,0,0], "acceleration":[0,0,0]}"#;
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
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(response, r#"{ "msg" : "Update action executed" }"#);

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(r#"[{"name":"ship1","position":[1000000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}]"#).unwrap());
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[tokio::test]
async fn test_remove_ship() {
    const PORT: u16 = 3015;
    let _server = spawn_test_server(PORT).await;

    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0]}"#;
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

    assert_eq!(entities, "[]");
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[tokio::test]
async fn test_set_acceleration() {
    const PORT: u16 = 3016;
    let _server = spawn_test_server(PORT).await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0]}"#;
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

    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(r#"[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[0.0,0.0,0.0],"kind":"Ship"}]"#)
        .unwrap());

    let response = reqwest::Client::new()
        .post(path(PORT, SET_ACCELERATION_PATH))
        .body(r#"{"name":"ship1","acceleration":[1,2,3]}"#)
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
    let entities = reqwest::get(path(PORT, GET_ENTITIES_PATH))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert_eq!(serde_json::from_str::<Entities>(entities.as_str()).unwrap(),
        serde_json::from_str(r#"[{"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],"acceleration":[1.0,2.0,3.0],"kind":"Ship"}]"#).unwrap());
}
/**
 * Test that will compute a simple path and return it, checking if the simple computation is correct.
 */
#[tokio::test]
async fn test_compute_path() {
    const PORT: u16 = 3017;
    let _server = spawn_test_server(PORT).await;
    let ship = r#"{"name":"ship1","position":[0,0,0],"velocity":[0,0,0], "acceleration":[0,0,0]}"#;
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
        .body(r#"{"entity_name":"ship1","end_pos":[58860000,0,0],"end_vel":[0,0,0]}"#)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    let plan = serde_json::from_str::<FlightPathMsg>(response.as_str()).unwrap();

    assert_eq!(plan.path.len(), 3);
    assert_eq!(plan.path[0], Vec3::zero());
    assert_ulps_eq!(plan.path[1], Vec3 { x: 29430000.0, y: 0.0, z: 0.0 });
    assert_ulps_eq!(plan.path[2], Vec3 { x: 58860000.0, y: 0.0, z: 0.0 });
    assert_ulps_eq!(plan.end_velocity, Vec3::zero());
    assert_eq!(plan.accelerations.len(), 2);
    assert_ulps_eq!(plan.accelerations[0].0, Vec3 { x: 6.0, y: 0.0, z: 0.0 });
    assert_eq!(plan.accelerations[0].1, 1000);
    assert_ulps_eq!(plan.accelerations[1].0, Vec3 { x: -6.0, y: 0.0, z: 0.0 });    
    assert_eq!(plan.accelerations[0].1, 1000);
}

/*!
 * Test the web server functionality provided in main.rs as a set of integration tests.
 * Each test spins up a running callisto server and issues http requests to it.
 * The goal here is not to exercise all the logic in the server, but rather to ensure that the server
 * is up and running and responds to requests.  We want to test all the message formats back and forth.
 * Testing the logic should be done in the unit tests for main.rs.
 */
extern crate callisto;
use std::env::var;
use std::io;

use assert_json_diff::assert_json_eq;
use futures_util::{SinkExt, StreamExt};
use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};
use tokio_native_tls::TlsStream;
use tokio_tungstenite::{
    client_async_tls, connect_async,
    tungstenite::{Error, Result},
    MaybeTlsStream, WebSocketStream,
};

use serde_json::json;

use callisto::debug;

use callisto::entity::{Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME_F64};
use callisto::payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, EffectMsg, FireAction, IncomingMsg, LoadScenarioMsg,
    LoginMsg, OutgoingMsg, SetCrewActions, SetPlanMsg, EMPTY_FIRE_ACTIONS_MSG,
};

use callisto::crew::{Crew, Skills};

use cgmath::{assert_ulps_eq, Zero};

type MyWebSocket = WebSocketStream<MaybeTlsStream<TlsStream<TcpStream>>>;

const SERVER_ADDRESS: &str = "127.0.0.1";
const SERVER_PATH: &str = "target/debug/callisto";

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
            var("RUST_LOG").unwrap_or_else(|_| String::new()),
        )
        .env(
            "RUSTFLAGS",
            var("RUSTFLAGS").unwrap_or_else(|_| String::new()),
        )
        .env(
            "CARGO_LLVM_COV",
            var("CARGO_LLVM_COV").unwrap_or_else(|_| String::new()),
        )
        .env(
            "CARGO_LLVM_COV_SHOW_ENV",
            var("CARGO_LLVM_COV_SHOW_ENV").unwrap_or_else(|_| String::new()),
        )
        .env(
            "CARGO_LLVM_COV_TARGET_DIR",
            var("CARGO_LLVM_COV_TARGET_DIR").unwrap_or_else(|_| String::new()),
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
 * @return A handle to the running server.  This is critical as otherwise with `kill_on_drop` the server will be killed before the tests complete.
 */
async fn spawn_test_server(port: u16) -> Result<Child, io::Error> {
    spawn_server(port, true, None, None, false).await
}

/**
 * Send a quit message to cleanly end a test.
 */
async fn send_quit(stream: &mut MyWebSocket) {
    stream
        .send(serde_json::to_string(&IncomingMsg::Quit).unwrap().into())
        .await
        .unwrap();
}

async fn open_socket(
    port: u16,
) -> Result<WebSocketStream<MaybeTlsStream<TlsStream<TcpStream>>>, Error> {
    let connector: tokio_native_tls::TlsConnector =
        tokio_native_tls::native_tls::TlsConnector::builder()
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap()
            .into();
    let stream = TcpStream::connect(format!("{SERVER_ADDRESS}:{port}"))
        .await
        .unwrap();
    let tls_stream = connector.connect(SERVER_ADDRESS, stream).await.unwrap();
    debug!("(webservers.open_socket) TLS stream established.");

    let socket_url = format!("ws://{SERVER_ADDRESS}:{port}/");
    debug!("(webservers.open_socket) Attempt to connect to WebSocket URL: {socket_url}");

    let (ws_stream, _) = client_async_tls(socket_url, tls_stream)
        .await
        .unwrap_or_else(|e| panic!("Client_async_tls failed with {e:?}"));

    debug!("(webservers.open_socket) WebSocket stream established.");
    Ok(ws_stream)
}

async fn rpc(stream: &mut MyWebSocket, request: IncomingMsg) -> OutgoingMsg {
    stream
        .send(serde_json::to_string(&request).unwrap().into())
        .await
        .unwrap();

    let reply = stream
        .next()
        .await
        .unwrap_or_else(|| panic!("No response from server for request: {request:?}."))
        .unwrap_or_else(|err| {
            panic!("Receiving error from server {err:?} in response to request: {request:?}.")
        });
    let body = serde_json::from_str::<OutgoingMsg>(reply.to_text().unwrap()).unwrap();
    body
}

/**
 * Do authentication with the test server
 * Return the user name and the key from `SetCookie`
 */
async fn test_authenticate(stream: &mut MyWebSocket) -> Result<String, String> {
    /* let client = Client::new();
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
    */
    let msg = IncomingMsg::Login(LoginMsg {
        code: "test_code".to_string(),
    });

    let body = rpc(stream, msg).await;
    if let OutgoingMsg::AuthResponse(auth_response) = body {
        Ok(auth_response.email)
    } else {
        Err(format!("Expected auth response to login. Got {body:?}"))
    }
}

/**
 * Test for get_designs in server.
 */
#[test_log::test(tokio::test)]
async fn integration_get_designs() {
    const PORT: u16 = 3010;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();

    let _ = test_authenticate(&mut stream).await.unwrap();

    let body = rpc(&mut stream, IncomingMsg::DesignTemplateRequest).await;

    assert!(
        matches!(body, OutgoingMsg::DesignTemplateResponse(_)),
        "Improper response to design request received."
    );

    if let OutgoingMsg::DesignTemplateResponse(designs) = body {
        assert!(!designs.is_empty(), "Received empty design list.");
        assert!(
            designs.contains_key("Buccaneer"),
            "Buccaneer not found in designs."
        );
        assert!(
            designs.get("Buccaneer").unwrap().name == "Buccaneer",
            "Buccaneer body malformed in design file."
        );
    }
    send_quit(&mut stream).await;
}

/**
 * Test that we can get a response to a get request when the entities state is empty (so the response is very simple)
 */
#[tokio::test]
async fn integration_simple_get() {
    const PORT: u16 = 3011;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();

    let _ = test_authenticate(&mut stream).await.unwrap();

    let body = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    assert!(
        matches!(body, OutgoingMsg::EntityResponse(_)),
        "Improper response to get request received."
    );

    if let OutgoingMsg::EntityResponse(entities) = body {
        assert!(entities.is_empty(), "Expected empty entities list.");
    }
    send_quit(&mut stream).await;
}

#[tokio::test]
async fn integration_action_without_login() {
    const PORT: u16 = 3012;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();

    // Intentionally skip test authenticate here.
    debug!("****** ONE");
    let body = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    debug!("****** TWO");
    assert!(
        matches!(body, OutgoingMsg::PleaseLogin),
        "Expected request to log in, but instead got {body:?}"
    );

    send_quit(&mut stream).await;
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test_log::test(tokio::test)]
async fn integration_add_ship() {
    const PORT: u16 = 3013;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates().await;

    let ship = AddShipMsg {
        name: "ship1".to_string(),
        position: [0.0, 0.0, 0.0].into(),
        velocity: [0.0, 0.0, 0.0].into(),
        acceleration: [0.0, 0.0, 0.0].into(),
        design: "Buccaneer".to_string(),
        crew: None,
    };

    let body = rpc(&mut stream, IncomingMsg::AddShip(ship)).await;

    assert!(
        matches!(body, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"),
        "Improper response to add ship request received."
    );

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;

    assert!(
        matches!(entities, OutgoingMsg::EntityResponse(_)),
        "Improper response to entities request received."
    );

    if let OutgoingMsg::EntityResponse(entities) = entities {
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
    }
    send_quit(&mut stream).await;
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test_log::test(tokio::test)]
async fn integration_add_planet_ship() {
    const PORT: u16 = 3014;
    let _server = spawn_test_server(PORT).await;

    // Need this only because we are going to deserialize ships.
    callisto::ship::config_test_ship_templates().await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 2000.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;

    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship2".to_string(),
            position: [10000.0, 10000.0, 10000.0].into(),
            velocity: [10000.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let response = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = response {
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
    } else {
        panic!("Improper response to entities request received.");
    }

    let message = rpc(
        &mut stream,
        IncomingMsg::AddPlanet(AddPlanetMsg {
            name: "planet1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            color: "red".to_string(),
            radius: 1.5e6,
            mass: 3e24,
            primary: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add planet action executed"));

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;

    if let OutgoingMsg::EntityResponse(entities) = entities {
        let compare = json!({"planets":[
        {"name":"planet1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
          "color":"red","radius":1.5e6,"mass":3e24,
          "gravity_radius_1":4_518_410.048_543_495,
          "gravity_radius_05":6_389_996.771_013_086,
          "gravity_radius_025": 9_036_820.097_086_99,
          "gravity_radius_2": 3_194_998.385_506_543}],
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

        assert_json_eq!(entities, compare);
    } else {
        panic!("Improper response to entities request received.");
    }

    let message = rpc(
        &mut stream,
        IncomingMsg::AddPlanet(AddPlanetMsg {
            name: "planet2".to_string(),
            position: [1_000_000.0, 0.0, 0.0].into(),
            color: "red".to_string(),
            radius: 1.5e6,
            mass: 1e23,
            primary: Some("planet1".to_string()),
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add planet action executed"));

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        let compare = json!({"missiles":[],
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

        assert_json_eq!(&entities, &compare);
    } else {
        panic!("Improper response to entities request received.");
    }

    send_quit(&mut stream).await;
}

/*
 * Test that creates a ship and then updates its position.
 */
#[tokio::test]
async fn integration_update_ship() {
    const PORT: u16 = 3015;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [1000.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;

    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let response = rpc(&mut stream, IncomingMsg::Update(EMPTY_FIRE_ACTIONS_MSG)).await;
    assert!(matches!(response, OutgoingMsg::Effects(eq) if eq.is_empty()));

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        assert_eq!(
            ship.get_position(),
            Vec3::new(1000.0 * DELTA_TIME_F64, 0.0, 0.0)
        );
        assert_eq!(ship.get_velocity(), Vec3::new(1000.0, 0.0, 0.0));
    } else {
        panic!("Improper response to entities request received.");
    }
    send_quit(&mut stream).await;
}

/*
 * Test to create two ships, launch a missile, and advance the round and see the missile move.
 *
 */
#[allow(clippy::too_many_lines)]
#[tokio::test]
async fn integration_update_missile() {
    const PORT: u16 = 3016;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [1000.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "System Defense Boat".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship2".to_string(),
            position: [5000.0, 0.0, 5000.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "System Defense Boat".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let fire_actions = vec![(
        "ship1".to_string(),
        vec![FireAction {
            weapon_id: 1,
            target: "ship2".to_string(),
            called_shot_system: None,
        }],
    )];

    let effects = rpc(&mut stream, IncomingMsg::Update(fire_actions)).await;
    if let OutgoingMsg::Effects(effects) = effects {
        let filtered_effects: Vec<_> = effects
            .iter()
            .filter(|e| !matches!(e, EffectMsg::Message { .. }))
            .collect();

        let compare = vec![EffectMsg::ShipImpact {
            position: Vec3::new(5000.0, 0.0, 5000.0),
        }];
        assert_json_eq!(filtered_effects, compare);
    } else {
        panic!("Improper response to update request received.");
    }

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        let compare = json!({"ships":[
            {"name":"ship1","position":[360_000.0,0.0,0.0],"velocity":[1000.0,0.0,0.0],
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

        assert_json_eq!(entities, compare);
    } else {
        panic!("Improper response to entities request received.");
    }

    send_quit(&mut stream).await;
}

/*
 * Test that we can add a ship, then remove it, and test that the entities list is empty.
 */
#[tokio::test]
async fn integration_remove_ship() {
    const PORT: u16 = 3017;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let message = rpc(&mut stream, IncomingMsg::Remove("ship1".to_string())).await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Remove action executed"));

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        assert!(entities.ships.is_empty());
        assert!(entities.missiles.is_empty());
        assert!(entities.planets.is_empty());
    } else {
        panic!("Improper response to entities request received.");
    }

    send_quit(&mut stream).await;
}

/**
 * Test that creates a ship entity, assigns an acceleration, and then gets all entities to check that the acceleration is properly set.
 */
#[tokio::test]
async fn integration_set_acceleration() {
    const PORT: u16 = 3018;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        let flight_plan = &ship.plan;
        assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
        assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
        assert!(!flight_plan.has_second());
    }

    let message = rpc(
        &mut stream,
        IncomingMsg::SetPlan(SetPlanMsg {
            name: "ship1".to_string(),
            plan: vec![([1.0, 2.0, 2.0].into(), 10000)].into(),
        }),
    )
    .await;
    assert!(
        matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Set acceleration action executed")
    );

    let entities = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;
    if let OutgoingMsg::EntityResponse(entities) = entities {
        let ship = entities.ships.get("ship1").unwrap().read().unwrap();
        let flight_plan = &ship.plan;
        assert_eq!(flight_plan.0 .0, [1.0, 2.0, 2.0].into());
        assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
        assert!(!flight_plan.has_second());
    }

    send_quit(&mut stream).await;
}

/**
 * Test that will compute a simple path and return it, checking if the simple computation is correct.
 */
#[tokio::test]
async fn integration_compute_path_basic() {
    const PORT: u16 = 3019;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _cookie = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    let message = rpc(
        &mut stream,
        IncomingMsg::ComputePath(ComputePathMsg {
            entity_name: "ship1".to_string(),
            end_pos: [58_842_000.0, 0.0, 0.0].into(),
            end_vel: [0.0, 0.0, 0.0].into(),
            standoff_distance: 0.0,
            target_velocity: None,
        }),
    )
    .await;

    if let OutgoingMsg::FlightPath(plan) = message {
        assert_eq!(plan.path.len(), 9);
        assert_eq!(plan.path[0], Vec3::zero());
        assert_ulps_eq!(
            plan.path[1],
            Vec3 {
                x: 1_906_480.8,
                y: 0.0,
                z: 0.0
            }
        );
        assert_ulps_eq!(
            plan.path[2],
            Vec3 {
                x: 7_625_923.2,
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
            panic!("Expecting second acceleration.");
        }
        assert_eq!(t, 1414);
    } else {
        panic!("Improper response to compute path request received.");
    }

    send_quit(&mut stream).await;
}

/**
 * Test that will compute a path with standoff value
 */
#[test_log::test(tokio::test)]
async fn integration_compute_path_with_standoff() {
    const PORT: u16 = 3021;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    // Add test ship
    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "ship1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    // Compute path with standoff
    let message = rpc(
        &mut stream,
        IncomingMsg::ComputePath(ComputePathMsg {
            entity_name: "ship1".to_string(),
            end_pos: [58_842_000.0, 0.0, 0.0].into(),
            end_vel: [0.0, 0.0, 0.0].into(),
            standoff_distance: 60000.0,
            target_velocity: None,
        }),
    )
    .await;

    if let OutgoingMsg::FlightPath(plan) = message {
        assert_eq!(plan.path.len(), 9);
        assert_eq!(plan.path[0], Vec3::zero());
        assert_ulps_eq!(
            plan.path[1],
            Vec3 {
                x: 1_906_480.8,
                y: 0.0,
                z: 0.0
            }
        );
        assert_ulps_eq!(
            plan.path[2],
            Vec3 {
                x: 7_625_923.2,
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
            panic!("Expecting second acceleration.");
        }
        assert_eq!(t, 1413);
    } else {
        panic!("Expected FlightPath response");
    }

    send_quit(&mut stream).await;
}

/**
 * Test various malformed requests to ensure proper error handling
 */
#[test_log::test(tokio::test)]
async fn integration_malformed_requests() {
    const PORT: u16 = 3022;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    // Test invalid ship design
    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "bad_ship".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "NonexistentDesign".to_string(),
            crew: None,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::Error(_)));

    // Test invalid planet primary
    let message = rpc(
        &mut stream,
        IncomingMsg::AddPlanet(AddPlanetMsg {
            name: "planet1".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            color: "red".to_string(),
            primary: Some("InvalidPrimary".to_string()),
            radius: 1.5e6,
            mass: 3e24,
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::Error(_)));

    // Test invalid compute path request
    let message = rpc(
        &mut stream,
        IncomingMsg::ComputePath(ComputePathMsg {
            entity_name: "nonexistent_ship".to_string(),
            end_pos: [0.0, 0.0, 0.0].into(),
            end_vel: [0.0, 0.0, 0.0].into(),
            standoff_distance: 0.0,
            target_velocity: None,
        }),
    )
    .await;
    assert!(
        matches!(message, OutgoingMsg::Error(_)),
        "Expected error for invalid compute path request, got {message:?}"
    );

    // Test invalid flight plan
    let message = rpc(
        &mut stream,
        IncomingMsg::SetPlan(SetPlanMsg {
            name: "nonexistent_ship".to_string(),
            plan: vec![([f64::NAN, 0.0, 0.0].into(), 1000)].into(),
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::Error(_)));

    // Test fire action with invalid parameters
    let message = rpc(
        &mut stream,
        IncomingMsg::Update(vec![(
            "nonexistent_ship".to_string(),
            vec![FireAction {
                weapon_id: 0,
                target: "nonexistent_target".to_string(),
                called_shot_system: None,
            }],
        )]),
    )
    .await;
    assert!(
        matches!(&message, OutgoingMsg::Effects(x) if x.is_empty()),
        "Expected empty effects for invalid fire action, got {message:?}"
    );

    send_quit(&mut stream).await;
}

#[test_log::test(tokio::test)]
async fn integration_bad_requests() {
    const PORT: u16 = 3024;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    // Test setting crew actions for non-existent ship
    let msg = IncomingMsg::SetCrewActions(SetCrewActions {
        ship_name: "ship1".to_string(),
        dodge_thrust: Some(3),
        assist_gunners: Some(true),
    });
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::Error(_)));

    // Test adding planet with invalid primary
    let msg = IncomingMsg::AddPlanet(AddPlanetMsg {
        name: "planet1".to_string(),
        position: [0.0, 0.0, 0.0].into(),
        color: "red".to_string(),
        primary: Some("InvalidPlanet".to_string()),
        radius: 1.5e6,
        mass: 3e24,
    });
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::Error(_)));

    // Test removing non-existent ship
    let msg = IncomingMsg::Remove("ship1".to_string());
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::Error(_)));

    // Test setting flight plan for non-existent ship
    let msg = IncomingMsg::SetPlan(SetPlanMsg {
        name: "ship1".to_string(),
        plan: vec![([1.0, 2.0, 2.0].into(), 10000)].into(),
    });
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::Error(_)));

    // Test fire action with invalid weapon_id
    let msg = IncomingMsg::Update(vec![(
        "ship1".to_string(),
        vec![FireAction {
            weapon_id: u32::MAX,
            target: "ship2".to_string(),
            called_shot_system: None,
        }],
    )]);
    let response = rpc(&mut stream, msg).await;
    assert!(
        matches!(&response, OutgoingMsg::Effects(x) if x.is_empty()),
        "Expected empty effects for invalid weapon_id. Instead got {response:?}"
    );

    send_quit(&mut stream).await;
}

#[test_log::test(tokio::test)]
async fn integration_load_scenario() {
    const PORT: u16 = 3023;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    // Test successful scenario load
    let msg = IncomingMsg::LoadScenario(LoadScenarioMsg {
        scenario_name: "./tests/test-scenario.json".to_string(),
    });
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::SimpleMsg(_)));

    // Verify the scenario was loaded by checking entities
    let msg = IncomingMsg::EntitiesRequest;
    let response = rpc(&mut stream, msg).await;
    if let OutgoingMsg::EntityResponse(entities) = response {
        assert!(
            !entities.ships.is_empty() || !entities.planets.is_empty(),
            "Expected scenario to load some entities"
        );
    } else {
        panic!("Expected EntityResponse");
    }

    // Test loading non-existent scenario
    let msg = IncomingMsg::LoadScenario(LoadScenarioMsg {
        scenario_name: "./scenarios/nonexistent.json".to_string(),
    });
    let response = rpc(&mut stream, msg).await;
    assert!(matches!(response, OutgoingMsg::Error(_)));

    send_quit(&mut stream).await;
}

#[cfg_attr(feature = "ci", ignore)]
#[test_log::test(tokio::test)]
async fn integration_load_cloud_scenario() {
    const PORT: u16 = 3027;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    // Test loading non-existent cloud scenario
    let message = rpc(
        &mut stream,
        IncomingMsg::LoadScenario(LoadScenarioMsg {
            scenario_name: "gs://nobucket/nonexistent.json".to_string(),
        }),
    )
    .await;
    assert!(matches!(message, OutgoingMsg::Error(_)));

    send_quit(&mut stream).await;
}

#[tokio::test]
async fn integration_fail_login() {
    const PORT: u16 = 3025;
    let _server = spawn_test_server(PORT).await;

    // Test unauthenticated connection
    let socket_url = format!("ws://127.0.0.1:{PORT}/ws");
    let stream = connect_async(socket_url).await;
    assert!(
        stream.is_err(),
        "Expected connection to fail without authentication"
    );

    // Test invalid authentication
    let mut stream = open_socket(PORT).await.unwrap();
    let message = rpc(
        &mut stream,
        IncomingMsg::Login(LoginMsg {
            code: "invalid_code".to_string(),
        }),
    )
    .await;
    assert!(
        matches!(message, OutgoingMsg::Error(_)),
        "Expected error for invalid authentication. Got: {message:?}"
    );

    send_quit(&mut stream).await;
}

/**
 * Test setting crew actions through WebSocket
 */
#[test_log::test(tokio::test)]
async fn integration_set_crew_actions() {
    const PORT: u16 = 3026;
    let _server = spawn_test_server(PORT).await;

    let mut stream = open_socket(PORT).await.unwrap();
    let _ = test_authenticate(&mut stream).await.unwrap();

    callisto::ship::config_test_ship_templates().await;

    // First add a ship to test with
    let mut crew = Crew::default();
    crew.set_skill(Skills::Pilot, 2);
    crew.set_skill(Skills::EngineeringJump, 1);
    crew.set_skill(Skills::EngineeringPower, 3);
    crew.set_skill(Skills::EngineeringManeuver, 2);
    crew.set_skill(Skills::Sensors, 1);
    crew.add_gunnery(0);
    crew.add_gunnery(1);
    crew.add_gunnery(2);

    let message = rpc(
        &mut stream,
        IncomingMsg::AddShip(AddShipMsg {
            name: "test_ship".to_string(),
            position: [0.0, 0.0, 0.0].into(),
            velocity: [0.0, 0.0, 0.0].into(),
            acceleration: [0.0, 0.0, 0.0].into(),
            design: "Buccaneer".to_string(),
            crew: Some(crew),
        }),
    )
    .await;

    assert!(matches!(message, OutgoingMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

    // Verify the crew skills were set by checking entities
    let response = rpc(&mut stream, IncomingMsg::EntitiesRequest).await;

    if let OutgoingMsg::EntityResponse(entities) = response {
        let ship = entities.ships.get("test_ship").unwrap().read().unwrap();
        let crew = ship.get_crew();
        debug!("(integration_set_crew_actions) Crew: ({:?})", crew);
        assert_eq!(crew.get_pilot(), 2);
        assert_eq!(crew.get_engineering_jump(), 1);
        assert_eq!(crew.get_engineering_power(), 3);
        assert_eq!(crew.get_engineering_maneuver(), 2);
        assert_eq!(crew.get_gunnery(0), 0);
        assert_eq!(crew.get_gunnery(1), 1);
        assert_eq!(crew.get_gunnery(2), 2);
    } else {
        panic!("Expected EntityResponse");
    }

    // Test successful crew actions set
    let message = rpc(
        &mut stream,
        IncomingMsg::SetCrewActions(SetCrewActions {
            ship_name: "test_ship".to_string(),
            dodge_thrust: Some(1),
            assist_gunners: Some(true),
        }),
    )
    .await;

    assert!(matches!(message, OutgoingMsg::SimpleMsg(_)));

    // Test setting crew actions for non-existent ship
    let message = rpc(
        &mut stream,
        IncomingMsg::SetCrewActions(SetCrewActions {
            ship_name: "nonexistent_ship".to_string(),
            dodge_thrust: Some(1),
            assist_gunners: Some(true),
        }),
    )
    .await;

    assert!(matches!(message, OutgoingMsg::Error(_)));

    send_quit(&mut stream).await;
}

#[cfg_attr(feature = "ci", ignore)]
#[tokio::test]
async fn integration_create_regular_server() {
    const PORT_1: u16 = 3028;
    const PORT_2: u16 = 3029;
    const PORT_3: u16 = 3030;

    // Test regular server
    let mut server1 = spawn_server(PORT_1, false, None, None, true).await.unwrap();
    assert!(server1.try_wait().unwrap().is_none());

    // Test server with scenario
    let mut server2 = spawn_server(
        PORT_2,
        false,
        Some("./tests/test-scenario.json".to_string()),
        None,
        true,
    )
    .await
    .unwrap();
    assert!(server2.try_wait().unwrap().is_none());

    // Test server with design file
    let mut server3 = spawn_server(
        PORT_3,
        false,
        None,
        Some("./tests/test_templates.json".to_string()),
        true,
    )
    .await
    .unwrap();
    assert!(server3.try_wait().unwrap().is_none());
}

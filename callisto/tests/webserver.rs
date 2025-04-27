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
use std::sync::atomic::AtomicU16;

use assert_json_diff::assert_json_eq;
use futures_util::{SinkExt, StreamExt};

use tokio::net::TcpStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};
use tokio_tungstenite::{
  connect_async,
  tungstenite::{Error, Result},
  MaybeTlsStream, WebSocketStream,
};

#[cfg(not(feature = "no_tls_upgrade"))]
use rustls::pki_types::{pem::PemObject, CertificateDer};
#[cfg(not(feature = "no_tls_upgrade"))]
use std::sync::Arc;
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_tungstenite::connect_async_tls_with_config;
#[cfg(not(feature = "no_tls_upgrade"))]
use tokio_tungstenite::Connector;

use serde_json::json;

use callisto::{debug, error};

use callisto::action::ShipAction;
use callisto::entity::{Entity, Vec3, DEFAULT_ACCEL_DURATION, DELTA_TIME_F64, G};
use callisto::payloads::{
  AddPlanetMsg, AddShipMsg, ComputePathMsg, CreateScenarioMsg, EffectMsg, JoinScenarioMsg, LoginMsg, RequestMsg,
  ResponseMsg, SetPilotActions, SetPlanMsg, EMPTY_FIRE_ACTIONS_MSG,
};

use callisto::crew::{Crew, Skills};

use cgmath::{assert_ulps_eq, Zero};

type MyWebSocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const SERVER_ADDRESS: &str = "127.0.0.1";
const SERVER_PATH: &str = "target/debug/callisto";

static NEXT_PORT: AtomicU16 = AtomicU16::new(3010);

fn get_next_port() -> u16 {
  NEXT_PORT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

async fn spawn_server(
  port: u16, test_mode: bool, scenario_dir: Option<String>, design_file: Option<String>, auto_kill: bool,
) -> Result<Child, io::Error> {
  let mut handle = Command::new(SERVER_PATH);
  let mut handle = handle
    .env("RUST_LOG", var("RUST_LOG").unwrap_or_else(|_| String::new()))
    .env("RUSTFLAGS", var("RUSTFLAGS").unwrap_or_else(|_| String::new()))
    .env("CARGO_LLVM_COV", var("CARGO_LLVM_COV").unwrap_or_else(|_| String::new()))
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
  if let Some(scenario) = scenario_dir {
    handle = handle.arg("--scenario-dir").arg(scenario);
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

async fn open_socket(port: u16) -> Result<MyWebSocket, Error> {
  #[cfg(feature = "no_tls_upgrade")]
  {
    let socket_url = format!("ws://{SERVER_ADDRESS}:{port}/ws");
    debug!("(webservers.open_socket) Attempt to connect to WebSocket URL: {socket_url}");
    let (ws_stream, _) = connect_async(socket_url).await?;
    debug!("(webservers.open_socket) WebSocket stream established.");
    Ok(ws_stream)
  }
  #[cfg(not(feature = "no_tls_upgrade"))]
  {
    let rust_cert = CertificateDer::pem_file_iter("keys/rootCA.crt")
      .expect("Cannot open CA file")
      .map(|result| result.unwrap());
    let mut root_store = rustls::RootCertStore::empty();
    root_store.add_parsable_certificates(rust_cert);

    let config = rustls::client::ClientConfig::builder()
      .with_root_certificates(root_store)
      .with_no_client_auth();

    let connector = Connector::Rustls(Arc::new(config));

    let socket_url = format!("wss://{SERVER_ADDRESS}:{port}/");
    debug!("(webservers.open_socket) Attempt to connect to WebSocket URL: {socket_url}");

    let (ws_stream, _) = connect_async_tls_with_config(socket_url, None, false, Some(connector))
      .await
      .unwrap_or_else(|e| panic!("Client_async_tls failed with {e:?}"));

    debug!("(webservers.open_socket) WebSocket stream established.");
    Ok(ws_stream)
  }
}

async fn rpc(stream: &mut MyWebSocket, request: RequestMsg) -> ResponseMsg {
  debug!("(webservers.rpc) Sending request: {request:?}");
  stream.send(serde_json::to_string(&request).unwrap().into()).await.unwrap();

  let reply = stream
    .next()
    .await
    .unwrap_or_else(|| panic!("No response from server for request: {request:?}."))
    .unwrap_or_else(|err| panic!("Receiving error from server {err:?} in response to request: {request:?}."));
  debug!("(webservers.rpc) Received response: {reply:?}");
  let body = serde_json::from_str::<ResponseMsg>(reply.to_text().unwrap()).unwrap();
  body
}

/**
 * Since we tend to get a lot of entity messages in reply to actions, this helper method
 * will drain away an extra entity response.
 */
async fn drain_entity_response(stream: &mut MyWebSocket) -> ResponseMsg {
  let Ok(reply) = stream.next().await.unwrap() else {
    panic!("Expected entity response.  Got error.");
  };
  let body = serde_json::from_str::<ResponseMsg>(reply.to_text().expect("Expected text response"))
    .expect("Failed to parse entity response");
  assert!(
    matches!(body, ResponseMsg::EntityResponse(_)),
    "Expected entity response: {body:?}"
  );

  body
}

/**
 * Drain all the messages the server sends on connect.  This right now is
 * 3 messages: templates, entities, and users.  See [`callisto:build_successful_auth_msgs`].
 */
async fn drain_initialization_messages(stream: &mut MyWebSocket) {
  let Ok(scenario_msg) = stream.next().await.unwrap() else {
    panic!("Expected scenario response.  Got error.");
  };

  assert!(
    matches!(
      serde_json::from_str::<ResponseMsg>(scenario_msg.to_text().unwrap()),
      Ok(ResponseMsg::Scenarios(_))
    ),
    "Expected scenario response, got {scenario_msg:?}."
  );
  debug!("(webservers.drain_initialization_messages) Drained scenario message.");

  let Ok(template_msg) = stream.next().await.unwrap() else {
    panic!("Expected template response.  Got error.");
  };
  assert!(
    matches!(
      serde_json::from_str::<ResponseMsg>(template_msg.to_text().unwrap()),
      Ok(ResponseMsg::DesignTemplateResponse(_))
    ),
    "Expected template response, got {template_msg:?}."
  );
  debug!("Drained template initialization message.");
}

/**
 * Drain all messages the server send post-creation/joining of a scenario.
 */
async fn drain_post_scenario_messages(stream: &mut MyWebSocket) {
  let Ok(entities_msg) = stream.next().await.unwrap() else {
    panic!("Expected entities response.  Got error.");
  };
  assert!(
    matches!(
      serde_json::from_str::<ResponseMsg>(entities_msg.to_text().unwrap()),
      Ok(ResponseMsg::EntityResponse(_))
    ),
    "Expected entities response, got {entities_msg:?}."
  );
  debug!("Drained entities initialization message.");

  let Ok(users_msg) = stream.next().await.unwrap() else {
    panic!("Expected users response.  Got error.");
  };
  assert!(
    matches!(
      serde_json::from_str::<ResponseMsg>(users_msg.to_text().unwrap()),
      Ok(ResponseMsg::Users(_))
    ),
    "Expected users response, got {users_msg:?}."
  );
  debug!("Drained users initialization message.");
}

/**
 * Send a quit message to cleanly end a test.
 */
async fn send_quit(stream: &mut MyWebSocket) {
  stream
    .send(serde_json::to_string(&RequestMsg::Quit).unwrap().into())
    .await
    .unwrap();

  stream.close(None).await.unwrap();
}

/**
 * Do authentication with the test server
 * Return the user name and the key from `SetCookie`
 * Also drain the initialization messages.
 */
async fn test_authenticate(stream: &mut MyWebSocket) -> Result<String, String> {
  let msg = RequestMsg::Login(LoginMsg {
    code: "test_code".to_string(),
  });

  let body = rpc(stream, msg).await;
  if let ResponseMsg::AuthResponse(auth_response) = body {
    drain_initialization_messages(stream).await;
    Ok(auth_response.email)
  } else {
    Err(format!("Expected auth response to login. Got {body:?}"))
  }
}

async fn test_create_scenario(stream: &mut MyWebSocket) -> Result<(), String> {
  let msg = RequestMsg::CreateScenario(CreateScenarioMsg {
    name: "test_scenario".to_string(),
    scenario: String::new(),
  });
  let body = rpc(stream, msg).await;

  // Only when creating scenarios, we get back a Scenarios message
  let Ok(scenario_msg) = stream.next().await.unwrap() else {
    panic!("Expected scenario response.  Got error.");
  };
  assert!(
    matches!(
      serde_json::from_str::<ResponseMsg>(scenario_msg.to_text().unwrap()),
      Ok(ResponseMsg::Scenarios(_))
    ),
    "Expected scenario response, got {scenario_msg:?}."
  );

  if let ResponseMsg::JoinedScenario(_) = body {
    drain_post_scenario_messages(stream).await;
    Ok(())
  } else {
    Err(format!("Expected simple message.  Got {body:?}"))
  }
}

async fn test_join_scenario(stream: &mut MyWebSocket) -> Result<(), String> {
  let msg = RequestMsg::JoinScenario(JoinScenarioMsg {
    scenario_name: "test_scenario".to_string(),
  });
  let body = rpc(stream, msg).await;
  if let ResponseMsg::JoinedScenario(_) = body {
    drain_post_scenario_messages(stream).await;
    Ok(())
  } else {
    Err(format!("Expected simple message.  Got {body:?}"))
  }
}

/**
 * Test for get_designs in server.
 */
#[test_log::test(tokio::test)]
async fn integration_get_designs() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();

  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  let body = rpc(&mut stream, RequestMsg::DesignTemplateRequest).await;

  assert!(
    matches!(body, ResponseMsg::DesignTemplateResponse(_)),
    "Improper response to design request received."
  );

  if let ResponseMsg::DesignTemplateResponse(designs) = body {
    assert!(!designs.is_empty(), "Received empty design list.");
    assert!(designs.contains_key("Buccaneer"), "Buccaneer not found in designs.");
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();

  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  let body = rpc(&mut stream, RequestMsg::EntitiesRequest).await;
  assert!(
    matches!(body, ResponseMsg::EntityResponse(_)),
    "Improper response to get request received."
  );

  if let ResponseMsg::EntityResponse(entities) = body {
    assert!(entities.is_empty(), "Expected empty entities list.");
  }
  send_quit(&mut stream).await;
}

#[tokio::test]
async fn integration_action_without_login() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();

  // Intentionally skip test authenticate here.
  let body = rpc(&mut stream, RequestMsg::EntitiesRequest).await;
  assert!(
    matches!(body, ResponseMsg::PleaseLogin),
    "Expected request to log in, but instead got {body:?}"
  );

  send_quit(&mut stream).await;
}

/**
 * Test that we can add a ship to the server and get it back.
 */
#[test_log::test(tokio::test)]
async fn integration_add_ship() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  // Need this only because we are going to deserialize ships.
  callisto::ship::config_test_ship_templates().await;

  let ship = AddShipMsg {
    name: "ship1".to_string(),
    position: [0.0, 0.0, 0.0].into(),
    velocity: [0.0, 0.0, 0.0].into(),
    design: "Buccaneer".to_string(),
    crew: None,
  };

  let body = rpc(&mut stream, RequestMsg::AddShip(ship)).await;

  assert!(
    matches!(body, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"),
    "Improper response to add ship request received."
  );

  let entities = rpc(&mut stream, RequestMsg::EntitiesRequest).await;

  assert!(
    matches!(entities, ResponseMsg::EntityResponse(_)),
    "Improper response to entities request received."
  );

  if let ResponseMsg::EntityResponse(entities) = entities {
    let compare = json!({"ships":[
        {"name":"ship1","position":[0.0,0.0,0.0],"velocity":[0.0,0.0,0.0],
         "plan":[[[0.0,0.0,0.0],50000]],
         "design":"Buccaneer",
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
  }
  send_quit(&mut stream).await;
}

/*
* Test that we can add a ship, a planet, and a missile to the server and get them back.
*/
#[test_log::test(tokio::test)]
async fn integration_add_planet_ship() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  // Need this only because we are going to deserialize ships.
  callisto::ship::config_test_ship_templates().await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 2000.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;

  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship2".to_string(),
      position: [10000.0, 10000.0, 10000.0].into(),
      velocity: [10000.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let response = rpc(&mut stream, RequestMsg::EntitiesRequest).await;
  if let ResponseMsg::EntityResponse(entities) = response {
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
  } else {
    panic!("Improper response to entities request received.");
  }

  let message = rpc(
    &mut stream,
    RequestMsg::AddPlanet(AddPlanetMsg {
      name: "planet1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      color: "red".to_string(),
      radius: 1.5e6,
      mass: 3e24,
      primary: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add planet action executed"));

  // This time lets just grab the entities that follows each such change (vs requesting one)
  let entities = drain_entity_response(&mut stream).await;

  if let ResponseMsg::EntityResponse(entities) = entities {
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

    assert_json_eq!(entities, compare);
  } else {
    panic!("Improper response to entities request received.");
  }

  let message = rpc(
    &mut stream,
    RequestMsg::AddPlanet(AddPlanetMsg {
      name: "planet2".to_string(),
      position: [1_000_000.0, 0.0, 0.0].into(),
      color: "red".to_string(),
      radius: 1.5e6,
      mass: 1e23,
      primary: Some("planet1".to_string()),
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add planet action executed"));

  let entities = drain_entity_response(&mut stream).await;

  if let ResponseMsg::EntityResponse(entities) = entities {
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [1000.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;

  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let _response = rpc(&mut stream, RequestMsg::ModifyActions(EMPTY_FIRE_ACTIONS_MSG)).await;
  let entity_msg = drain_entity_response(&mut stream).await;
  let ResponseMsg::EntityResponse(entities) = entity_msg else {
    panic!("Expected EntityResponse");
  };
  assert!(entities.actions.is_empty(), "Expected an empty action list");

  let response = rpc(&mut stream, RequestMsg::Update).await;
  assert!(matches!(response, ResponseMsg::Effects(eq) if eq.is_empty()));

  let entities = drain_entity_response(&mut stream).await;
  if let ResponseMsg::EntityResponse(entities) = entities {
    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    assert_eq!(ship.get_position(), Vec3::new(1000.0 * DELTA_TIME_F64, 0.0, 0.0));
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [1000.0, 0.0, 0.0].into(),
      design: "System Defense Boat".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship2".to_string(),
      position: [5000.0, 0.0, 5000.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "System Defense Boat".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let fire_actions = vec![(
    "ship1".to_string(),
    vec![ShipAction::FireAction {
      weapon_id: 1,
      target: "ship2".to_string(),
      called_shot_system: None,
    }],
  )];

  let _response = rpc(&mut stream, RequestMsg::ModifyActions(fire_actions)).await;
  let entity_msg = drain_entity_response(&mut stream).await;
  if let ResponseMsg::EntityResponse(entities) = entity_msg {
    assert!(
      entities.actions.iter().any(|(name, _)| name == "ship1"),
      "Expected ship1 in actions"
    );
  } else {
    panic!("Expected EntityResponse");
  }

  let effects = rpc(&mut stream, RequestMsg::Update).await;
  if let ResponseMsg::Effects(effects) = effects {
    let filtered_effects: Vec<_> = effects.iter().filter(|e| !matches!(e, EffectMsg::Message { .. })).collect();

    let compare = vec![EffectMsg::ShipImpact {
      target: "ship2".to_string(),
      position: [5000.0, 0.0, 5000.0].into(),
    }];
    assert_json_eq!(filtered_effects, compare);
  } else {
    panic!("Improper response to update request received.");
  }

  let entities = drain_entity_response(&mut stream).await;
  if let ResponseMsg::EntityResponse(entities) = entities {
    let compare = json!({"ships":[
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let message = rpc(&mut stream, RequestMsg::Remove("ship1".to_string())).await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Remove action executed"));

  let entities = drain_entity_response(&mut stream).await;

  if let ResponseMsg::EntityResponse(entities) = entities {
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

  let entities = drain_entity_response(&mut stream).await;
  if let ResponseMsg::EntityResponse(entities) = entities {
    let ship = entities.ships.get("ship1").unwrap().read().unwrap();
    let flight_plan = &ship.plan;
    assert_eq!(flight_plan.0 .0, [0.0, 0.0, 0.0].into());
    assert_eq!(flight_plan.0 .1, DEFAULT_ACCEL_DURATION);
    assert!(!flight_plan.has_second());
  }

  let message = rpc(
    &mut stream,
    RequestMsg::SetPlan(SetPlanMsg {
      name: "ship1".to_string(),
      plan: vec![([1.0, 2.0, 2.0].into(), 50000)].into(),
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Set acceleration action executed"));

  let entities = drain_entity_response(&mut stream).await;
  if let ResponseMsg::EntityResponse(entities) = entities {
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _cookie = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  let message = rpc(
    &mut stream,
    RequestMsg::ComputePath(ComputePathMsg {
      entity_name: "ship1".to_string(),
      end_pos: [58_842_000.0, 0.0, 0.0].into(),
      end_vel: [0.0, 0.0, 0.0].into(),
      standoff_distance: 0.0,
      target_velocity: None,
      target_acceleration: None,
    }),
  )
  .await;

  if let ResponseMsg::FlightPath(plan) = message {
    assert_eq!(plan.path.len(), 10);
    assert_eq!(plan.path[0], Vec3::zero());
    assert_ulps_eq!(
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
      epsilon = 1e-5
    );
    assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-5);
    let (a, t) = plan.plan.0.into();
    assert_ulps_eq!(a, Vec3 { x: 3.0, y: 0.0, z: 0.0 } * G, epsilon = 1e-5);
    assert_eq!(t, 1414);

    if let Some(accel) = plan.plan.1 {
      let (a, _t) = accel.into();
      assert_ulps_eq!(
        a,
        Vec3 {
          x: -3.0,
          y: 0.0,
          z: 0.0
        } * G,
        epsilon = 1e-5
      );
    } else {
      panic!("Expecting second acceleration.");
    }
    assert_eq!(t, 1414);
  } else {
    panic!("Improper response to compute path request received: {message:?}");
  }

  send_quit(&mut stream).await;
}

/**
 * Test that will compute a path with standoff value
 */
#[test_log::test(tokio::test)]
async fn integration_compute_path_with_standoff() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  callisto::ship::config_test_ship_templates().await;

  // Add test ship
  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));
  drain_entity_response(&mut stream).await;

  // Compute path with standoff
  let message = rpc(
    &mut stream,
    RequestMsg::ComputePath(ComputePathMsg {
      entity_name: "ship1".to_string(),
      end_pos: [58_842_000.0, 0.0, 0.0].into(),
      end_vel: [0.0, 0.0, 0.0].into(),
      standoff_distance: 60000.0,
      target_velocity: None,
      target_acceleration: None,
    }),
  )
  .await;

  if let ResponseMsg::FlightPath(plan) = message {
    assert_eq!(plan.path.len(), 10);
    assert_eq!(plan.path[0], Vec3::zero());
    assert_ulps_eq!(
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
      epsilon = 1e-5
    );
    assert_ulps_eq!(plan.end_velocity, Vec3::zero(), epsilon = 1e-7);

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
        epsilon = 1e-5
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
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  // Test invalid ship design
  let message = rpc(
    &mut stream,
    RequestMsg::AddShip(AddShipMsg {
      name: "bad_ship".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "NonexistentDesign".to_string(),
      crew: None,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::Error(_)));

  // Test invalid planet primary
  let message = rpc(
    &mut stream,
    RequestMsg::AddPlanet(AddPlanetMsg {
      name: "planet1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      color: "red".to_string(),
      primary: Some("InvalidPrimary".to_string()),
      radius: 1.5e6,
      mass: 3e24,
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::Error(_)));

  // Test invalid compute path request
  let message = rpc(
    &mut stream,
    RequestMsg::ComputePath(ComputePathMsg {
      entity_name: "nonexistent_ship".to_string(),
      end_pos: [0.0, 0.0, 0.0].into(),
      end_vel: [0.0, 0.0, 0.0].into(),
      standoff_distance: 0.0,
      target_velocity: None,
      target_acceleration: None,
    }),
  )
  .await;
  assert!(
    matches!(message, ResponseMsg::Error(_)),
    "Expected error for invalid compute path request, got {message:?}"
  );

  // This isn't an error but want to print this warning at the same log level as errors.
  error!(
    "(integration_malformed_requests) Expect an error to occur after this from server (Failed to parse, expected f64)"
  );
  // Test invalid flight plan
  let message = rpc(
    &mut stream,
    RequestMsg::SetPlan(SetPlanMsg {
      name: "nonexistent_ship".to_string(),
      plan: vec![([f64::NAN, 0.0, 0.0].into(), 1000)].into(),
    }),
  )
  .await;
  assert!(matches!(message, ResponseMsg::Error(_)));
  // Test fire action with invalid parameters
  let _response = rpc(
    &mut stream,
    RequestMsg::ModifyActions(vec![(
      "nonexistent_ship".to_string(),
      vec![ShipAction::FireAction {
        weapon_id: 0,
        target: "nonexistent_target".to_string(),
        called_shot_system: None,
      }],
    )]),
  )
  .await;
  drain_entity_response(&mut stream).await;
  let message = rpc(&mut stream, RequestMsg::Update).await;
  assert!(
    matches!(&message, ResponseMsg::Effects(x) if x.is_empty()),
    "Expected empty effects for invalid fire action, got {message:?}"
  );

  send_quit(&mut stream).await;
}

#[test_log::test(tokio::test)]
async fn integration_bad_requests() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

  // Test setting crew actions for non-existent ship
  let msg = RequestMsg::SetPilotActions(SetPilotActions {
    ship_name: "ship1".to_string(),
    dodge_thrust: Some(3),
    assist_gunners: Some(true),
  });
  let response = rpc(&mut stream, msg).await;
  assert!(matches!(response, ResponseMsg::Error(_)));

  // Test adding planet with invalid primary
  let msg = RequestMsg::AddPlanet(AddPlanetMsg {
    name: "planet1".to_string(),
    position: [0.0, 0.0, 0.0].into(),
    color: "red".to_string(),
    primary: Some("InvalidPlanet".to_string()),
    radius: 1.5e6,
    mass: 3e24,
  });
  let response = rpc(&mut stream, msg).await;
  assert!(matches!(response, ResponseMsg::Error(_)));

  // Test removing non-existent ship
  let msg = RequestMsg::Remove("ship1".to_string());
  let response = rpc(&mut stream, msg).await;
  assert!(matches!(response, ResponseMsg::Error(_)));

  // Test setting flight plan for non-existent ship
  let msg = RequestMsg::SetPlan(SetPlanMsg {
    name: "ship1".to_string(),
    plan: vec![([1.0, 2.0, 2.0].into(), 50000)].into(),
  });
  let response = rpc(&mut stream, msg).await;
  assert!(matches!(response, ResponseMsg::Error(_)));

  // Test fire action with invalid weapon_id
  let msg = RequestMsg::ModifyActions(vec![(
    "ship1".to_string(),
    vec![ShipAction::FireAction {
      weapon_id: usize::MAX,
      target: "ship2".to_string(),
      called_shot_system: None,
    }],
  )]);
  let _response = rpc(&mut stream, msg).await;
  drain_entity_response(&mut stream).await;

  let response = rpc(&mut stream, RequestMsg::Update).await;

  assert!(
    matches!(&response, ResponseMsg::Effects(x) if x.is_empty()),
    "Expected empty effects for invalid weapon_id. Instead got {response:?}"
  );

  send_quit(&mut stream).await;
}

#[tokio::test]
async fn integration_fail_login() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  // Test unauthenticated connection
  let socket_url = format!("ws://127.0.0.1:{port}/ws");
  let stream = connect_async(socket_url).await;

  if cfg!(feature = "no_tls_upgrade") {
    assert!(stream.is_ok(), "Expected connection to succeed without TLS upgrade");
  } else {
    assert!(stream.is_err(), "Expected connection to fail without authentication");
  }

  // Test invalid authentication
  let mut stream = open_socket(port).await.unwrap();
  let message = rpc(
    &mut stream,
    RequestMsg::Login(LoginMsg {
      code: "invalid_code".to_string(),
    }),
  )
  .await;
  assert!(
    matches!(message, ResponseMsg::Error(_)),
    "Expected error for invalid authentication. Got: {message:?}"
  );

  send_quit(&mut stream).await;
}

/**
 * Test setting crew actions through WebSocket
 */
#[test_log::test(tokio::test)]
async fn integration_set_crew_actions() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream).await.unwrap();
  test_create_scenario(&mut stream).await.unwrap();

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
    RequestMsg::AddShip(AddShipMsg {
      name: "test_ship".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: Some(crew),
    }),
  )
  .await;

  assert!(matches!(message, ResponseMsg::SimpleMsg(msg) if msg == "Add ship action executed"));

  // Verify the crew skills were set by checking entities
  let response = drain_entity_response(&mut stream).await;

  if let ResponseMsg::EntityResponse(entities) = response {
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
    RequestMsg::SetPilotActions(SetPilotActions {
      ship_name: "test_ship".to_string(),
      dodge_thrust: Some(1),
      assist_gunners: Some(true),
    }),
  )
  .await;

  assert!(matches!(message, ResponseMsg::SimpleMsg(_)));
  drain_entity_response(&mut stream).await;

  // Test setting crew actions for non-existent ship
  let message = rpc(
    &mut stream,
    RequestMsg::SetPilotActions(SetPilotActions {
      ship_name: "nonexistent_ship".to_string(),
      dodge_thrust: Some(1),
      assist_gunners: Some(true),
    }),
  )
  .await;

  assert!(matches!(message, ResponseMsg::Error(_)));

  send_quit(&mut stream).await;
}

#[test_log::test(tokio::test)]
async fn integration_multi_client_test() {
  let port = get_next_port();
  let _server = spawn_test_server(port).await;

  let mut stream1 = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream1).await.unwrap();
  test_create_scenario(&mut stream1).await.unwrap();
  let mut stream2 = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream2).await.unwrap();
  test_join_scenario(&mut stream2).await.unwrap();
  let mut stream3 = open_socket(port).await.unwrap();
  let _ = test_authenticate(&mut stream3).await.unwrap();
  test_join_scenario(&mut stream3).await.unwrap();

  callisto::ship::config_test_ship_templates().await;
  rpc(
    &mut stream1,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship1".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;

  rpc(
    &mut stream2,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship2".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;

  // Before we do our RPC, get rid of the extra two messages
  debug!("(integration_multi_client_test) Draining extra messages (1).");
  drain_entity_response(&mut stream3).await;
  debug!("(integration_multi_client_test) Draining extra messages (2).");
  drain_entity_response(&mut stream3).await;

  rpc(
    &mut stream3,
    RequestMsg::AddShip(AddShipMsg {
      name: "ship3".to_string(),
      position: [0.0, 0.0, 0.0].into(),
      velocity: [0.0, 0.0, 0.0].into(),
      design: "Buccaneer".to_string(),
      crew: None,
    }),
  )
  .await;

  // Should now be just the 1 entity response message (since we drained above)
  let entities = drain_entity_response(&mut stream3).await;
  if let ResponseMsg::EntityResponse(entities) = entities {
    assert_eq!(entities.ships.len(), 3);
  } else {
    panic!("Expected EntityResponse");
  }

  send_quit(&mut stream1).await;
}

#[cfg_attr(feature = "ci", ignore)]
#[tokio::test]
async fn integration_create_regular_server() {
  let port_1 = get_next_port();
  let port_2 = get_next_port();

  // Test regular server
  let mut server1 = spawn_server(port_1, false, None, None, true).await.unwrap();
  assert!(server1.try_wait().unwrap().is_none());

  // Test server with design file
  let mut server2 = spawn_server(port_2, false, None, Some("./tests/test_templates.json".to_string()), true)
    .await
    .unwrap();
  assert!(server2.try_wait().unwrap().is_none());
}

pub mod combat;
mod computer;
pub mod entity;
pub mod missile;
pub mod payloads;
pub mod planet;
pub mod ship;
mod damage_tables;

extern crate pretty_env_logger;
#[macro_use]
extern crate log;

use std::sync::{Arc, Mutex};

use http_body_util::{BodyExt, Full};
use hyper::body::Bytes;
use hyper::body::{Body, Incoming};
use hyper::{Method, Request, Response, StatusCode};

use cgmath::InnerSpace;
use serde_json::from_slice;

use computer::{compute_flight_path, FlightParams};
use entity::{Entities, Entity, G};
use payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FireActionsMsg, FlightPathMsg, RemoveEntityMsg,
    SetPlanMsg,
};

use combat::do_fire_actions;

enum SizeCheckError {
    SizeErr(Response<Full<Bytes>>),
    HyperErr(hyper::Error),
}

impl From<hyper::Error> for SizeCheckError {
    fn from(err: hyper::Error) -> Self {
        SizeCheckError::HyperErr(err)
    }
}

fn build_ok_response(body: &str) -> Response<Full<Bytes>> {
    let msg = Bytes::copy_from_slice(format!("{{ \"msg\" : \"{body}\" }}").as_bytes());
    let resp = Response::builder()
        .status(StatusCode::OK)
        .header("Access-Control-Allow-Origin", "*")
        .body(msg.into())
        .unwrap();

    resp.clone()
}

// Read a body while also protecting our server from massive bodies.
async fn get_body_size_check(req: Request<Incoming>) -> Result<Bytes, SizeCheckError> {
    let upper = req.body().size_hint().upper().unwrap_or(u64::MAX);
    if upper > 1024 * 64 {
        let mut resp: Response<Full<Bytes>> = Response::new("Body too big".as_bytes().into());
        *resp.status_mut() = StatusCode::PAYLOAD_TOO_LARGE;
        return Err(SizeCheckError::SizeErr(resp));
    }
    let body_bytes = req.collect().await?.to_bytes();
    Ok(body_bytes)
}

macro_rules! deserialize_body_or_respond {
    ($req: ident, $msg_type:tt) => {{
        info!("Received and processing {} request.", stringify!($msg_type));
        let body_bytes = match get_body_size_check($req).await {
            Ok(bytes) => bytes,
            Err(SizeCheckError::SizeErr(resp)) => return Ok(resp),
            Err(SizeCheckError::HyperErr(err)) => return Err(err),
        };

        debug!("Body bytes: {:?}", body_bytes);

        let msg: $msg_type = match from_slice(&body_bytes) {
            Ok(msg) => msg,
            Err(e) => {
                warn!("Invalid JSON ({}): {:?}", e, body_bytes);
                let mut resp: Response<Full<Bytes>> =
                    Response::new("Invalid JSON".as_bytes().into());
                *resp.status_mut() = StatusCode::BAD_REQUEST;
                return Ok(resp);
            }
        };
        msg
    }};
}

pub async fn handle_request(
    req: Request<Incoming>,
    entities: Arc<Mutex<Entities>>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    info!(
        "Request: {:?}\n\tmethod: {}\n\turi: {}",
        req,
        req.method(),
        req.uri().path()
    );
    match (req.method(), req.uri().path()) {
        (&Method::OPTIONS, _) => {
            let mut resp = Response::new("".as_bytes().into());
            resp.headers_mut()
                .insert("Access-Control-Allow-Origin", "*".parse().unwrap());
            resp.headers_mut().insert(
                "Access-Control-Allow-Methods",
                "POST, GET, OPTIONS".parse().unwrap(),
            );
            resp.headers_mut().insert(
                "Access-Control-Allow-Headers",
                "Content-Type".parse().unwrap(),
            );
            Ok(resp)
        }
        (&Method::POST, "/add_ship") => {
            let ship = deserialize_body_or_respond!(req, AddShipMsg);

            // Add the ship to the server
            entities.lock().unwrap().add_ship(
                ship.name,
                ship.position,
                ship.velocity,
                ship.acceleration,
                &ship.usp,
            );

            Ok(build_ok_response("Add ship action executed"))
        }
        (&Method::POST, "/add_planet") => {
            let planet = deserialize_body_or_respond!(req, AddPlanetMsg);

            // Add the planet to the server
            entities.lock().unwrap().add_planet(
                planet.name,
                planet.position,
                planet.color,
                planet.primary,
                planet.radius,
                planet.mass,
            );

            Ok(build_ok_response("Add planet action executed"))
        }
        //TODO: Remove this method
        /*
        (&Method::POST, "/launch_missile") => {
            let missile = deserialize_body_or_respond!(req, LaunchMissileMsg);

            // Add the missile to the server
            entities
                .lock()
                .unwrap()
                .launch_missile(&missile.source, &missile.target);

            Ok(build_ok_response("Launch missile action executed"))
        }*/
        (&Method::POST, "/remove") => {
            let name = deserialize_body_or_respond!(req, RemoveEntityMsg);
            debug!("Removing entity: {}", name);

            // Remove the entity from the server
            let mut entities = entities.lock().unwrap();
            if entities.ships.remove(&name).is_none()
                && entities.planets.remove(&name).is_none()
                && entities.missiles.remove(&name).is_none()
            {
                warn!("Unable to find entity named {} to remove", name);
                let err_msg = format!("Unable to find entity named {} to remove", name);
                return Ok(Response::new(
                    Bytes::copy_from_slice(err_msg.as_bytes()).into(),
                ));
            }

            Ok(build_ok_response("Remove action executed"))
        }
        (&Method::POST, "/set_plan") => {
            info!("Received and processing plan set request.");
            let plan_msg = deserialize_body_or_respond!(req, SetPlanMsg);

            // Change the acceleration of the entity
            let okay = entities
                .lock()
                .unwrap()
                .set_flight_plan(&plan_msg.name, &plan_msg.plan);

            if !okay {
                warn!(
                    "Unable to set flight plan {:?} for entity {}",
                    &plan_msg.plan, plan_msg.name
                );
                // When set_flight_plan fails, we don't set a new plan. So return a 304 Not Modified
                let err_msg = format!("Unable to set acceleration for entity {}", plan_msg.name);
                let resp: Response<Full<Bytes>> = Response::builder()
                    .status(StatusCode::BAD_REQUEST)
                    .header("Access-Control-Allow-Origin", "*")
                    .body(Bytes::copy_from_slice(err_msg.as_bytes()).into())
                    .unwrap();
                return Ok(resp);
            }
            Ok(build_ok_response("Set acceleration action executed"))
        }
        (&Method::POST, "/update") => {
            info!("(/update) Received and processing update request.");

            // Outlining the logic here to capture it in one place.
            // We've been sent a set of FireActions from the client.  This can be beam weapons, sand, or missiles being launched.
            // For combat we need to apply all that to a copy of our ships, apply all attacks, then copy those ships back once the effect
            // of that combat is applied.  We do this so that all fire is simultaneous and order doesn't matter.
            // 1. Create a copy of current ships called next_round_ships
            // 2. First all the fire actions are applied to the next_round_ships.  
            // 3. Fire actions also can create a set of missiles and a set
            // of effects (things to show back to the user - explosions and such).
            // 4. Those missiles are then added into the list of missiles in entities. The effects are saved to send back to user after adding in
            // any effects from updating missiles.
            // 5. Copy back the next_round_ships as all the ships are done firing.
            // So at this point we have a bunch of missiles in flight, and all other weapons have fired.
            // 6. update_all updates all the planets, missiles, ships in that order.  Ships come last so that damage done to them impacts
            // their movement.
            // In updating missiles, they may hit and cause damage.  Note we don't in this case need a clone of the ships as what happens
            // to a ship that launches a missile doesn't impact the missile once its taken flight.
            // 7. Once all combat is done (fire actions and missile updates) we do a pass over each ship to validate it (really see if it explodes)
            // 8. We collect all the effects from the fire actions, the missile updates, and ship validation.
            // 9. Send those effects back to the user.
            // Call out each of these steps in the code below to make this clear.

            // Get the set of fire actions provided with the REST call to update
            let fire_actions = deserialize_body_or_respond!(req, FireActionsMsg);

            info!("(/update) Processing {} fire_actions.", fire_actions.len());

            // Grab the lock on entities
            let mut entities = entities
                .lock()
                .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {}", e));

            // 1. Create a copy of ships called next_round_ships
            let mut next_round_ships = entities.ships.clone();

            // 2. Apply all fire actions to next_round_ships
            // 3. Get back a set of missiles and effects.
            let (new_missiles, mut effects) = fire_actions
                .into_iter()
                .map(|action| do_fire_actions(&action.0, &mut next_round_ships, action.1))
                .fold(
                    (vec![], vec![]),
                    |(mut missiles, mut effects), (mut new_missiles, mut new_effects)| {
                        missiles.append(&mut new_missiles);
                        effects.append(&mut new_effects);
                        (missiles, effects)
                    },
                );

            // 4. Launch all missiles created by the fire actions
            new_missiles.iter().for_each(|missile| {
                entities.launch_missile(&missile.source, &missile.target);
            });

            // 5. Copy back the next_round_ships into entities
            entities.ships = next_round_ships;


            // 6. Update all planets, ships, and missiles.
            // 7. Collect the effects and append them to the ones created by the fire actions.
            effects.append(&mut entities.update_all());

            debug!("(/update) Effects: {:?}", effects);

            // 8. Marshall the events and reply with them back to the user.
            let json = match serde_json::to_string(&effects) {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting update actions to JSON".as_bytes().into(),
                    ));
                }
            };

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            Ok(resp)
        }

        (&Method::POST, "/compute_path") => {
            let msg = deserialize_body_or_respond!(req, ComputePathMsg);

            info!(
                "(/compute_path) Received and processing compute path request. {:?}",
                msg
            );

            debug!(
                "(/compute_path) Computing path for entity: {} End pos: {:?} End vel: {:?}",
                msg.entity_name, msg.end_pos, msg.end_vel
            );
            // Do this in a block to clean up the lock as soon as possible.
            let (start_pos, start_vel, max_accel) = {
                let entities = entities.lock().unwrap();
                let entity = entities
                    .ships
                    .get(&msg.entity_name)
                    .unwrap()
                    .read()
                    .unwrap();
                (
                    entity.get_position(),
                    entity.get_velocity(),
                    entity.usp.maneuver as f64 * G,
                )
            };

            let adjusted_end_pos = if msg.standoff_distance > 0.0 {
                msg.end_pos - (msg.end_pos - start_pos).normalize() * msg.standoff_distance
            } else {
                msg.end_pos
            };

            if msg.standoff_distance > 0.0 {
                debug!("(/compute_path) Standoff distance: {:0.0?} Adjusted end pos: {:0.0?} Original end pos {:0.0?}Difference {:0.0?}", msg.standoff_distance, adjusted_end_pos, msg.end_pos, 
                    (adjusted_end_pos - msg.end_pos).magnitude());
            }

            let params = FlightParams::new(
                start_pos,
                adjusted_end_pos,
                start_vel,
                msg.end_vel,
                msg.target_velocity,
                max_accel,
            );

            debug!("(/compute_path)Call computer with params: {:?}", params);

            let plan: FlightPathMsg = compute_flight_path(&params);

            let json = match serde_json::to_string(&plan) {
                Ok(json) => json,
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting flight path to JSON".as_bytes().into(),
                    ));
                }
            };

            debug!("(/compute_path) Flight path response: {}", json);

            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();
            Ok(resp)
        }

        (&Method::GET, "/") => {
            info!("Received and processing get request.");
            let json = match serde_json::to_string::<Entities>(&entities.lock().unwrap()) {
                Ok(json) => {
                    info!("(/) Entities: {:?}", json);
                    json
                }
                Err(_) => {
                    return Ok(Response::new(
                        "Error converting entities to JSON".as_bytes().into(),
                    ));
                }
            };

            debug!("Entities response: {}", json);
            let resp = Response::builder()
                .status(StatusCode::OK)
                .header("Access-Control-Allow-Origin", "*")
                .body(Bytes::copy_from_slice(json.as_bytes()).into())
                .unwrap();

            Ok(resp)
        }
        _ => {
            // Return a 404 Not Found response for any other requests
            Ok(Response::builder()
                .status(404)
                .body("Not Found".as_bytes().into())
                .unwrap())
        }
    }
}

use std::result::Result;
use std::sync::{Arc, Mutex};

use cgmath::InnerSpace;
use rand::RngCore;

use crate::computer::{compute_flight_path, FlightParams};
use crate::entity::{Entities, Entity, G};
use crate::payloads::{
    AddPlanetMsg, AddShipMsg, ComputePathMsg, FireActionsMsg, FlightPathMsg, RemoveEntityMsg,
    SetPlanMsg,
};
use crate::cov_util::{debug, info};
// Struct wrapping an Arc<Mutex<Entities>> (i.e. a multi-threaded safe Entities)
// Add function beyond what Entities does and provides an API to our server.
pub struct Server {
    entities: Arc<Mutex<Entities>>,
    rng: Box<dyn RngCore + Send>,
}

impl Server {
    pub fn new(entities: Arc<Mutex<Entities>>, rng: Box<dyn RngCore + Send>) -> Self {
        Server { entities, rng }
    }

    pub fn add_ship(&self, ship: AddShipMsg) -> Result<String, String> {
        // Add the ship to the server
        self.entities.lock().unwrap().add_ship(
            ship.name,
            ship.position,
            ship.velocity,
            ship.acceleration,
            &ship.usp,
        );

        Ok("Add ship action executed".to_string())
    }

    pub fn get_entities(&self) -> Result<Entities, String> {
        Ok(self.entities.lock().unwrap().clone())
    }

    pub fn add_planet(&self, planet: AddPlanetMsg) -> Result<String, String> {
        // Add the planet to the server
        self.entities.lock().unwrap().add_planet(
            planet.name,
            planet.position,
            planet.color,
            planet.primary,
            planet.radius,
            planet.mass,
        );

        Ok("Add planet action executed".to_string())
    }

    pub fn remove(&self, name: RemoveEntityMsg) -> Result<String, String> {
        // Remove the entity from the server
        let mut entities = self.entities.lock().unwrap();
        if entities.ships.remove(&name).is_none()
            && entities.planets.remove(&name).is_none()
            && entities.missiles.remove(&name).is_none()
        {
            warn!("Unable to find entity named {} to remove", name);
            let err_msg = format!("Unable to find entity named {} to remove", name);
            return Err(err_msg);
        }

        Ok("Remove action executed".to_string())
    }

    pub fn set_plan(&self, plan_msg: SetPlanMsg) -> Result<(), String> {
        // Change the acceleration of the entity
        self.entities
            .lock()
            .unwrap()
            .set_flight_plan(&plan_msg.name, &plan_msg.plan)
    }

    pub fn update(&mut self, fire_actions: FireActionsMsg) -> Result<String, String> {
        // Grab the lock on entities
        let mut entities = self
            .entities
            .lock()
            .unwrap_or_else(|e| panic!("Unable to obtain lock on Entities: {}", e));

        // 1. This method will perform all the fire actions on a clone of the ships and then copy it back over the current ships
        // so that all effects are "simultaneous"
        // 2. Add all new missiles into the entities structure.
        // 3. Return a set of effects
        let mut effects = entities.fire_actions(fire_actions, &mut self.rng);

        // 4. Update all entities (ships, planets, missiles) and gather in their effects.
        effects.append(&mut entities.update_all(&mut self.rng));

        debug!("(/update) Effects: {:?}", effects);

        // 5. Marshall the events and reply with them back to the user.
        let json = match serde_json::to_string(&effects) {
            Ok(json) => json,
            Err(_) => return Err("Error converting update actions to JSON".to_string()),
        };

        Ok(json)
    }

    pub fn compute_path(&self, msg: ComputePathMsg) -> Result<String, String> {
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
            let entities = self.entities.lock().unwrap();
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
            Err(_) => return Err("Error converting flight path to JSON".to_string()),
        };

        debug!("(/compute_path) Flight path response: {}", json);

        Ok(json)
    }

    pub fn get(&self) -> Result<String, String> {
        info!("Received and processing get request.");
        match serde_json::to_string::<Entities>(&self.entities.lock().unwrap()) {
            Ok(json) => {
                info!("(/) Entities: {:?}", json);
                Ok(json)
            }
            Err(_) => Err("Error converting entities to JSON".to_string()),
        }
    }
}

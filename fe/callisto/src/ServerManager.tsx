import { Acceleration, EntityRefreshCallback, FlightPathResult, Ship, Missile, Planet } from "./Universal";
import { Effect } from "./Effects";

const address = "localhost";
const port = "3000";

export function addShip(name: string, position: [number, number, number], velocity: [number, number, number], acceleration: [number, number, number], callBack: EntityRefreshCallback) {
  console.log(`Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Acceleration ${acceleration}`);

  let payload = {
    name: name,
    position: position,
    velocity: velocity,
    acceleration: acceleration
  }

  fetch(`http://${address}:${port}/add_ship`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack))
    .catch((error) => console.error("Error adding entity:", error));
}

export function removeEntity(target: string, callBack: EntityRefreshCallback) {
  fetch(`http://${address}:${port}/remove`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(target),
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack))
    .catch((error) =>
      console.error("Error removing entity '" + target + "':", error)
    );
}

export function setPlan(
  target: string,
  plan: [Acceleration, Acceleration | null],
  callBack: EntityRefreshCallback
) {
  let plan_arr = [];

  // Since the Rust backend just expects null values in flightplans to be skipped
  // we have to custom build the body.
  if (plan[1] == null) {
    plan_arr = [plan[0]];
  } else {
    plan_arr = [plan[0], plan[1]];
  }
  let payload = { name: target, plan: plan_arr };

  fetch(`http://${address}:${port}/set_plan`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack))
    .catch((error) =>
      console.error(
        "Error setting acceleration for entity '" + target + "':",
        error
      )
    );
}

export function nextRound(setEvents: (events: Effect[] | null) => void, callBack: EntityRefreshCallback) {
  fetch(`http://${address}:${port}/update`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
  })
    .then((response) => response.json())
    .then((events) => setEvents(events))
    .then(() => getEntities(callBack))
    .catch((error) => console.error("Error adding entity:", error));
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  setProposedPlan: (plan: FlightPathResult | null) => void,
  target_vel: [number, number, number] | null = null,
  standoff: number = 0
) {
  if (entity_name == null) {
    setProposedPlan(null);
    return;
  }
  let payload = {
    entity_name: entity_name,
    end_pos: end_pos,
    end_vel: end_vel,
    target_velocity: target_vel,
    standoff_distance: standoff
  };

  fetch(`http://${address}:${port}/compute_path`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => response.json())
    .then((plan) => setProposedPlan(plan))
    .catch((error) => console.error("Error computing flight path:", error));
}

export function launchMissile(
  source: string,
  target: string,
  callback: EntityRefreshCallback
) {
  let payload = {
    source: source,
    target: target,
  }

  fetch(`http://${address}:${port}/launch_missile`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(payload)
  })
  .then((response) => response.json())
  .then(() => getEntities(callback))
  .catch((error) => console.error("Error launching missile", error));
}

export function getEntities(callback: EntityRefreshCallback) {

  return fetch(`http://${address}:${port}/`)
    .then((response) => response.json())
    .then((rawEntities) => {
      let ships: Ship[] = [];
      let planets: Planet[] = [];
      let missiles: Missile[] = [];

      for (const entity of rawEntities) {
      if ("Ship" in entity.kind) {
            let ship = new Ship(entity.name, entity.position, entity.velocity, entity.kind.Ship.plan);
            ships.push(ship);
      } else if ("Missile" in entity.kind) {
            let missile = new Missile(entity.name, entity.position, entity.velocity, entity.kind.Missile.acceleration);
            missiles.push(missile);
      } else if ("Planet" in entity.kind) {
            let planet = new Planet(entity.name, entity.position, entity.velocity, entity.kind.Planet.color, entity.kind.Planet.primary, entity.kind.Planet.radius, entity.kind.Planet.mass);
            planets.push(planet);
        } else {
            console.log(`Unknown entity kind: ${JSON.stringify(entity.kind)}`);
        }
      }

      console.log(`Received Entities:\nSHIPS = ${JSON.stringify(ships)}\nMISSILES = ${JSON.stringify(missiles)}\nPLANETS = ${JSON.stringify(planets)}`);
      callback({ ships: ships, planets: planets, missiles: missiles });
    })
    .catch((error) => console.error("Error fetching entities:", error));
}

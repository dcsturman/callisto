import { Acceleration, EntityRefreshCallback, FlightPathResult } from "./Universal";
import { Effect } from "./Effects";

const address = "localhost";
const port = "3000";

export function addShip(name: string, position: [number, number, number], velocity: [number, number, number], acceleration: [number, number, number], usp: string, callBack: EntityRefreshCallback) {
  console.log(`Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Acceleration ${acceleration}`);

  let payload = {
    name: name,
    position: position,
    velocity: velocity,
    acceleration: acceleration,
    usp: usp,
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

  // Since the Rust backend just expects null values in flight plans to be skipped
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
  standoff: number | null = null
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
    .then((entities) => {
      console.log(`Received Entities: ${JSON.stringify(entities)}`);
      callback(entities);
    })
    .catch((error) => console.error("Error fetching entities:", error));
}

import { Entity, EntityRefreshCallback, FlightPlan } from "./Contexts";

const address = "localhost";
const port = "3000";


export function addEntity(entity: Entity, callBack: EntityRefreshCallback) {
  console.log("Adding entity: " + JSON.stringify(entity));

  fetch(`http://${address}:${port}/add`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(entity),
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

export function setAcceleration(
  target: string,
  acceleration: [number, number, number],
  callBack: EntityRefreshCallback
) {
  let payload = { name: target, acceleration: acceleration };
  fetch(`http://${address}:${port}/setaccel`, {
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

export function nextRound(callBack: EntityRefreshCallback) {
  fetch(`http://${address}:${port}/update`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack))
    .catch((error) => console.error("Error adding entity:", error));
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  setCurrentPlan: (plan: FlightPlan | null) => void) {

  if (entity_name == null) {
    setCurrentPlan(null);
    return;
  }
  let payload = {
    entity_name: entity_name,
    end_pos: end_pos,
    end_vel: end_vel,
  };

  fetch(`http://${address}:${port}/computepath`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
  .then((response) => response.json())
  .then((plan) => setCurrentPlan(plan))
  .catch((error) => console.error("Error computing flight path:", error))
}

export function getEntities(callback: EntityRefreshCallback) {
  return fetch(`http://${address}:${port}/`)
    .then((response) => response.json())
    .then((entities) => {
      callback(entities);
    })
    .catch((error) => console.error("Error fetching entities:", error));
}

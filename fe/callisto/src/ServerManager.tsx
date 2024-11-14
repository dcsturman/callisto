import {
  Acceleration,
  EntityRefreshCallback,
  EntityList,
  FlightPathResult,
  ShipDesignTemplates,
  ShipDesignTemplate,
} from "./Universal";
import { FireActionMsg } from "./Controls";
import { Effect } from "./Effects";

const address = "localhost";
const port = "3000";

type AuthResponse = {
  email: string;
  key: string;
}

export function login(code: string, setEmail: (email: string) => void, setToken: (token: string) => void) {
  fetch(`http://${address}:${port}/login`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ "code" : code })
  })
    .then((response) => response.text())
    .then((body) =>  JSON.parse(body) as AuthResponse)
    .then((authResponse: AuthResponse) => { setEmail(authResponse.email); setToken(authResponse.key); })
    .catch((error) => console.error("Error logging in:", error));
}

export function addShip(
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  acceleration: [number, number, number],
  design: string,
  callBack: EntityRefreshCallback,
  token: string
) {
  console.log(
    `Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Acceleration ${acceleration}`
  );

  let payload = {
    name: name,
    position: position,
    velocity: velocity,
    acceleration: acceleration,
    design: design,
  };

  fetch(`http://${address}:${port}/add_ship`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack, token))
    .catch((error) => console.error("Error adding entity:", error));
}

export function removeEntity(target: string, callBack: EntityRefreshCallback, token: string) {
  fetch(`http://${address}:${port}/remove`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
    },
    mode: "cors",
    body: JSON.stringify(target),
  })
    .then((response) => response.json())
    .then(() => getEntities(callBack, token))
    .catch((error) =>
      console.error("Error removing entity '" + target + "':", error)
    );
}

export async function setPlan(
  target: string,
  plan: [Acceleration, Acceleration | null],
  callBack: EntityRefreshCallback,
  token: string
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

  let response = await fetch(`http://${address}:${port}/set_plan`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
    },
    mode: "cors",
    body: JSON.stringify(payload),
  });

  if (response.status === 200) {
    await response.json();
    getEntities(callBack, token);
  } else if (response.status === 400) {
    let msg = await response.text();
    alert(`Proposed plan cannot be assigned: ${JSON.stringify(payload)} because ${msg}`);
    console.log(`Invalid plan provided: ${JSON.stringify(payload)} because ${msg}`);
  } else {
    console.error(
      "Unknown response code " +
        response.status +
        " from server when setting plan."
    );
  }
}

export function nextRound(
  fireActions: FireActionMsg,
  setEvents: (events: Effect[] | null) => void,
  callBack: EntityRefreshCallback,
  token: string
) {
  fetch(`http://${address}:${port}/update`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
    },
    body: JSON.stringify(Object.entries(fireActions)),
    mode: "cors",
  })
    .then((response) => response.json())
    .then((events) => setEvents(events))
    .then(() => getEntities(callBack, token))
    .catch((error) => console.error("Error adding entity:", error));
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  setProposedPlan: (plan: FlightPathResult | null) => void,
  target_vel: [number, number, number] | null = null,
  standoff: number | null = null,
  token: string
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
    standoff_distance: standoff,
  };

  fetch(`http://${address}:${port}/compute_path`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
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
  callback: EntityRefreshCallback,
  token: string
) {
  let payload = {
    source: source,
    target: target,
  };

  fetch(`http://${address}:${port}/launch_missile`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "Authorization": token
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => response.json())
    .then(() => getEntities(callback, token))
    .catch((error) => console.error("Error launching missile", error));
}

export function getEntities(callback: EntityRefreshCallback, token: string) {
  return fetch(`http://${address}:${port}/`, {
    headers: {
      "Authorization": token
    }
  })
    .then((response) => response.json())
    .then((json) => EntityList.parse(json))
    .then((entities) => {
      console.log(`Received Entities: ${JSON.stringify(entities)}`);
      callback(entities);
    })
    .catch((error) => console.error("Error fetching entities:", error));
}

export async function getTemplates(
  callBack: (templates: ShipDesignTemplates) => void,
  token: string
) {
  return fetch(`http://${address}:${port}/designs`, {
    headers: {
      "Authorization": token
    }
  })
    .then((response) => response.json())
    .then((json: any) => {
      let templates: {[key: string]: ShipDesignTemplate} = {};
      Object.entries(json).forEach((entry: [string, any]) => {
        let currentTemplate: ShipDesignTemplate = ShipDesignTemplate.parse(
          entry[1]
        );
        templates[entry[0]] = currentTemplate;
      });
      return templates;
    })
    .then((templates: ShipDesignTemplates) => {
      console.log("Received Templates: ");
      for (let v of Object.values(templates)) {
        console.log(` ${v.name}`);
      }
      callBack(templates);
    })
    .catch((error) => console.error("Error fetching templates:", error));
}

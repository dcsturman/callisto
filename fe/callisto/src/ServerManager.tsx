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
import { StatusCodes, getReasonPhrase } from "http-status-codes";
import { Crew } from "./CrewBuilder";

export const CALLISTO_BACKEND =
  process.env.REACT_APP_C_BACKEND || "http://localhost:30000";

type AuthResponse = {
  email: string;
  key: string;
};

async function validate_response(
  response: Response,
  setToken: (token: string | null) => void
): Promise<Response> {
  if (response.ok) {
    return response;
  } else if (
    response.status === StatusCodes.UNAUTHORIZED ||
    response.status === StatusCodes.FORBIDDEN
  ) {
    console.log(
      "(ServerManager.validate_response) Clearing token: " +
        getReasonPhrase(response.status)
    );
    setToken(null);
    throw new NetworkError(
      response.status,
      "Authorization error received from server."
    );
  } else {
    return response.json().then((json) => {
      if (response.status === StatusCodes.BAD_REQUEST) {
        console.log(
          "(ServerManager.validate_response) Response not ok: " +
            JSON.stringify(json)
        );
        throw new ApplicationError(json.msg);
      } else {
        throw new NetworkError(response.status, json.msg);
      }
    });
  }
}

function handle_network_error(
  error: NetworkError,
  setToken: (token: string | null) => void
) {
  setToken(null);
  console.error("(ServerManager.handle_network_error) Network Error: " + error);
  alert(`Network Error: (Status ${error.status}) ${error.message}`);
}

class NetworkError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
    this.name = "NetworkError";
  }
}

class ApplicationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "ApplicationError";
  }
}

export function login(
  code: string,
  setEmail: (email: string) => void,
  setToken: (token: string | null) => void
) {
  fetch(`${CALLISTO_BACKEND}/login`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
    },
    body: JSON.stringify({ code: code }),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.text())
    .then((body) => JSON.parse(body) as AuthResponse)
    .then((authResponse: AuthResponse) => {
      setEmail(authResponse.email);
      setToken(authResponse.key);
    })
    .catch((error) => handle_network_error(error, setToken));
}

export function addShip(
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  acceleration: [number, number, number],
  design: string,
  crew: Crew,
  callBack: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
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
    crew: crew,
  };

  fetch(`${CALLISTO_BACKEND}/add_ship`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then(() => getEntities(callBack, token, setToken))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function setAgility(
  target: string,
  thrust: number,
  callBack: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
) {
  fetch(`${CALLISTO_BACKEND}/set_agility`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify({ ship_name: target, thrust: thrust }),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then(() => getEntities(callBack, token, setToken))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function removeEntity(
  target: string,
  callBack: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
) {
  fetch(`${CALLISTO_BACKEND}/remove`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify(target),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then(() => getEntities(callBack, token, setToken))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export async function setPlan(
  target: string,
  plan: [Acceleration, Acceleration | null],
  callBack: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
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

  fetch(`${CALLISTO_BACKEND}/set_plan`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then(() => {
      getEntities(callBack, token, setToken);
    })
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function nextRound(
  fireActions: FireActionMsg,
  setEvents: (events: Effect[] | null) => void,
  callBack: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
) {
  fetch(`${CALLISTO_BACKEND}/update`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    body: JSON.stringify(Object.entries(fireActions)),
    mode: "cors",
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then((events) => setEvents(events))
    .then(() => getEntities(callBack, token, setToken))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  setProposedPlan: (plan: FlightPathResult | null) => void,
  target_vel: [number, number, number] | null = null,
  standoff: number | null = null,
  token: string,
  setToken: (token: string | null) => void
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

  fetch(`${CALLISTO_BACKEND}/compute_path`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then((plan) => setProposedPlan(plan))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function launchMissile(
  source: string,
  target: string,
  callback: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
) {
  let payload = {
    source: source,
    target: target,
  };

  fetch(`${CALLISTO_BACKEND}/launch_missile`, {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      Authorization: token,
    },
    mode: "cors",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then(() => getEntities(callback, token, setToken))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function getEntities(
  callback: EntityRefreshCallback,
  token: string,
  setToken: (token: string | null) => void
) {
  return fetch(`${CALLISTO_BACKEND}/entities`, {
    headers: {
      Authorization: token,
    },
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then((json) => EntityList.parse(json))
    .then((entities) => {
      console.log(`Received Entities: ${JSON.stringify(entities)}`);
      callback(entities);
    })
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export async function getTemplates(
  callBack: (templates: ShipDesignTemplates) => void,
  token: string,
  setToken: (token: string | null) => void
) {
  return fetch(`${CALLISTO_BACKEND}/designs`, {
    headers: {
      Authorization: token,
    },
  })
    .then((response) => validate_response(response, setToken))
    .then((response) => response.json())
    .then((json: any) => {
      let templates: { [key: string]: ShipDesignTemplate } = {};
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
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setToken);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

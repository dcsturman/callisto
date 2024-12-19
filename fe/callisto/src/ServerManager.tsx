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
};

// Standard headers for all fetch calls
const standard_headers: RequestInit = {
  method: "GET",
  credentials: "include",
  mode: "cors",
  headers: {
    "Content-Type": "application/json",
    "Access-Control-Allow-Credentials": "true",
  },
};

async function validate_response(
  response: Response,
  setAuthenticated: (authenticated: boolean) => void,
): Promise<Response> {
  if (response.ok) {
    return response;
  } else if (
    response.status === StatusCodes.UNAUTHORIZED ||
    response.status === StatusCodes.FORBIDDEN
  ) {
    console.log(
      "(ServerManager.validate_response) Setting as not authenticated " +
        getReasonPhrase(response.status)
    );
    setAuthenticated(false);
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
  setAuthenticated: (authenticated: boolean) => void,
) {
  setAuthenticated(false);
  console.group("(ServerManager.handle_network_error) Network Error");
  console.error("(ServerManager.handle_network_error) Network Error: " + error);
  console.error(error.stack);
  console.groupEnd();
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
  setAuthenticated: (authenticated: boolean) => void
) {
  let fetch_params = {
    ...standard_headers,
    method: "POST",
    body: JSON.stringify({ code: code }),
  };

  fetch(`${CALLISTO_BACKEND}/login`, fetch_params)
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.text())
    .then((body) => JSON.parse(body) as AuthResponse)
    .then((authResponse: AuthResponse) => {
      setEmail(authResponse.email);
      setAuthenticated(true);
    })
    .catch((error) => handle_network_error(error, setAuthenticated));
}

export function addShip(
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  acceleration: [number, number, number],
  design: string,
  crew: Crew,
  callBack: EntityRefreshCallback,
  setAuthenticated: (authenticated: boolean) => void
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
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then(() => getEntities(callBack, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function setCrewActions(
  target: string,
  dodge: number,
  assist_gunners: boolean,
  callBack: EntityRefreshCallback,
  setAuthenticated: (authenticated: boolean) => void
) {
  fetch(`${CALLISTO_BACKEND}/set_crew_actions`, {
    ...standard_headers,
    method: "POST",    
    body: JSON.stringify({ ship_name: target, dodge_thrust: dodge, assist_gunners: assist_gunners }),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then(() => getEntities(callBack, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
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
  setAuthenticated: (authenticated: boolean) => void
) {
  fetch(`${CALLISTO_BACKEND}/remove`, {
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(target),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then(() => getEntities(callBack, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
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
  setAuthenticated: (authenticated: boolean) => void
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
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then(() => {
      getEntities(callBack, setAuthenticated);
    })
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
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
  setAuthenticated: (authenticated: boolean) => void
) {
  fetch(`${CALLISTO_BACKEND}/update`, {
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(Object.entries(fireActions)),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then((events) => setEvents(events))
    .then(() => getEntities(callBack, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
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
  setAuthenticated: (authenticated: boolean) => void
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
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then((plan) => setProposedPlan(plan))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
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
  setAuthenticated: (authenticated: boolean) => void
) {
  let payload = {
    source: source,
    target: target,
  };

  fetch(`${CALLISTO_BACKEND}/launch_missile`, {
    ...standard_headers,
    method: "POST",
    body: JSON.stringify(payload),
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then(() => getEntities(callback, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function loadScenario(
  scenario_name: string,
  callback: EntityRefreshCallback,
  setAuthenticated: (authenticated: boolean) => void
) {
  fetch(`${CALLISTO_BACKEND}/load_scenario`, {
    ...standard_headers,
    method: "POST",
    body: JSON.stringify({ scenario_name: scenario_name }),
  })
    .then((response) => response.json())
    .then(() => getEntities(callback, setAuthenticated))
    .catch((error) => {
      if (error instanceof NetworkError) {
        handle_network_error(error, setAuthenticated);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export function getEntities(
  callback: EntityRefreshCallback,
  setAuthenticated: (authenticated: boolean) => void
) {
  return fetch(`${CALLISTO_BACKEND}/entities`, {
    ...standard_headers,
  })
    .then((response) => validate_response(response, setAuthenticated))
    .then((response) => response.json())
    .then((json) => EntityList.parse(json))
    .then((entities) => {
      console.log(`Received Entities: ${JSON.stringify(entities)}`);
      callback(entities);
    })
    .catch((error) => {
      if (error instanceof NetworkError || error instanceof TypeError) {
        // It seems that 401's get turned into TypeErrors vs a network error?
        if (error instanceof TypeError) {
          error = new NetworkError(0, error.message);
        }
        handle_network_error(error, setAuthenticated);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

export async function getTemplates(
  callBack: (templates: ShipDesignTemplates) => void,
  setAuthenticated: (authenticated: boolean) => void
) {
  return fetch(`${CALLISTO_BACKEND}/designs`, {
    ...standard_headers,
  })
    .then((response) => validate_response(response, setAuthenticated))
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
      if (error instanceof NetworkError || error instanceof TypeError) {
        if (error instanceof TypeError) {
          // It seems that 401's get turned into TypeErrors vs a network error?
          error = new NetworkError(0, error.message);
        }
        handle_network_error(error, setAuthenticated);
      } else if (error instanceof ApplicationError) {
        alert(error.message);
      } else {
        throw error;
      }
    });
}

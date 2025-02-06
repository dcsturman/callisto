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
import { Crew } from "./CrewBuilder";

export const CALLISTO_BACKEND =
  process.env.REACT_APP_C_BACKEND || "http://localhost:30000";

// Message structures
// This message (a simple enum on the rust server side) is just a string.
const DESIGN_TEMPLATE_REQUEST = "\"DesignTemplateRequest\"";
const ENTITIES_REQUEST = "\"EntitiesRequest\"";

// Define the (global) websocket
let socket = new WebSocket(CALLISTO_BACKEND);

// Message handlers, one for each type of incoming data we can receive.
let setEmail: (email: string) => void = () => {
  console.error("Calling default implementation of setEmail()");
};
let setAuthenticated: (authenticated: boolean) => void = () => {
  console.error("Calling default implementation of setAuthenticated()");
};
let setTemplates: (templates: ShipDesignTemplates) => void = () => {
  console.error("Calling default implementation of setTemplates()");
};
let setEntities: EntityRefreshCallback = () => {
  console.error("Calling default implementation of setEntities()");
};
let setFlightPath: (plan: FlightPathResult) => void = () => {
  console.error("Calling default implementation of setFlightPath()");
};
let setEffects: (effects: Effect[]) => void = () => {
  console.error("Calling default implementation of setEffects()");
};

// 
// Functions managing the socket connection
//

export function startWebsocket(setReady: (ready: boolean) => void) {
  console.log("Trying to establish websocket.");
  setReady(false);
  socket = new WebSocket(CALLISTO_BACKEND);
  socket.onopen = () => {
    console.log("Socket opened");
    setReady(true);
  };
  socket.onclose = handleClose;
  socket.onmessage = handleMessage;
}

export function socketReady() {
  return socket.readyState === WebSocket.OPEN;
}

export function setMessageHandlers(
  email: (email: string) => void,
  authenticated: (authenticated: boolean) => void,
  templates: (templates: ShipDesignTemplates) => void,
  entities: (entities: EntityList) => void,
  flightPath: (plan: FlightPathResult) => void,
  effects: (effects: Effect[]) => void,
) {
  setEmail = email;
  setAuthenticated = authenticated;
  setTemplates = templates;
  setEntities = entities;
  setFlightPath = flightPath;
  setEffects = effects;
}

const handleClose = (event: CloseEvent) => {
  const msg = "Socket closed: " + event.code + " Reason: " + event.reason;
  if (event.wasClean) {
    console.log(msg);
  } else {
    console.error(msg);
  }
};


const handleMessage = (event: MessageEvent) => {
  const json = JSON.parse(event.data);

  console.log("Received Message: " + JSON.stringify(json));

  // Because this isn't an object (just a string)  check for it differently.
  if (json === "PleaseLogin") {
    setAuthenticated(false);
    return;
  }
  if ("AuthResponse" in json) {
    const email = json.AuthResponse.email;
    setEmail(email);
    setAuthenticated(true);
    return;
  }

  if ("DesignTemplateResponse" in json) {
    const response = json.DesignTemplateResponse;
    handleTemplates(response, setTemplates);
    return;
  }

  if ("EntityResponse" in json) {
    const response = json.EntityResponse;
    handleEntities(response, setEntities);
    return;
  }

  if ("FlightPath" in json) {
    const response = json.FlightPath;
    handleFlightPath(response, setFlightPath);
    return;
  }

  if ("Effects" in json) {
    console.log("Received Effects");
    const response = json.Effects;
    handleEffect(response, setEffects);
    return;
  }

  if ("LaunchMissile" in json) {
    console.error(
      "LaunchMissile currently deprecated. Should never receive this message."
    );
  }

  if ("SimpleMsg" in json) {
    // Mostly ignore these except for debugging.  It tells us we didn't get an error.
    return;
  }


  if ("Error" in json) {
    console.error("Received Error: " + json.Error);
  }
};


// 
// Functions called to communicate to the server
//
export function login(code: string) {
  const payload = { Login: { code: code } };
  socket.send(JSON.stringify(payload));
}

export function addShip(
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  acceleration: [number, number, number],
  design: string,
  crew: Crew
) {
  console.log(
    `Adding Ship ${name}: Position ${position}, Velocity ${velocity}, Acceleration ${acceleration}`
  );

  const payload = {
    AddShip: {
      name: name,
      position: position,
      velocity: velocity,
      acceleration: acceleration,
      design: design,
      crew: crew,
    },
  };

  socket.send(JSON.stringify(payload));
}

export function setCrewActions(
  target: string,
  dodge: number,
  assist_gunners: boolean
) {
  const payload = {
    SetCrewActions: {
      ship_name: target,
      dodge_thrust: dodge,
      assist_gunners: assist_gunners,
    },
  };

  socket.send(JSON.stringify(payload));
}

export function removeEntity(target: string) {
  const payload = {
    RemoveEntity: {
      name: target,
    },
  };

  socket.send(JSON.stringify(payload));
}

export async function setPlan(
  target: string,
  plan: [Acceleration, Acceleration | null]
) {
  let plan_arr = [];

  // Since the Rust backend just expects null values in flight plans to be skipped
  // we have to custom build the body.
  if (plan[1] == null) {
    plan_arr = [plan[0]];
  } else {
    plan_arr = [plan[0], plan[1]];
  }
  const payload = { SetPlan: { name: target, plan: plan_arr } };

  socket.send(JSON.stringify(payload));
}

export function nextRound(fireActions: FireActionMsg) {
  const payload = { Update: Object.entries(fireActions) };
  socket.send(JSON.stringify(payload));
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
  const payload = {
    ComputePath: {
      entity_name: entity_name,
      end_pos: end_pos,
      end_vel: end_vel,
      target_velocity: target_vel,
      standoff_distance: standoff,
    },
  };

  socket.send(
    JSON.stringify(payload, (key, value) => {
      if (value !== null) {
        return value;
      }
    })
  );
}

export function loadScenario(scenario_name: string) {
  const payload = { LoadScenario: { scenario_name: scenario_name } };
  socket.send(JSON.stringify(payload));
}

export function getEntities() {
  socket.send(ENTITIES_REQUEST);
}

export function getTemplates() {
  socket.send(DESIGN_TEMPLATE_REQUEST);
}

//
// Functions to handle incoming messages that are more complex than a few lines.
//
function handleTemplates(
  json: object,
  setTemplates: (templates: ShipDesignTemplates) => void
) {
  const templates: { [key: string]: ShipDesignTemplate } = {};

  // First coerce the free-form json we receive into a formal templates object
  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  Object.entries(json).forEach((entry: [string, any]) => {
    const currentTemplate: ShipDesignTemplate = ShipDesignTemplate.parse(
      entry[1]
    );
    templates[entry[0]] = currentTemplate;
  });

  // Output all the templates to the console.
  console.log("Received Templates: ");
  for (const v of Object.values(templates)) {
    console.log(` ${v.name}`);
  }

  setTemplates(templates);
}

function handleEntities(
  json: object,
  setEntities: (entities: EntityList) => void
) {
  const entities = EntityList.parse(json);
  setEntities(entities);
}

function handleFlightPath(
  json: object,
  setProposedPlan: (plan: FlightPathResult) => void
) {
  const path = FlightPathResult.parse(json);
  setProposedPlan(path);
}

function handleEffect(json: object[], setEvents: (effects: Effect[]) => void) {
  const effects = json.map((effect: object) => Effect.parse(effect));
  setEvents(effects);
}

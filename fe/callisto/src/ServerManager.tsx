import {
  Acceleration,
  EntityRefreshCallback,
  EntityList,
  FlightPathResult,
  G,
  Ship,
  ShipDesignTemplates,
  ShipDesignTemplate,
  ViewMode,
} from "./Universal";
import {Effect} from "./Effects";
import {UserList, UserContext} from "./UserList";
import {ActionType, actionPayload, payloadToAction} from "./Actions";

export const CALLISTO_BACKEND = process.env.REACT_APP_CALLISTO_BACKEND || "http://localhost:30000";

// Message structures
// This message (a simple enum on the rust server side) is just a string.
const DESIGN_TEMPLATE_REQUEST = '"DesignTemplateRequest"';
const ENTITIES_REQUEST = '"EntitiesRequest"';
const UPDATE_REQUEST = "Update";
const LOGOUT_REQUEST = '"Logout"';

// Define the (global) websocket
export let socket: WebSocket;

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
let setActions: (actions: ActionType) => void = () => {
  console.error("Calling default implementation of setActions()");
};
let setFlightPath: (plan: FlightPathResult) => void = () => {
  console.error("Calling default implementation of setFlightPath()");
};
let setEffects: (effects: Effect[]) => void = () => {
  console.error("Calling default implementation of setEffects()");
};
let setUsers: (users: UserList) => void = () => {
  console.error("Calling default implementation of setUsers()");
};

//
// Functions managing the socket connection
//

export function startWebsocket(setReady: (ready: boolean) => void) {
  console.log("(ServerManager.startWebsocket) Trying to establish websocket.");
  const stripped_name = CALLISTO_BACKEND.replace("https://", "").replace("http://", "");

  if (socket === undefined || socket.readyState === WebSocket.CLOSED) {
    setReady(false);
    const back_end = `wss://${stripped_name}`;
    console.log(`(ServerManager.startWebsocket) Open web socket to ${back_end}`);
    socket = new WebSocket(back_end);
  } else {
    console.log("Socket already defined.  Not building it.");
  }
  socket.onopen = () => {
    console.log("(ServerManager.startWebsocket.onopen) Socket opened");
    setReady(true);
  };
  socket.onclose = (event: CloseEvent) => {
    console.log("(ServerManager.startWebsocket.onclose) Socket closed");
    setReady(false);
    handleClose(event);
  };
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
  actions: (actions: ActionType) => void,
  flightPath: (plan: FlightPathResult) => void,
  effects: (effects: Effect[]) => void,
  users: (users: UserList) => void
) {
  if (email) {
    setEmail = email;
  }
  if (authenticated) {
    setAuthenticated = authenticated;
  }
  if (templates) {
    setTemplates = templates;
  }
  if (entities) {
    setEntities = entities;
  }
  if (actions) {
    setActions = actions;
  }
  if (flightPath) {
    setFlightPath = flightPath;
  }
  if (effects) {
    setEffects = effects;
  }
  if (users) {
    setUsers = users;
  }
}

const handleClose = (event: CloseEvent) => {
  const msg =
    "(ServerManager.handleClose) Socket closed: " + event.code + " Reason: " + event.reason;
  if (event.wasClean) {
    console.log(msg);
  } else {
    console.error(msg);
  }
};

const handleMessage = (event: MessageEvent) => {
  const json = JSON.parse(event.data);

  // Because this isn't an object (just a string)  check for it differently.
  if (json === "PleaseLogin") {
    setAuthenticated(false);
    return;
  }
  if ("AuthResponse" in json) {
    const email = json.AuthResponse.email;
    if (email !== null) {
      setEmail(email);
      setAuthenticated(true);
    } else {
      console.log("(ServerManager) Authenticated with null email. Ignoring.");
      setAuthenticated(false);
    }
    return;
  }

  if ("DesignTemplateResponse" in json) {
    const response = json.DesignTemplateResponse;
    handleTemplates(response, setTemplates);
    return;
  }

  if ("EntityResponse" in json) {
    const response = json.EntityResponse;
    handleEntities(response, setEntities, setActions);
    return;
  }

  if ("FlightPath" in json) {
    const response = json.FlightPath;
    handleFlightPath(response, setFlightPath);
    return;
  }

  if ("Effects" in json) {
    const response = json.Effects;
    handleEffect(response, setEffects);
    return;
  }

  if ("Users" in json) {
    const response = json.Users;
    handleUsers(response, setUsers);
    return;
  }

  if ("LaunchMissile" in json) {
    console.error("LaunchMissile currently deprecated. Should never receive this message.");
  }

  if ("SimpleMsg" in json) {
    // Mostly ignore these except for debugging.  It tells us we didn't get an error.
    return;
  }

  if ("Error" in json) {
    console.error("Received Error: " + json.Error);
    alert(json.Error);
  }
};

//
// Functions called to communicate to the server
//
export function login(code: string) {
  const payload = {Login: {code: code}};
  socket.send(JSON.stringify(payload));
}

export function addShip(ship: Ship) {
  console.log(`Adding Ship ${ship.name}: Position ${ship.position}, Velocity ${ship.velocity}`);

  const payload = {
    AddShip: {
      name: ship.name,
      position: ship.position,
      velocity: ship.velocity,
      design: ship.design,
      crew: ship.crew,
    },
  };

  socket.send(JSON.stringify(payload));
}

export function setCrewActions(target: string, dodge: number, assist_gunners: boolean) {
  const payload = {
    SetPilotActions: {
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

export async function setPlan(target: string, plan: [Acceleration, Acceleration | null]) {
  let plan_arr = [];

  // Since the Rust backend just expects null values in flight plans to be skipped
  // we have to custom build the body.
  if (plan[1] == null) {
    plan_arr = [plan[0]];
  } else {
    plan_arr = [plan[0], plan[1]];
  }
  const payload = {SetPlan: {name: target, plan: plan_arr}};

  socket.send(JSON.stringify(payload));
}

export function updateActions(actions: ActionType) {
  if (Object.entries(actions).length === 0) {
    return;
  }
  console.group("(updateActions) Sending actions: ");
  console.log(JSON.stringify(actions));
  console.groupEnd();
  const payload = {ModifyActions: actionPayload(actions)};
  socket.send(JSON.stringify(payload));
}

export function nextRound() {
  const payload = UPDATE_REQUEST;
  socket.send(JSON.stringify(payload));
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  setProposedPlan: (plan: FlightPathResult | null) => void,
  target_vel: [number, number, number] | null = null,
  target_accel: [number, number, number] | null = null,
  standoff: number | null = null
) {
  if (entity_name == null) {
    setProposedPlan(null);
    return;
  }

  // If there is a target acceleration, convert it to m/s^2 from G's
  if (target_accel != null) {
    target_accel = [target_accel[0] * G, target_accel[1] * G, target_accel[2] * G];
  }

  const payload = {
    ComputePath: {
      entity_name: entity_name,
      end_pos: end_pos,
      end_vel: end_vel,
      target_velocity: target_vel,
      target_acceleration: target_accel,
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
  const payload = {LoadScenario: {scenario_name: scenario_name}};
  socket.send(JSON.stringify(payload));
}

export function setRole(role: ViewMode, ship: string | null) {
  if (ship !== null) {
    const payload = {SetRole: {role: ViewMode[role], ship: ship}};
    socket.send(JSON.stringify(payload));
  } else {
    const payload = {SetRole: {role: ViewMode[role]}};
    socket.send(JSON.stringify(payload));
  }
}

export function getEntities() {
  socket.send(ENTITIES_REQUEST);
}

export function getTemplates() {
  socket.send(DESIGN_TEMPLATE_REQUEST);
}

export function logout() {
  socket.send(LOGOUT_REQUEST);
}

//
// Functions to handle incoming messages that are more complex than a few lines.
//
function handleTemplates(json: object, setTemplates: (templates: ShipDesignTemplates) => void) {
  const templates: {[key: string]: ShipDesignTemplate} = {};

  // First coerce the free-form json we receive into a formal templates object
  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  Object.entries(json).forEach((entry: [string, any]) => {
    const currentTemplate: ShipDesignTemplate = ShipDesignTemplate.parse(entry[1]);
    templates[entry[0]] = currentTemplate;
  });

  // Output all the templates to the console.
  console.groupCollapsed("Received Templates: ");
  for (const v of Object.values(templates)) {
    console.log(` ${v.name}`);
  }
  console.groupEnd();
  setTemplates(templates);
}

function handleEntities(
  json: object,
  setEntities: (entities: EntityList) => void,
  setActions: (actions: ActionType) => void
) {
  const entities = EntityList.parse(json);
  console.groupCollapsed("Received Entities: ");
  console.groupCollapsed("Ships: ");
  for (const v of entities.ships) {
    console.log(` ${JSON.stringify(v)}`);
  }
  console.groupEnd();
  console.groupCollapsed("Missiles: ");
  for (const v of entities.missiles) {
    console.log(` ${JSON.stringify(v)}`);
  }
  console.groupEnd();
  console.groupCollapsed("Planets: ");
  for (const v of entities.planets) {
    console.log(` ${JSON.stringify(v)}`);
  }
  console.groupEnd();
  console.groupEnd();
  setEntities(entities);
  if (Object.hasOwn(json, "actions")) {
    const actions = (json as {actions: object[]}).actions;
    const parsed_actions = payloadToAction(actions);
    setActions(parsed_actions);

    console.groupCollapsed("Received Actions: ");
    console.log(JSON.stringify(actions));
    console.groupEnd();
  } else {
    console.log(JSON.stringify(json));
    console.groupEnd();
    setActions({});
  }
}

function handleFlightPath(json: object, setProposedPlan: (plan: FlightPathResult) => void) {
  const path = FlightPathResult.parse(json);
  setProposedPlan(path);
}

function handleEffect(json: object[], setEvents: (effects: Effect[]) => void) {
  const effects = json.map((effect: object) => Effect.parse(effect));
  setEvents(effects);
}

function handleUsers(json: [UserContext], setUsers: (users: UserList) => void) {
  console.log("(handleUsers) Received users: " + JSON.stringify(json));
  const users: UserList = [];
  for (const user of json) {
    const c: UserContext = {} as UserContext;
    c.email = user.email;
    switch (user.role as unknown as string) {
      case "General":
        c.role = ViewMode.General as ViewMode;
        break;
      case "Pilot":
        c.role = ViewMode.Pilot as ViewMode;
        break;
      case "Sensors":
        c.role = ViewMode.Sensors as ViewMode;
        break;
      case "Gunner":
        c.role = ViewMode.Gunner as ViewMode;
        break;
      case "Observer":
        c.role = ViewMode.Observer as ViewMode;
        break;
    }
    c.ship = user.ship;
    users.push(c);
  }
  setUsers(users);
}

import * as Sentry from "@sentry/react";
import { Event } from "components/space/Effects";
import { UserList, UserContext } from "components/UserList";
import {
  ActionType,
  actionPayload,
  payloadToAction,
} from "components/controls/Actions";
import {
  SCENARIO_BUILDER_PREFIX,
  TUTORIAL_PREFIX,
} from "components/scenarios/ScenarioManager";
import {
  setSocketReady,
  setAuthenticated,
  setTemplates,
  setEntities,
  setUsers,
  setScenarios,
} from "state/serverSlice";
import { setEvents, setProposedPlan, setShowResults } from "state/uiSlice";
import { setEmail, setRoleShip, setJoinedScenario } from "state/userSlice";
import { AppMode, setAppMode } from "state/tutorialSlice";
import { setActions } from "state/actionsSlice";
import { store } from "state/store";
import { G } from "lib/universal";
import {
  EntityList,
  Ship,
  Planet,
  MetaData,
  EngineerActionResult,
} from "lib/entities";
import { ViewMode, stringToViewMode } from "lib/view";
import { Acceleration } from "lib/entities";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";
import { FlightPath } from "lib/flightPath";
import { resetState as resetServerState } from "state/store";

export const CALLISTO_BACKEND =
  import.meta.env.VITE_CALLISTO_BACKEND || "http://localhost:30000";

// Message structures
// This message (a simple enum on the rust server side) is just a string.
const DESIGN_TEMPLATE_REQUEST = '"DesignTemplateRequest"';
const ENTITIES_REQUEST = '"EntitiesRequest"';
const UPDATE_REQUEST = "Update";
const RESET_REQUEST = '"Reset"';
const EXIT_REQUEST = '"Exit"';
const LOGOUT_REQUEST = '"Logout"';
const PING_REQUEST = '"Ping"';
const VALIDATE_SESSION_REQUEST = '"ValidateSession"';

// Send a keepalive every minute.
const KEEP_ALIVE_INTERVAL = 60000;

// Define the (global) websocket
export let socket: WebSocket;

// Result type for saveScenario(). On success the server returns the filename
// it wrote (with `.json` extension). On failure the raw error string is
// returned — the caller pattern-matches on "SCENARIO_EXISTS" or
// "NOT_OWNER:<email>" prefixes to drive the confirm/error UX.
export type SaveScenarioResult =
  | { ok: true; filename: string }
  | { ok: false; error: string };

// At most one save can be in flight per session; the dialog is modal.
let pendingSaveCallback: ((result: SaveScenarioResult) => void) | null = null;

//
// Functions managing the socket connection
//
export function startWebsocket() {
  console.log("(ServerManager.startWebsocket) Trying to establish websocket.");
  const stripped_name = CALLISTO_BACKEND.replace("https://", "").replace(
    "http://",
    "",
  );

  // Use ws:// for http:// backends (local dev), wss:// for https:// backends (production)
  const protocol = CALLISTO_BACKEND.startsWith("https://") ? "wss://" : "ws://";

  if (socket === undefined || socket.readyState === WebSocket.CLOSED) {
    store.dispatch(setSocketReady(false));
    const back_end = `${protocol}${stripped_name}`;
    console.log(
      `(ServerManager.startWebsocket) Open web socket to ${back_end}`,
    );
    socket = new WebSocket(back_end);
  } else {
    console.log("Socket already defined.  Not building it.");
  }
  socket.onopen = () => {
    console.log("(ServerManager.startWebsocket.onopen) Socket opened");
    store.dispatch(setSocketReady(true));
    // Validate session on reconnect - server will respond with AuthResponse or PleaseLogin
    socket.send(VALIDATE_SESSION_REQUEST);
  };
  socket.onclose = (event: CloseEvent) => {
    console.log("(ServerManager.startWebsocket.onclose) Socket closed");
    store.dispatch(setSocketReady(false));
    handleClose(event);
  };
  socket.onerror = (event) => {
    console.error("(ServerManager.startWebsocket.onerror) Socket error", event);
    Sentry.captureException(new Error("WebSocket error"), {
      tags: { component: "serverManager", phase: "websocket-error" },
      extra: {
        backend: CALLISTO_BACKEND,
        readyState: socket?.readyState,
      },
    });
  };
  socket.onmessage = handleMessage;
}

export function socketReady() {
  return socket.readyState === WebSocket.OPEN;
}

//
export function setUpKeepAlive() {
  // Send a keepalive every KEEP_ALIVE_INTERVAL milliseconds.
  setInterval(() => {
    if (socket.readyState === WebSocket.OPEN) {
      socket.send(PING_REQUEST);
    }
  }, KEEP_ALIVE_INTERVAL);
}

const handleClose = (event: CloseEvent) => {
  const msg =
    "(ServerManager.handleClose) Socket closed: " +
    event.code +
    " Reason: " +
    event.reason;
  if (event.wasClean) {
    console.log(msg);
  } else {
    console.error(msg);
    Sentry.captureMessage("WebSocket closed abnormally", {
      level: "error",
      tags: { component: "serverManager", phase: "websocket-close" },
      extra: {
        backend: CALLISTO_BACKEND,
        code: event.code,
        reason: event.reason,
        wasClean: event.wasClean,
      },
    });
  }
};

const handleMessage = (event: MessageEvent) => {
  let json;
  try {
    json = JSON.parse(event.data);
  } catch (err) {
    Sentry.captureException(err, {
      tags: { component: "serverManager", phase: "websocket-parse" },
      extra: { rawData: typeof event.data === "string" ? event.data.slice(0, 500) : "(non-string)" },
    });
    console.error("(ServerManager.handleMessage) Failed to parse:", event.data);
    return;
  }

  // Because these first two aren't an object (just a string)  check for it differently.
  // Response to keepalive message
  if (json === "Pong") {
    return;
  }

  if (json === "PleaseLogin") {
    store.dispatch(setAuthenticated(false));
    return;
  }

  // The remainder are all objects with keys.
  if ("AuthResponse" in json) {
    handleAuthenticated(json.AuthResponse);
    return;
  }

  if ("DesignTemplateResponse" in json) {
    const response = json.DesignTemplateResponse;
    handleTemplates(response);
    return;
  }

  if ("EntityResponse" in json) {
    const response = json.EntityResponse;
    handleEntities(response);
    return;
  }

  if ("FlightPath" in json) {
    const response = json.FlightPath;
    handleFlightPath(response);
    return;
  }

  if ("Effects" in json) {
    const response = json.Effects;
    handleEffect(response);
    return;
  }

  if ("Users" in json) {
    const response = json.Users;
    handleUsers(response);
    return;
  }

  if ("Scenarios" in json) {
    const response = json.Scenarios;
    handleScenarioList(response);
    return;
  }

  if ("JoinedScenario" in json) {
    handleJoinedScenario(json);
    return;
  }

  if ("LaunchMissile" in json) {
    console.error(
      "LaunchMissile currently deprecated. Should never receive this message.",
    );
  }

  if ("SimpleMsg" in json) {
    // Mostly ignore these except for debugging.  It tells us we didn't get an error.
    return;
  }

  if ("ScenarioSaved" in json) {
    const cb = pendingSaveCallback;
    pendingSaveCallback = null;
    if (cb) cb({ ok: true, filename: json.ScenarioSaved });
    return;
  }

  if ("CaptainActionResult" in json) {
    // Result fields are also reflected on the ship via the followup
    // EntityResponse (leadership_points + leadership_rolled), so the FE
    // derives its display from there. Nothing more to do here.
    return;
  }

  if ("Error" in json) {
    // If a save is in flight, route the error through its callback so the
    // dialog can handle SCENARIO_EXISTS / NOT_OWNER specifically. Otherwise
    // fall back to the legacy alert.
    if (pendingSaveCallback) {
      const cb = pendingSaveCallback;
      pendingSaveCallback = null;
      cb({ ok: false, error: json.Error });
      return;
    }
    console.error("Received Error: " + json.Error);
    alert(json.Error);
  }

};

//
// Functions called to communicate to the server
//
export function login(code: string) {
  const payload = { Login: { code: code } };
  socket.send(JSON.stringify(payload));
}

export function addShip(ship: Ship) {
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

interface AddPlanetMsg {
  name: string;
  position: [number, number, number];
  color: string;
  primary: string | null;
  radius: number;
  mass: number;
  visual_effects: Planet["visual_effects"];
}

export function addPlanet(planet: Planet) {
  const payload: { AddPlanet: AddPlanetMsg } = {
    AddPlanet: {
      name: planet.name,
      position: planet.position,
      color: planet.color,
      primary: planet.primary,
      radius: planet.radius,
      mass: planet.mass,
      visual_effects: planet.visual_effects,
    },
  };

  socket.send(JSON.stringify(payload));
}

export function setCrewActions(
  target: string,
  dodge: number,
  assist_gunners: boolean,
) {
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

export async function setPlan(
  target: string,
  plan: [Acceleration, Acceleration | null],
) {
  let plan_arr = [];

  // Since the Rust backend just expects null values in flight plans to be skipped
  // we have to custom build the body.
  // Convert all accelerations to m/s^2 from G's
  if (plan[1] == null) {
    plan_arr[0] = [
      [plan[0][0][0] * G, plan[0][0][1] * G, plan[0][0][2] * G],
      plan[0][1],
    ];
  } else {
    plan_arr = [
      [[plan[0][0][0] * G, plan[0][0][1] * G, plan[0][0][2] * G], plan[0][1]],
      [[plan[1][0][0] * G, plan[1][0][1] * G, plan[1][0][2] * G], plan[1][1]],
    ];
  }
  const payload = { SetPlan: { name: target, plan: plan_arr } };

  socket.send(JSON.stringify(payload));
}

export function updateActions(actions: ActionType) {
  if (Object.entries(actions).length === 0) {
    return;
  }
  //console.group("(updateActions) Sending actions: ");
  //console.log(JSON.stringify(actions));
  //console.groupEnd();

  const payload = { ModifyActions: actionPayload(actions) };
  socket.send(JSON.stringify(payload));
}

export function nextRound() {
  // Flush any locally-held boost state to the server before ending the round.
  // Boost toggles don't round-trip on every click anymore (see
  // actionsSlice.toggleBoost), so the captain's pending list is in Redux
  // only — pin it to the server here.
  updateActions(store.getState().actions);
  const payload = UPDATE_REQUEST;
  socket.send(JSON.stringify(payload));
}

// Captain hits the "Captain Action" button. Server rolls leadership 2d6 +
// leadership − 8 immediately and replies with `CaptainActionResult`. The
// result is also stored on the captain's ship as `leadership_points` until
// end-of-turn Phase 0 consumes it.
export function captainAction(shipName: string) {
  // Pin the captain's intended boost list to the server at roll time so the
  // server has the right LeadershipCheck queued when the rolled
  // `leadership_points` is consumed at end-of-turn.
  updateActions(store.getState().actions);
  const payload = { CaptainAction: { ship_name: shipName } };
  socket.send(JSON.stringify(payload));
}

export function computeFlightPath(
  entity_name: string | null,
  end_pos: [number, number, number],
  end_vel: [number, number, number],
  target_vel: [number, number, number] | null = null,
  target_accel: [number, number, number] | null = null,
  standoff: number | null = null,
) {
  if (entity_name == null) {
    store.dispatch(setProposedPlan(null));
    return;
  }

  // If there is a target acceleration, convert it to m/s^2 from G's
  if (target_accel != null) {
    target_accel = [
      target_accel[0] * G,
      target_accel[1] * G,
      target_accel[2] * G,
    ];
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

  console.log(
    "(computeFlightPath) Sending flight path request: " +
      JSON.stringify(payload),
  );
  socket.send(
    JSON.stringify(payload, (key, value) => {
      if (value !== null) {
        return value;
      }
    }),
  );
}

export function requestRoleChoice(role: ViewMode, ship: string | null) {
  if (ship !== null) {
    const payload = { SetRole: { role: ViewMode[role], ship: ship } };
    socket.send(JSON.stringify(payload));
  } else {
    const payload = { SetRole: { role: ViewMode[role] } };
    socket.send(JSON.stringify(payload));
  }
}

export function joinScenario(scenario_name: string) {
  const payload = { JoinScenario: { scenario_name: scenario_name } };
  socket.send(JSON.stringify(payload));
}

export function createScenario(name: string, scenario: string) {
  const payload = { CreateScenario: { name: name, scenario: scenario } };
  socket.send(JSON.stringify(payload));
}

// Save the current scenario to disk / GCS. The callback fires when the server
// responds with ScenarioSaved or Error. Pass force_overwrite=true to confirm
// after a SCENARIO_EXISTS rejection.
//
// `name` is the on-disk filename (the server appends `.json` if missing).
// `displayName` lands in the file's `metadata.name` and is what the picker
// renders. They're separate concerns: a `planetfun.json` file can hold a
// "Fun with a planet" display name.
export function saveScenario(
  name: string,
  displayName: string,
  description: string,
  forceOverwrite: boolean,
  callback: (result: SaveScenarioResult) => void,
) {
  if (pendingSaveCallback) {
    callback({ ok: false, error: "A save is already in progress." });
    return;
  }
  pendingSaveCallback = callback;
  const payload = {
    SaveScenario: {
      name,
      display_name: displayName,
      description,
      force_overwrite: forceOverwrite,
    },
  };
  socket.send(JSON.stringify(payload));
}

export function getEntities() {
  socket.send(ENTITIES_REQUEST);
}

export function getTemplates() {
  socket.send(DESIGN_TEMPLATE_REQUEST);
}

export function resetServer(appMode: AppMode) {
  if (window.confirm("Are you sure you want to reset the server?")) {
    store.dispatch(resetServerState());
    store.dispatch(setAppMode(appMode));
    socket.send(RESET_REQUEST);
  }
}

export function exit_scenario() {
  socket.send(EXIT_REQUEST);
}

export function logout() {
  socket.send(LOGOUT_REQUEST);
}

//
// Functions to handle incoming messages that are more complex than a few lines.
//
function handleTemplates(json: object) {
  const templates = json as ShipDesignTemplates;

  // Output all the templates to the console.
  console.groupCollapsed("Received Templates: ");
  for (const v of Object.values(templates)) {
    console.log(` ${v.name}`);
  }
  console.groupEnd();
  store.dispatch(setTemplates(templates));
}

function handleEntities(json: object) {
  const entities = json as EntityList;

  // Convert all ship plans to G's from m/s^2
  entities.ships.forEach((ship) => {
    ship.plan[0][0] = [
      ship.plan[0][0][0] / G,
      ship.plan[0][0][1] / G,
      ship.plan[0][0][2] / G,
    ];
    if (ship.plan[1] != null) {
      ship.plan[1][0] = [
        ship.plan[1][0][0] / G,
        ship.plan[1][0][1] / G,
        ship.plan[1][0][2] / G,
      ];
    }
  });

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
  store.dispatch(setEntities(entities));
  // The captain's local leadership boost list is held in Redux only between
  // explicit flushes; thread their shipName into setActions so the reducer
  // can preserve it across this server-driven overwrite. Also pass
  // `leadership_rolled` so the reducer can drop the local list at end-of-turn
  // (when the server resets that flag back to false).
  const captainShipName = store.getState().user.shipName ?? null;
  const captainShip = captainShipName
    ? entities.ships.find((s) => s.name === captainShipName)
    : null;
  const captainLeadershipRolled = captainShip?.leadership_rolled ?? false;
  if (Object.hasOwn(json, "actions")) {
    const actions = (json as { actions: object[] }).actions;
    const parsed_actions = payloadToAction(actions);
    store.dispatch(
      setActions({
        parsed: parsed_actions,
        captainShipName,
        captainLeadershipRolled,
      })
    );

    console.groupCollapsed("Received Actions: ");
    console.log(JSON.stringify(actions));
    console.groupEnd();
  } else {
    console.log(JSON.stringify(json));
    console.groupEnd();
    store.dispatch(
      setActions({
        parsed: {} as ActionType,
        captainShipName,
        captainLeadershipRolled,
      })
    );
  }
}

function handleFlightPath(json: object) {
  const path = json as FlightPath;

  // Convert all accelerations in FlightPath from m/s^2 to G's
  path.plan[0][0] = [
    path.plan[0][0][0] / G,
    path.plan[0][0][1] / G,
    path.plan[0][0][2] / G,
  ];
  if (path.plan[1] != null) {
    path.plan[1][0] = [
      path.plan[1][0][0] / G,
      path.plan[1][0][1] / G,
      path.plan[1][0][2] / G,
    ];
  }

  store.dispatch(setProposedPlan(path));
}

function handleEffect(json: object[]) {
  console.groupCollapsed("Received Effects: ");
  console.log("(handleEffect) Received effects: " + JSON.stringify(json));
  console.groupEnd();

  // Engineer-action effects ride the same Effects channel as combat / sensor
  // effects but the existing UI expects each event to be one of the visual
  // kinds (ShipImpact / ExhaustedMissile / ShipDestroyed / BeamHit / Message).
  // Translate `{kind:"EngineerAction", result:{...}}` into a Message-style
  // event so it surfaces in the existing ResultsWindow alongside everything
  // else from the same turn.
  const events: Event[] = (
    json as Array<Event | EngineerActionEffect | LeadershipActionEffect>
  ).map((event) => {
    if ((event as EngineerActionEffect).kind === "EngineerAction") {
      const result = (event as EngineerActionEffect).result;
      return {
        kind: "Message",
        content: formatEngineerResult(result),
        position: null,
        target: null,
        origin: null,
      } as Event;
    }
    if ((event as LeadershipActionEffect).kind === "LeadershipAction") {
      const lead = event as LeadershipActionEffect;
      return {
        kind: "Message",
        content: formatLeadershipResult(lead),
        position: null,
        target: null,
        origin: null,
      } as Event;
    }
    return event as Event;
  });

  store.dispatch(setEvents(events));
  store.dispatch(setShowResults(true));
}

interface EngineerActionEffect {
  kind: "EngineerAction";
  result: EngineerActionResult;
}

function formatEngineerResult(result: EngineerActionResult): string {
  const outcome = result.critical_failure
    ? "CRITICAL FAILURE"
    : result.success
      ? "SUCCESS"
      : "FAILURE";
  return `[Engineer ${outcome}] ${result.message} (Check ${result.check} vs ${result.target})`;
}

interface LeadershipActionEffect {
  kind: "LeadershipAction";
  ship_name: string;
  points: number;
  boosts_applied: object[];
}

function formatLeadershipResult(lead: LeadershipActionEffect): string {
  const summary =
    lead.boosts_applied.length === 0
      ? "no boosts"
      : `${lead.boosts_applied.length} boost(s): ${lead.boosts_applied
          .map((b) => describeBoost(b))
          .join(", ")}`;
  return `[Captain] ${lead.ship_name} rolled ${lead.points} leadership point(s); ${summary}.`;
}

function describeBoost(b: object): string {
  // Wire form is `{Fire: {ship, weapon_id}}` etc. Convert to a readable
  // "Kind on ship[/weapon]" string.
  const obj = b as Record<string, { ship: string; weapon_id?: number }>;
  const kind = Object.keys(obj)[0];
  const v = obj[kind];
  const wid = v.weapon_id;
  return wid !== undefined
    ? `${kind} ${v.ship}#${wid}`
    : `${kind} ${v.ship}`;
}

function handleUsers(json: [UserContext]) {
  const users: UserList = [];
  for (const user of json) {
    const c: UserContext = {} as UserContext;
    c.email = user.email;
    c.role =
      stringToViewMode(user.role as unknown as string) ?? ViewMode.General;
    c.ship = user.ship;
    users.push(c);
  }
  store.dispatch(setUsers(users));
}

function handleScenarioList(json: {
  current_scenarios: [string, string][];
  templates: [string, MetaData][];
}) {
  store.dispatch(setScenarios([json.current_scenarios, json.templates]));
}

function handleJoinedScenario(json: { JoinedScenario: string }) {
  const scenario = json["JoinedScenario"] as string;
  if (scenario) {
    store.dispatch(setJoinedScenario(scenario));
    syncAppModeForScenario(scenario);
  }
}

function syncAppModeForScenario(scenario: string) {
  if (scenario.startsWith(TUTORIAL_PREFIX)) {
    store.dispatch(setAppMode(AppMode.Tutorial));
    return;
  }

  if (scenario.startsWith(SCENARIO_BUILDER_PREFIX)) {
    store.dispatch(setAppMode(AppMode.ScenarioBuilder));
    return;
  }

  if (store.getState().tutorial.appMode !== AppMode.ScenarioBuilder) {
    store.dispatch(setAppMode(AppMode.Game));
  }
}

function handleAuthenticated(json: {
  email: string | null;
  scenario: string | null;
  role: string | null;
  ship: string | null;
}): void {
  console.log(
    "(handleAuthenticated) Received Authenticated: " + JSON.stringify(json),
  );
  if (json.email != null) {
    store.dispatch(setEmail(json.email));
    store.dispatch(setAuthenticated(true));
    if (json.scenario != null) {
      store.dispatch(setJoinedScenario(json.scenario));
      syncAppModeForScenario(json.scenario);
    }
    if (json.role != null) {
      store.dispatch(
        setRoleShip([
          stringToViewMode(json.role) ?? ViewMode.General,
          json.ship,
        ]),
      );
    }
  } else {
    store.dispatch(setAuthenticated(false));
  }
}

export type ActionType = {
  [actor: string]: {
    /**
     * One slot per sensor operator, by their place in the crew. A ship with
     * two operators can jam with one and lock with the other, so this is a
     * list rather than the single action it used to be.
     */
    sensors: SensorState[];
    fire: FireState;
    unfire: UnfireState;
    pointDefense: PointDefenseState;
    /** One slot per engineer, on the same terms. */
    engineers: EngineerState[];
    /**
     * Systems the ship's computer is repairing itself through its drones.
     * One Auto-Repair point each, and nobody's action, so this is a list
     * rather than a crew slot.
     */
    computerRepairs?: ComputerRepairState;
    // Captain leadership state. `null` means no LeadershipCheck queued for this
    // ship. When non-null, `boosts` is the set of action targets to boost.
    // The actual roll happens server-side when the captain hits the "Captain
    // Action" button (cached on `ship.leadership_points`); end-of-turn Phase 0
    // truncates this list to the rolled N.
    leadershipCheck: { boosts: BoostTarget[] } | null;
    // Transient anti-actions. `clearSensors` and `clearEngineers` hold the
    // crew positions whose queued action the user explicitly cancelled, and
    // are emitted as `ClearSensorAction` / `ClearEngineerAction` naming that
    // person, so the server strips theirs and leaves their shipmates' alone.
    // Cleared again on any subsequent set.
    clearSensors: number[];
    clearEngineers: number[];
    clearLeadership: boolean;
    /**
     * Boost toggles this browser has made and not yet seen come back.
     *
     * The server is the truth about who is inspiring what -- two consoles on
     * the same ship must agree -- but a tick is in Redux before it is on the
     * wire, so a snapshot answering an earlier request would undo it. Each
     * entry says what was toggled and which way, is laid over whatever the
     * server last said, and retires the moment the server agrees.
     */
    pendingBoosts?: {target: BoostTarget; on: boolean; at: number}[];
  };
};

// Wire-form mirror of Rust `BoostTarget`. Field names match the Rust
// `#[serde]` defaults so encode/decode is structural identity. Jump is an
// engineer action, so it's boosted via `Engineer`.
export type BoostTarget =
  | { kind: "Fire"; ship: string; weapon_id: number }
  | { kind: "PointDefense"; ship: string; weapon_id: number }
  | { kind: "Sensor"; ship: string; operator: number }
  // Backed by no queued action: detection is free and happens every round. The
  // boost names the pair, because a captain concentrates the sensop on finding
  // one particular ship rather than on sensors in general.
  | { kind: "Detection"; ship: string; target: string }
  | { kind: "Engineer"; ship: string; engineer: number }
  | { kind: "Evade"; ship: string }
  | { kind: "AssistGunner"; ship: string };

import {PowerSystem} from "lib/power";
import {ShipSystem} from "lib/entities";

// All the different action types.
export type FireAction = {
  target: string;
  weapon_id: number;
  called_shot_system: string | null;
  /**
   * Which weapon type in the mount is firing.
   *
   * A mixed turret may only use one type per round (Core Rulebook p. 166), so
   * it has to be told which. Omitted for a uniform mount, which has no choice
   * to make.
   */
  firing_kind?: string;
  /**
   * How many missiles or torpedoes to launch, when fewer than the mount holds.
   * Omitted for a full salvo, and for anything that is not a launcher.
   */
  salvo_size?: number;
  /**
   * Fire Control points spent on this shot as a DM.
   *
   * The program's score is a pool each round, spent either on firing a mount
   * outright or on improving someone else's shot (Core Rulebook p. 161).
   */
  fire_control_dm?: number;
  /** Whether the computer is firing this mount instead of a gunner. */
  computer_fired?: boolean;
};

export type FireState = FireAction[];

/**
 * Systems the ship's computer is repairing itself this round, through its
 * drones. One Auto-Repair point each, and nobody's action.
 */
export type ComputerRepairState = ShipSystem[];
//export type FireActionMsg = {[key: string]: FireState};

export type UnfireAction = {
  weapon_id: number;
};

export type UnfireState = UnfireAction[];

export type PointDefenseAction = {
  weapon_id: number;
  /**
   * A ship to cover instead of this one.
   *
   * Point Defence software lets a ship shoot down what is coming at a
   * neighbour, within Close (/1) or Short (/2) range (High Guard p. 75).
   * Absent means the usual thing: defending yourself.
   */
  protecting?: string;
}
export type PointDefenseState = PointDefenseAction[];

export type SensorState = {
  action: SensorAction;
  target: string;
};

export enum SensorAction {
  None,
  JamMissiles,
  BreakSensorLock,
  SensorLock,
  JamComms,
}

export type SensorActionMsg = {[key: string]: SensorState};

// Sensor utilities
export const DEFAULT_SENSOR_STATE = {action: SensorAction.None, target: ""};

export function newSensorState(action: SensorAction, target: string) {
  return {action: action, target: target};
}

// Engineer action queued for end-of-turn evaluation. Mirrors the Rust
// ShipAction variants (`OverloadDrive` / `OverloadPlant` / `Repair { system }`
// / `Jump`). `null` means "no engineer action queued for this ship."
export type EngineerState =
  | { kind: "OverloadDrive" }
  | { kind: "OverloadPlant" }
  | { kind: "Repair"; system: string }
  | { kind: "Jump" }
  // Offline System: power something down, or bring it back (CRB p. 171).
  | { kind: "SetPower"; system: PowerSystem; online: boolean }
  | null;

// Marshalling/d-marshalling utilities
export function actionPayload(actions: ActionType) {
  return Object.entries(actions).map(([key, value]) => {
    let fire_actions: (object | string)[] = value.fire
      ? value.fire.map((fireAction) => fireActionPayload(fireAction))
      : [];
    if (value.pointDefense) {
      fire_actions = [...fire_actions, ...value.pointDefense.map((pointDefenseAction) => pointDefenseActionPayload(pointDefenseAction))];
    }
    // Anti-actions go last, after everything they might cancel. The client
    // re-sends its whole action list each time, so a delete placed before the
    // point-defence orders was applied and then immediately undone by the very
    // order it was meant to remove -- which is why a point-defence action could
    // not be clicked off and survived round after round. Fire actions escaped
    // this only because they happened to be listed before the delete.
    if (value.unfire) {
      fire_actions = [...fire_actions, ...value.unfire.map((unfireAction) => unfireActionPayload(unfireAction))];
    }
    // One action per operator and per engineer, each naming whose it is.
    (value.sensors ?? []).forEach((sensor, operator) => {
      const sensor_action = sensor ? sensorActionPayload(sensor, operator) : null;
      if (sensor_action) {
        fire_actions.push(sensor_action);
      }
    });

    (value.engineers ?? []).forEach((engineer, index) => {
      if (!engineer) {
        return;
      }
      const engineer_action = engineerActionPayload(engineer, index);
      if (engineer_action) {
        fire_actions.push(engineer_action);
      }
    });

    // The computer's own repairs: one per system, belonging to no one.
    for (const system of value.computerRepairs ?? []) {
      fire_actions.push({ComputerRepair: {system}});
    }

    // Captain leadership check (one per ship; mutually exclusive with
    // `clearLeadership`). Emit whenever there are boosts queued.
    if (value.leadershipCheck && value.leadershipCheck.boosts.length > 0) {
      fire_actions.push({
        LeadershipCheck: {
          boosts: value.leadershipCheck.boosts.map(boostTargetToWire),
        },
      });
    }

    // Anti-actions: explicit "strip queued sensor/engineer/leadership"
    // intents, each naming the crew member whose action is being withdrawn.
    for (const operator of value.clearSensors ?? []) {
      fire_actions.push({ClearSensorAction: {operator}});
    }
    for (const engineer of value.clearEngineers ?? []) {
      fire_actions.push({ClearEngineerAction: {engineer}});
    }
    if (value.clearLeadership) {
      fire_actions.push("ClearLeadershipCheck");
    }

    return [key, fire_actions];
  });
}

// FE BoostTarget -> Rust JSON wire form.
export function boostTargetToWire(b: BoostTarget): object {
  switch (b.kind) {
    case "Fire":
      return { Fire: { ship: b.ship, weapon_id: b.weapon_id } };
    case "PointDefense":
      return { PointDefense: { ship: b.ship, weapon_id: b.weapon_id } };
    case "Sensor":
      return { Sensor: { ship: b.ship, operator: b.operator } };
    case "Detection":
      return { Detection: { ship: b.ship, target: b.target } };
    case "Engineer":
      return { Engineer: { ship: b.ship, engineer: b.engineer } };
    case "Evade":
      return { Evade: { ship: b.ship } };
    case "AssistGunner":
      return { AssistGunner: { ship: b.ship } };
  }
}

// Rust JSON wire form -> FE BoostTarget. Returns null on unrecognized shapes.
export function wireToBoostTarget(raw: unknown): BoostTarget | null {
  if (typeof raw !== "object" || raw === null) {
    return null;
  }
  const obj = raw as Record<string, unknown>;
  if (Object.hasOwn(obj, "Fire")) {
    const v = obj["Fire"] as { ship: string; weapon_id: number };
    return { kind: "Fire", ship: v.ship, weapon_id: v.weapon_id };
  }
  if (Object.hasOwn(obj, "PointDefense")) {
    const v = obj["PointDefense"] as { ship: string; weapon_id: number };
    return { kind: "PointDefense", ship: v.ship, weapon_id: v.weapon_id };
  }
  if (Object.hasOwn(obj, "Sensor")) {
    const v = obj["Sensor"] as { ship: string; operator?: number };
    return { kind: "Sensor", ship: v.ship, operator: v.operator ?? 0 };
  }
  if (Object.hasOwn(obj, "Detection")) {
    const v = obj["Detection"] as { ship: string; target: string };
    return { kind: "Detection", ship: v.ship, target: v.target };
  }
  if (Object.hasOwn(obj, "Engineer")) {
    const v = obj["Engineer"] as { ship: string; engineer?: number };
    return { kind: "Engineer", ship: v.ship, engineer: v.engineer ?? 0 };
  }
  if (Object.hasOwn(obj, "Evade")) {
    const v = obj["Evade"] as { ship: string };
    return { kind: "Evade", ship: v.ship };
  }
  if (Object.hasOwn(obj, "AssistGunner")) {
    const v = obj["AssistGunner"] as { ship: string };
    return { kind: "AssistGunner", ship: v.ship };
  }
  return null;
}

// Stable equality for BoostTarget. Used by reducers that idempotently
// add/remove boost entries.
export function boostTargetEquals(a: BoostTarget, b: BoostTarget): boolean {
  if (a.kind !== b.kind) return false;
  if (a.ship !== b.ship) return false;
  if ((a.kind === "Fire" || a.kind === "PointDefense") &&
      (b.kind === "Fire" || b.kind === "PointDefense")) {
    return a.weapon_id === b.weapon_id;
  }
  // Detection is per pair, so two boosts on the same ship aimed at different
  // quarry are different boosts.
  if (a.kind === "Detection" && b.kind === "Detection") {
    return a.target === b.target;
  }
  return true;
}

function engineerActionPayload(action: EngineerState, engineer: number) {
  if (action === null) {
    return undefined;
  }
  switch (action.kind) {
    case "OverloadDrive":
      return {OverloadDrive: {engineer}};
    case "OverloadPlant":
      return {OverloadPlant: {engineer}};
    case "Repair":
      return {Repair: {system: action.system, engineer}};
    case "Jump":
      return {Jump: {engineer}};
    case "SetPower":
      return {SetPower: {system: action.system, online: action.online, engineer}};
  }
}

function sensorActionPayload(sensor: SensorState, operator: number) {
  switch (sensor.action) {
    case SensorAction.None:
      return undefined;
    case SensorAction.JamMissiles:
      return {JamMissiles: {operator}};
    case SensorAction.BreakSensorLock:
      return {BreakSensorLock: {target: sensor.target, operator}};
    case SensorAction.SensorLock:
      return {SensorLock: {target: sensor.target, operator}};
    case SensorAction.JamComms:
      return {JamComms: {target: sensor.target, operator}};
  }
}

function fireActionPayload(fireAction: FireAction) {
  return {
    FireAction: {
      weapon_id: fireAction.weapon_id,
      target: fireAction.target,
      called_shot_system: fireAction.called_shot_system,
      // Which gun of a mixed turret is firing, and how much of the rack to
      // throw. Both are omitted when there is no choice to record.
      firing_kind: fireAction.firing_kind,
      salvo_size: fireAction.salvo_size,
      // The computer's share of this shot: zero and false are the usual
      // case and stay off the wire.
      fire_control_dm: fireAction.fire_control_dm,
      computer_fired: fireAction.computer_fired,
    },
  };
}

function unfireActionPayload(unfireAction: UnfireAction) {
  return {
    DeleteFireAction: {
      weapon_id: unfireAction.weapon_id,
    },
  };
}

function pointDefenseActionPayload(pointDefenseAction: PointDefenseAction) {
  return {
    PointDefenseAction: {
      weapon_id: pointDefenseAction.weapon_id,
      protecting: pointDefenseAction.protecting,
    },
  };
}

export function payloadToAction(payload: object[]): ActionType {
  const result = {} as ActionType;
  for (const entry of payload) {
    const [shipName, value] = entry as [string, object[]];
    const actions = value as (
      | string
      | {FireAction: object}
      | {DeleteFireAction: object}
      | {PointDefenseAction: object}
      | {JamMissiles: string}
      | {BreakSensorLock: string}
      | {SensorLock: string}
      | {JamComms: string}
      | {Repair: {system: string}}
      | {LeadershipCheck: {boosts: unknown[]}}
    )[];
    if (!actions) {
      continue;
    }

    const fire_actions: FireAction[] = actions
      .filter((action) => typeof action !== "string" && Object.hasOwn(action, "FireAction"))
      .map((action) => {
        if (typeof action !== "string" && Object.hasOwn(action, "FireAction")) {
          return (action as {FireAction: FireAction})["FireAction"];
        } else {
          console.error(
            "(payloadToAction) BUG: Should never get here when looking for 'FireAction' " +
              JSON.stringify(action)
          );
          return {} as FireAction;
        }
      });
    result[shipName] = {...result[shipName], fire: fire_actions};

    const unfire_actions: UnfireAction[] = actions
      .filter((action) => typeof action !== "string" && Object.hasOwn(action, "DeleteFireAction"))
      .map((action) => {
        if (typeof action !== "string" && Object.hasOwn(action, "DeleteFireAction")) {
          return (action as {DeleteFireAction: UnfireAction})["DeleteFireAction"];
        } else {
          console.error(
            "(payloadToAction) BUG: Should never get here when looking for 'DeleteFireAction' " +
              JSON.stringify(action)
          );
          return {} as UnfireAction;
        }
      });
    result[shipName] = {...result[shipName], unfire: unfire_actions};

    const point_defense_actions: PointDefenseAction[] = actions
      .filter((action) => typeof action !== "string" && Object.hasOwn(action, "PointDefenseAction"))
      .map((action) => {
        if (typeof action !== "string" && Object.hasOwn(action, "PointDefenseAction")) {
          return (action as {PointDefenseAction: PointDefenseAction})["PointDefenseAction"];
        } else {
          console.error(
            "(payloadToAction) BUG: Should never get here when looking for 'PointDefenseAction' " +
              JSON.stringify(action)
          );
          return {} as PointDefenseAction;
        }
      });
    result[shipName] = {...result[shipName], pointDefense: point_defense_actions};

    // Sensor and engineer actions each name the crew member working them, so
    // they go back into that person's slot. A list with a hole in it is a
    // watch where somebody has nothing queued.
    const sensors: SensorState[] = [];
    const engineers: EngineerState[] = [];
    const place = <T,>(list: T[], index: number, value: T, empty: T) => {
      while (list.length <= index) {
        list.push(empty);
      }
      list[index] = value;
    };

    for (const action of actions) {
      if (typeof action === "string") {
        continue;
      }
      if (Object.hasOwn(action, "JamMissiles")) {
        const {operator} = (action as unknown as {JamMissiles: {operator?: number}}).JamMissiles ?? {};
        place(sensors, operator ?? 0, {action: SensorAction.JamMissiles, target: ""}, DEFAULT_SENSOR_STATE);
      } else if (Object.hasOwn(action, "BreakSensorLock")) {
        const raw = (action as unknown as {BreakSensorLock: {target: string; operator?: number}}).BreakSensorLock;
        place(
          sensors,
          raw.operator ?? 0,
          {action: SensorAction.BreakSensorLock, target: raw.target},
          DEFAULT_SENSOR_STATE
        );
      } else if (Object.hasOwn(action, "SensorLock")) {
        const raw = (action as unknown as {SensorLock: {target: string; operator?: number}}).SensorLock;
        place(sensors, raw.operator ?? 0, {action: SensorAction.SensorLock, target: raw.target}, DEFAULT_SENSOR_STATE);
      } else if (Object.hasOwn(action, "JamComms")) {
        const raw = (action as unknown as {JamComms: {target: string; operator?: number}}).JamComms;
        place(sensors, raw.operator ?? 0, {action: SensorAction.JamComms, target: raw.target}, DEFAULT_SENSOR_STATE);
      } else if (Object.hasOwn(action, "OverloadDrive")) {
        const {engineer} = (action as unknown as {OverloadDrive: {engineer?: number}}).OverloadDrive ?? {};
        place(engineers, engineer ?? 0, {kind: "OverloadDrive"}, null);
      } else if (Object.hasOwn(action, "OverloadPlant")) {
        const {engineer} = (action as unknown as {OverloadPlant: {engineer?: number}}).OverloadPlant ?? {};
        place(engineers, engineer ?? 0, {kind: "OverloadPlant"}, null);
      } else if (Object.hasOwn(action, "Jump")) {
        const {engineer} = (action as unknown as {Jump: {engineer?: number}}).Jump ?? {};
        place(engineers, engineer ?? 0, {kind: "Jump"}, null);
      } else if (Object.hasOwn(action, "Repair")) {
        const raw = (action as unknown as {Repair: {system: string; engineer?: number}}).Repair;
        place(engineers, raw.engineer ?? 0, {kind: "Repair", system: raw.system}, null);
      } else if (Object.hasOwn(action, "SetPower")) {
        const raw = (
          action as unknown as {SetPower: {system: PowerSystem; online: boolean; engineer?: number}}
        ).SetPower;
        place(
          engineers,
          raw.engineer ?? 0,
          {kind: "SetPower", system: raw.system, online: raw.online},
          null
        );
      }
    }
    result[shipName] = {...result[shipName], sensors, engineers};

    // Extract a queued LeadershipCheck (mutually exclusive — only one per ship).
    let leadershipCheck: ActionType[string]["leadershipCheck"] = null;
    for (const action of actions) {
      if (typeof action === "object" && Object.hasOwn(action, "LeadershipCheck")) {
        const raw = (action as {LeadershipCheck: {boosts: unknown[]}}).LeadershipCheck;
        const boosts: BoostTarget[] = (raw.boosts ?? [])
          .map(wireToBoostTarget)
          .filter((b): b is BoostTarget => b !== null);
        leadershipCheck = { boosts };
        break;
      }
    }
    result[shipName] = {...result[shipName], leadershipCheck};

    // Anti-action flags are transient client-only state; server never echoes them.
    result[shipName] = {
      ...result[shipName],
      clearSensors: [],
      clearEngineers: [],
      clearLeadership: false,
    };
  }
  return result;
}

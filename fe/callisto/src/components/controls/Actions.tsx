export type ActionType = {
  [actor: string]: {
    sensor: SensorState;
    fire: FireState;
    unfire: UnfireState;
    pointDefense: PointDefenseState;
    jump: boolean;
  };
};

// All the different action types.
export type FireAction = {
  target: string;
  weapon_id: number;
  called_shot_system: string | null;
};

export type FireState = FireAction[];
//export type FireActionMsg = {[key: string]: FireState};

export type UnfireAction = {
  weapon_id: number;
};

export type UnfireState = UnfireAction[];

export type PointDefenseAction = {
  weapon_id: number;
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

// Marshalling/d-marshalling utilities
export function actionPayload(actions: ActionType) {
  return Object.entries(actions).map(([key, value]) => {
    let fire_actions: (object | string)[] = value.fire
      ? value.fire.map((fireAction) => fireActionPayload(fireAction))
      : [];
    if (value.unfire) {
      fire_actions = [...fire_actions, ...value.unfire.map((unfireAction) => unfireActionPayload(unfireAction))];
    }
    if (value.pointDefense) {
      fire_actions = [...fire_actions, ...value.pointDefense.map((pointDefenseAction) => pointDefenseActionPayload(pointDefenseAction))];
    }
    const sensor_action = value.sensor ? sensorActionPayload(value.sensor): null;
    if (sensor_action) {
      fire_actions.push(sensor_action);
    }

    if (value.jump) {
      fire_actions.push("Jump");
    }

    return [key, fire_actions];
  });
}

function sensorActionPayload(sensor: SensorState) {
  switch (sensor.action) {
    case SensorAction.None:
      return undefined;
    case SensorAction.JamMissiles:
      return "JamMissiles";
    case SensorAction.BreakSensorLock:
      return {BreakSensorLock: {target: sensor.target}};
    case SensorAction.SensorLock:
      return {SensorLock: {target: sensor.target}};
    case SensorAction.JamComms:
      return {JamComms: {target: sensor.target}};
  }
}

function fireActionPayload(fireAction: FireAction) {
  return {
    FireAction: {
      weapon_id: fireAction.weapon_id,
      target: fireAction.target,
      called_shot_system: fireAction.called_shot_system,
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
      | {Jump: string}
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

    const sensor_action = actions.filter(
      (action) => {
        return !(((typeof action === "object") && Object.hasOwn(action, "FireAction")) || ((typeof action === "object") && Object.hasOwn(action, "PointDefenseAction")) || (typeof action === "string" && action === "Jump"));
      }
    );
    
    if (sensor_action.length === 1) {
      let s = {} as SensorState;
      const action = sensor_action[0] as string | {[key: string]: {target: string}};

      if (sensor_action[0] === "JamMissiles") {
        s = {action: SensorAction.JamMissiles, target: ""};
      } else if (typeof action === "object" && Object.hasOwn(action, "BreakSensorLock")) {
        s = {action: SensorAction.BreakSensorLock, target: action["BreakSensorLock"].target};
      } else if (typeof action === "object" && Object.hasOwn(action, "SensorLock")) {
        s = {action: SensorAction.SensorLock, target: action["SensorLock"].target};
      } else if (typeof action === "object" && Object.hasOwn(action, "JamComms")) {
        s = {action: SensorAction.JamComms, target: action["JamComms"].target};
      } else {
        console.error(
          "(payloadToAction) BUG: Should never get here when looking for sensor action " +
            JSON.stringify(action)
        );
      }
      result[shipName] = {...result[shipName], sensor: s};
    }

    const jump_action = actions.filter((action) => action === "Jump");
    if (jump_action.length === 1) {
      result[shipName] = {...result[shipName], jump: true};
    }
  }
  return result;
}

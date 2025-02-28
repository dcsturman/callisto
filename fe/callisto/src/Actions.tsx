import React, {useContext, createContext} from "react";
import {EntitiesServerContext} from "./Universal";
import {updateActions} from "./ServerManager";

export type ActionType = {
  [actor: string]: {
    sensor: SensorState;
    fire: FireState;
  };
};

// A context that allows access to and manipulations of actions. While accessing actions is
// relatively straightforward, writing to this structure is tricky we include a number of different
// fields on the context to make this more straightforward.
export const ActionContext = createContext<{
  actions: ActionType;
  setActions: (actions: ActionType) => void;
  setSensorAction: (shipName: string, action: SensorState) => void;
  fireWeapon: (
    shipName: string,
    weapon_id: number,
    target: string,
    called_shot_system?: string
  ) => void;
}>({
  actions: {},
  setActions: () => {},
  setSensorAction: () => {},
  fireWeapon: () => {},
});

const ActionContextProvider = ActionContext.Provider;

export type FireAction = {
  target: string;
  weapon_id: number;
  called_shot_system: string | null;
};

export type FireState = FireAction[];
export type FireActionMsg = {[key: string]: FireState};

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

export const DEFAULT_SENSOR_STATE = {action: SensorAction.None, target: ""};

export function newSensorState(action: SensorAction, target: string) {
  return {action: action, target: target};
}

type ActionsContextComponent =  {
  actions: ActionType;
  setActions: (actions: ActionType) => void;
}

export const ActionsContextComponent: React.FC<React.PropsWithChildren<ActionsContextComponent>> = ({actions, setActions, children}) => {
  const serverEntities = useContext(EntitiesServerContext);

  const setSensorAction = (shipName: string, action: SensorState) => {
    const next = {
      ...actions,
      [shipName]: {...actions[shipName], sensor: action},
    };
    setActions(next);
    updateActions(next);
  };

  const fireWeapon = (
    shipName: string,
    weapon_id: number,
    target: string,
    called_shot_system?: string
  ) => {
    // First validate shipName and target to be real ships.
    if (!serverEntities.entities.ships.find((ship) => ship.name === shipName)) {
      console.error("(Actions.fireWeapon) No such ship " + shipName + ".");
      return;
    }

    if (!serverEntities.entities.ships.find((ship) => ship.name === target)) {
      console.error("(Actions.fireWeapon) No such target " + target + ".");
      return;
    }

    const new_action: FireAction = {
      target: target,
      weapon_id: weapon_id,
      called_shot_system: called_shot_system ?? null,
    };
    const next = {
      ...actions,
      [shipName]: {
        ...actions[shipName],
        fire: [...actions[shipName]?.fire??[], new_action],
      },
    };
    setActions(next);
    updateActions(next);
  };

  return (
    <ActionContextProvider
      value={{
        actions,
        setActions,
        setSensorAction,
        fireWeapon,
      }}>
      {children}
    </ActionContextProvider>
  );
};

export function actionPayload(actions: ActionType) {
  return Object.entries(actions).map(([key, value]) => {
    const fire_actions: (object | string)[] = value.fire
      ? value.fire.map((fireAction) => fireActionPayload(fireAction))
      : [];
    const sensor_action = value.sensor ? sensorActionPayload(value.sensor): null;
    if (sensor_action) {
      fire_actions.push(sensor_action);
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

export function payloadToAction(payload: object[]): ActionType {
  const result = {} as ActionType;
  for (const entry of payload) {
    const [shipName, value] = entry as [string, object[]];
    const actions = value as (
      | string
      | {FireAction: object}
      | {BreakSensorLock: string}
      | {SensorLock: string}
      | {JamComms: string}
    )[];
    console.log(`(payloadToAction) Received actions for ${shipName}: ${JSON.stringify(actions)}`);
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

    const sensor_action = actions.filter(
      (action) => !((typeof action === "object") && Object.hasOwn(action, "FireAction"))
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
  }
  return result;
}

import React, { useState, useEffect, useContext, createContext } from "react";
import {
  EntitiesServerContext,
  DesignTemplatesContext,
  WeaponMount,
} from "./Universal";
import { updateActions } from "./ServerManager";

export type ActionType = {
  [actor: string]: {
    sensor: SensorState;
    fire: {
      weapons: CompressedWeaponType;
      state: FireState;
    };
  };
};

// A context that allows access to and manipulations of actions. While accessing actions is
// relatively straightforward, writing to this structure is tricky we include a number of different
// fields on the context to make this more straightforward.
export const ActionContext = createContext<{
  actions: ActionType;
  setActions: (actions: ActionType) => void;
  setSensorAction: (shipName: string, action: SensorState) => void;
  fireWeapon: (shipName: string, weapon: string) => void;
  setFireActions: (
    shipName: string,
    weapons: CompressedWeaponType,
    state: FireState
  ) => void;
  addFireAction: (shipName: string, action: FireAction) => void;
  resetActions: () => void;
}>({
  actions: {},
  setActions: () => {},
  setSensorAction: () => {},
  fireWeapon: () => {},
  setFireActions: () => {},
  addFireAction: () => {},
  resetActions: () => {},
});

const ActionContextProvider = ActionContext.Provider;

export type CompressedWeaponType = {
  [weapon: string]: {
    kind: string;
    mount: WeaponMount;
    used: number;
    total: number;
  };
};

export type FireAction = {
  target: string;
  weapon_id: number;
  called_shot_system: string | null;
};


export type FireState = FireAction[];
export type FireActionMsg = { [key: string]: FireState };

export function newFireAction(target: string, weapon_id: number) {
  return { target: target, weapon_id: weapon_id, called_shot_system: null };
}

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

export type SensorActionMsg = { [key: string]: SensorState };

export const DEFAULT_SENSOR_STATE = { action: SensorAction.None, target: "" };

export function newSensorState(action: SensorAction, target: string) {
  return { action: action, target: target };
}

export const ActionsContextComponent: React.FC<
  React.PropsWithChildren<unknown>
> = ({ children }) => {
  const serverEntities = useContext(EntitiesServerContext);
  const designs = useContext(DesignTemplatesContext);

  const [actions, setActions] = useState(() => {
    console.log("***************** Initializing actions");
    return Object.fromEntries(
      serverEntities.entities.ships.map((ship) => {
        return [
          ship.name,
          {
            sensor: DEFAULT_SENSOR_STATE,
            fire: {
              weapons: designs.templates[ship.design].compressedWeapons(),
              state: [] as FireAction[],
            },
          },
        ];
      })
    );
  }
  );

  useEffect(() => {
    if (actions && Object.keys(actions).length > 0) {
      updateActions(actions);
    }
  }, [actions]);

  const setSensorAction = (shipName: string, action: SensorState) => {
    setActions({
      ...actions,
      [shipName]: { ...actions[shipName], sensor: action },
    });
  };

  const fireWeapon = (shipName: string, weapon: string) => {
    setActions({
      ...actions,
      [shipName]: {
        ...actions[shipName],
        fire: {
          ...actions[shipName].fire,
          weapons: {
            ...actions[shipName].fire.weapons,
            [weapon]: {
              ...actions[shipName].fire.weapons[weapon],
              used: actions[shipName].fire.weapons[weapon].used + 1,
            },
          },
        },
      },
    });
  };

  const setFireActions = (
    shipName: string,
    weapons: CompressedWeaponType,
    state: FireState
  ) => {
    const current = actions[shipName];

    setActions({...actions,
        [shipName]: {
          ...current,
          fire: { weapons: weapons, state: state },
        },
      });
  };

  const addFireAction = (shipName: string, action: FireAction) => {
    setActions(
      {
        ...actions,
        [shipName]: {
          ...actions[shipName],
          fire: {
            ...actions[shipName].fire,
            state: [...actions[shipName].fire.state, action],
          },
        },
      }
    );
  };

  const resetActions = () => {
    setActions(Object.fromEntries(
      serverEntities.entities.ships.map((ship) => {
        return [
          ship.name,
          {
            sensor: DEFAULT_SENSOR_STATE,
            fire: {
              weapons: designs.templates[ship.design].compressedWeapons(),
              state: [],
            },
          },
        ];
      })
    ));
  };

  return (
    <ActionContextProvider
      value={{
        actions,
        setActions,
        setSensorAction,
        fireWeapon,
        setFireActions,
        addFireAction,
        resetActions,
      }}>
      {children}
    </ActionContextProvider>
  );
};

export function actionPayload(actions: ActionType) {
  console.log("********************(actionPayload) convert actions " + JSON.stringify(actions));
  return Object.entries(actions).map(([key, value]) => {
    const fire_actions: (object | string)[] =
      value.fire?.state ? value.fire.state.map((fireAction) => fireActionPayload(fireAction)) : [];
    const sensor_action = sensorActionPayload(value.sensor);
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
      return { BreakSensorLock: { target: sensor.target } };
    case SensorAction.SensorLock:
      return { SensorLock: { target: sensor.target } };
    case SensorAction.JamComms:
      return { JamComms: { target: sensor.target } };
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

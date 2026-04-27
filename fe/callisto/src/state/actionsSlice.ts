import {updateActions} from "lib/serverManager";
import {EntityList, findShip} from "lib/entities";
import {createSlice, PayloadAction} from "@reduxjs/toolkit";
import {
  ActionType,
  SensorState,
  SensorAction,
  EngineerState,
  PointDefenseAction,
  UnfireAction,
  FireAction,
  DEFAULT_SENSOR_STATE,
} from "components/controls/Actions";

export type ActionsState = ActionType;

const initialState = {} as ActionType;

const newShipAction = () => {
  return {
    sensor: DEFAULT_SENSOR_STATE,
    fire: [],
    unfire: [],
    pointDefense: [],
    jump: false,
    engineer: null as EngineerState,
    clearSensor: false,
    clearEngineer: false,
  };
};

export const actionsSlice = createSlice({
  name: "server",
  initialState,
  reducers: {
    setActions: (state, item: PayloadAction<ActionType>) => {
      // Clear existing state
      Object.keys(state).forEach(key => delete state[key]);
      // Copy new payload into state
      Object.assign(state, item.payload);
    },
    setSensorAction: (state, item: PayloadAction<{ shipName: string, action: SensorState}>) => {
      state[item.payload.shipName] ??= newShipAction();
      state[item.payload.shipName].sensor = item.payload.action;
      // Setting None means "clear" — flag the anti-action so the server strips
      // any sensor action it has queued for this ship. Setting any other
      // sensor action implicitly replaces, so no clear flag needed.
      state[item.payload.shipName].clearSensor =
        item.payload.action.action === SensorAction.None;
      updateActions(state);
    },
    setEngineerAction: (state, item: PayloadAction<{ shipName: string, action: EngineerState}>) => {
      state[item.payload.shipName] ??= newShipAction();
      state[item.payload.shipName].engineer = item.payload.action;
      // null means "clear" — flag the anti-action. Any other engineer action
      // replaces on the server side via merge.
      state[item.payload.shipName].clearEngineer = item.payload.action === null;
      updateActions(state);
    },
    fireWeapon: (
      state,
      item: PayloadAction<{
        shipName: string;
        weapon_id: number;
        target: string;
        entities: EntityList;
        called_shot?: string;
      }>
    ) => {
      const entities = item.payload.entities;
      // First validate shipName and target to be real ships.
      if (!findShip(entities, item.payload.shipName)) {
        console.error("(actionSlice.fireWeapon) No such ship " + item.payload.shipName + ".");
        return;
      }

      if (!findShip(entities, item.payload.target)) {
        console.error("(Actions.fireWeapon) No such target " + item.payload.target + ".");
        return;
      }

      const new_action: FireAction = {
        target: item.payload.target,
        weapon_id: item.payload.weapon_id,
        called_shot_system: item.payload.called_shot ?? null,
      };
      state[item.payload.shipName] ??= newShipAction();
      state[item.payload.shipName].fire.push(new_action);
      updateActions(state);
    },
    jump: (state, item: PayloadAction<string>) => {
      state[item.payload[0]] ??= newShipAction();
      state[item.payload].jump = true;
      updateActions(state);
    },
    pointDefenseWeapon: (state, item: PayloadAction<{shipName: string; weapon_id: number}>) => {
      const new_action: PointDefenseAction = {weapon_id: item.payload.weapon_id};

      state[item.payload.shipName] ??= newShipAction();
      state[item.payload.shipName].pointDefense.push(new_action);
      updateActions(state);
    },
    unfireWeapon: (state, item: PayloadAction<{shipName: string; weapon_id: number}>) => {
      const new_action: UnfireAction = {weapon_id: item.payload.weapon_id};
      state[item.payload.shipName].unfire.push(new_action);
      updateActions(state);
    },
    updateFireCalledShot: (
      state,
      item: PayloadAction<{shipName: string; index: number; system: string | null}>
    ) => {
      state[item.payload.shipName].fire[item.payload.index].called_shot_system =
        item.payload.system;
      updateActions(state);
    },
    resetServer: () => initialState,
  },
});

export const {
  setActions,
  setSensorAction,
  setEngineerAction,
  fireWeapon,
  jump,
  pointDefenseWeapon,
  unfireWeapon,
  updateFireCalledShot,
  resetServer,
} = actionsSlice.actions;

export type ActionsReducer = ReturnType<typeof actionsSlice.reducer>;
export default actionsSlice.reducer;

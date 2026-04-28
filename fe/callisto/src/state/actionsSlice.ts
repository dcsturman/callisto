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
  BoostTarget,
  boostTargetEquals,
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
    engineer: null as EngineerState,
    leadershipCheck: null as { boosts: BoostTarget[] } | null,
    clearSensor: false,
    clearEngineer: false,
    clearLeadership: false,
  };
};

export const actionsSlice = createSlice({
  name: "server",
  initialState,
  reducers: {
    // Replace the entire actions slice with a server-derived snapshot. The
    // captain's locally-edited boost list is held in Redux only between
    // explicit flushes (Update / CaptainAction), so when an EntityResponse
    // arrives mid-turn we must NOT clobber pending boosts the captain has not
    // yet committed.
    //
    // After end-of-turn `Update`, the server resets `leadership_rolled` to
    // false on the captain's ship. We use that signal: when the parsed
    // snapshot reflects an unrolled captain, drop the local boost list (the
    // round is over, those boosts have either been applied or expired).
    // While `leadership_rolled` is true (mid-turn after the captain rolled),
    // preserve local boosts.
    //
    // The two-property payload shape avoids `import { store }` cycles inside
    // the slice file.
    setActions: (
      state,
      item: PayloadAction<{
        parsed: ActionType;
        captainShipName: string | null;
        captainLeadershipRolled: boolean;
      }>
    ) => {
      const { parsed, captainShipName, captainLeadershipRolled } = item.payload;

      // Snapshot the captain's local leadership boosts before reset, so a
      // peer's ModifyActions / EntityResponse round-trip doesn't drop them.
      const localCaptainLC =
        captainShipName && state[captainShipName]
          ? state[captainShipName].leadershipCheck
          : null;
      const localCaptainClearLeadership =
        captainShipName && state[captainShipName]
          ? state[captainShipName].clearLeadership
          : false;

      // Clear existing state
      Object.keys(state).forEach((key) => delete state[key]);
      // Copy new payload into state
      Object.assign(state, parsed);

      // Restore captain's local boosts only when:
      //  - We have a captain ship.
      //  - Local boosts are non-empty (worth preserving).
      //  - The captain is still mid-turn (`leadership_rolled` true on the
      //    fresh server snapshot). End-of-turn flips this to false and we
      //    intentionally let the boost list go stale-then-cleared.
      if (
        captainShipName &&
        captainLeadershipRolled &&
        localCaptainLC &&
        localCaptainLC.boosts.length > 0
      ) {
        state[captainShipName] ??= newShipAction();
        state[captainShipName].leadershipCheck = localCaptainLC;
        state[captainShipName].clearLeadership = localCaptainClearLeadership;
      }
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
    // Idempotently add or remove a boost target. When the list goes empty,
    // set `clearLeadership` so the server strips its queued LeadershipCheck;
    // otherwise the LeadershipCheck wire form rides along on the next
    // ModifyActions. Boost state is held locally in Redux only — no
    // updateActions round-trip per click. The list is flushed to the server
    // on Update / CaptainAction (see serverManager.nextRound /
    // serverManager.captainAction).
    toggleBoost: (state, item: PayloadAction<{ shipName: string; target: BoostTarget }>) => {
      state[item.payload.shipName] ??= newShipAction();
      const slot = state[item.payload.shipName];
      const boosts = slot.leadershipCheck?.boosts ?? [];
      const idx = boosts.findIndex((b) => boostTargetEquals(b, item.payload.target));
      const nextBoosts =
        idx === -1 ? [...boosts, item.payload.target] : boosts.filter((_, i) => i !== idx);
      slot.leadershipCheck = { boosts: nextBoosts };
      slot.clearLeadership = nextBoosts.length === 0;
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
  toggleBoost,
  fireWeapon,
  pointDefenseWeapon,
  unfireWeapon,
  updateFireCalledShot,
  resetServer,
} = actionsSlice.actions;

export type ActionsReducer = ReturnType<typeof actionsSlice.reducer>;
export default actionsSlice.reducer;

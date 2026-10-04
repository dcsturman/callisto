import {updateActions} from "lib/serverManager";
import {EntityList, ShipSystem, findShip} from "lib/entities";
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
    sensors: [] as SensorState[],
    fire: [],
    unfire: [],
    pointDefense: [],
    engineers: [] as EngineerState[],
    computerRepairs: [] as ShipSystem[],
    leadershipCheck: null as { boosts: BoostTarget[] } | null,
    pendingBoosts: [] as {target: BoostTarget; on: boolean}[],
    clearSensors: [] as number[],
    clearEngineers: [] as number[],
    clearLeadership: false,
  };
};

/** One ship's queued actions: the value type of the state map. Named from the
 * map rather than from `newShipAction`, whose empty literals infer `never[]`. */
type ShipActionSlot = ActionType[string];

/**
 * Remove the boosts that inspired an action which no longer exists.
 *
 * A captain's boost is a +1 on some other crew member's roll, so it only means
 * anything while that roll is still going to happen. Boost state is held
 * locally and deliberately survives server snapshots mid-turn (see
 * `setActions`), which means nothing else will ever clear it -- so every
 * place an action is withdrawn has to withdraw its boost too, or the captain
 * is left with an inspire pinned to nothing. That is exactly what happened:
 * deselect Assist Gunner and its boost stayed, and the only way out was to
 * reselect, un-inspire, and deselect again.
 *
 * `which` narrows the drop to one mount or one crew member: a weapon id for
 * Fire and PointDefense, a crew position for Sensor and Engineer. Without it
 * every boost of those kinds goes.
 */
const dropBoostsFrom = (
  slot: ShipActionSlot | undefined,
  kinds: BoostTarget["kind"][],
  which?: number,
) => {
  const boosts = slot?.leadershipCheck?.boosts;
  if (slot == null || boosts == null || boosts.length === 0) {
    return;
  }
  const next = boosts.filter((b) => {
    if (!kinds.includes(b.kind)) {
      return true;
    }
    if (which !== undefined && "weapon_id" in b) {
      return b.weapon_id !== which;
    }
    if (which !== undefined && "operator" in b) {
      return b.operator !== which;
    }
    if (which !== undefined && "engineer" in b) {
      return b.engineer !== which;
    }
    return false;
  });
  if (next.length === boosts.length) {
    return;
  }
  // Withdrawing a boost is a toggle like any other, and has to be held
  // against an in-flight snapshot the same way.
  const removed = boosts.filter(
    (boost) => !next.some((kept) => boostTargetEquals(kept, boost))
  );
  slot.pendingBoosts = [
    ...(slot.pendingBoosts ?? []).filter(
      (entry) => !removed.some((boost) => boostTargetEquals(entry.target, boost))
    ),
    ...removed.map((target) => ({target, on: false})),
  ];
  slot.leadershipCheck = { boosts: next };
  // Same rule as toggleBoost: an empty list means "strip the queued
  // LeadershipCheck", not "leave the old one on the server".
  slot.clearLeadership = next.length === 0;
};

export const actionsSlice = createSlice({
  name: "server",
  initialState,
  reducers: {
    // Replace the entire actions slice with a server-derived snapshot.
    //
    // The server is the truth about who is inspiring what: two consoles on
    // the same ship have to agree, and the one that did not make a tick has
    // no business keeping its older idea of the list. But a tick made here
    // is in Redux before it is on the wire, so a snapshot answering an
    // earlier request would undo it -- and the next order sent would make
    // that permanent.
    //
    // So the snapshot wins, with this browser's own un-echoed toggles laid
    // back over it. Each toggle says which way it went, and retires as soon
    // as the server agrees. `rolledShips` is every ship whose captain has
    // rolled; when a round ends that flag clears, and with it any toggle
    // still waiting, because those boosts have been applied or have expired.
    setActions: (
      state,
      item: PayloadAction<{
        parsed: ActionType;
        rolledShips: string[];
      }>
    ) => {
      const { parsed, rolledShips } = item.payload;

      const pending = new Map<string, {target: BoostTarget; on: boolean}[]>();
      for (const shipName of rolledShips) {
        const waiting = state[shipName]?.pendingBoosts;
        if (waiting && waiting.length > 0) {
          pending.set(shipName, waiting.map((entry) => ({...entry})));
        }
      }

      // Clear existing state
      Object.keys(state).forEach((key) => delete state[key]);
      // Copy new payload into state
      Object.assign(state, parsed);

      for (const [shipName, waiting] of pending) {
        state[shipName] ??= newShipAction();
        const slot = state[shipName];
        let boosts = [...(slot.leadershipCheck?.boosts ?? [])];
        const stillWaiting: {target: BoostTarget; on: boolean}[] = [];
        for (const entry of waiting) {
          const present = boosts.some((boost) => boostTargetEquals(boost, entry.target));
          if (present === entry.on) {
            // The server has caught up with this one.
            continue;
          }
          stillWaiting.push(entry);
          boosts = entry.on
            ? [...boosts, entry.target]
            : boosts.filter((boost) => !boostTargetEquals(boost, entry.target));
        }
        slot.pendingBoosts = stillWaiting;
        slot.leadershipCheck = {boosts};
        slot.clearLeadership = boosts.length === 0 && stillWaiting.length > 0;
      }
    },
    // One operator's action. `operator` is their place in the crew, so a
    // shipmate's queued action is untouched.
    setSensorAction: (
      state,
      item: PayloadAction<{shipName: string; operator?: number; action: SensorState}>
    ) => {
      const {shipName, action} = item.payload;
      const operator = item.payload.operator ?? 0;
      state[shipName] ??= newShipAction();
      const slot = state[shipName];
      while (slot.sensors.length <= operator) {
        slot.sensors.push(DEFAULT_SENSOR_STATE);
      }
      slot.sensors[operator] = action;
      // Setting None means "clear" -- name this operator in the anti-action so
      // the server strips theirs. Anything else replaces, so no flag needed.
      slot.clearSensors = slot.clearSensors.filter((at) => at !== operator);
      if (action.action === SensorAction.None) {
        slot.clearSensors.push(operator);
        dropBoostsFrom(slot, ["Sensor"], operator);
      }
      updateActions(state);
    },
    // One engineer's job, on the same terms.
    setEngineerAction: (
      state,
      item: PayloadAction<{shipName: string; engineer?: number; action: EngineerState}>
    ) => {
      const {shipName, action} = item.payload;
      const engineer = item.payload.engineer ?? 0;
      state[shipName] ??= newShipAction();
      const slot = state[shipName];
      while (slot.engineers.length <= engineer) {
        slot.engineers.push(null);
      }
      slot.engineers[engineer] = action;
      slot.clearEngineers = slot.clearEngineers.filter((at) => at !== engineer);
      if (action === null) {
        slot.clearEngineers.push(engineer);
        dropBoostsFrom(slot, ["Engineer"], engineer);
      }
      updateActions(state);
    },
    // Idempotently add or remove a boost target. When the list goes empty,
    // set `clearLeadership` so the server strips its queued LeadershipCheck;
    // otherwise the LeadershipCheck wire form rides along on the ModifyActions
    // below.
    //
    // Each toggle goes to the server, as every other queued action does. Held
    // locally it was invisible to everyone else: a player taking the captain's
    // seat, or the referee looking at that ship, saw empty boxes while the
    // captain saw their own ticks.
    toggleBoost: (state, item: PayloadAction<{ shipName: string; target: BoostTarget }>) => {
      state[item.payload.shipName] ??= newShipAction();
      const slot = state[item.payload.shipName];
      const boosts = slot.leadershipCheck?.boosts ?? [];
      const idx = boosts.findIndex((b) => boostTargetEquals(b, item.payload.target));
      const on = idx === -1;
      const nextBoosts = on ? [...boosts, item.payload.target] : boosts.filter((_, i) => i !== idx);
      slot.leadershipCheck = { boosts: nextBoosts };
      slot.clearLeadership = nextBoosts.length === 0;
      // Remember the toggle until the server says it back, so a snapshot
      // already in flight cannot undo it.
      slot.pendingBoosts = [
        ...(slot.pendingBoosts ?? []).filter(
          (entry) => !boostTargetEquals(entry.target, item.payload.target)
        ),
        {target: item.payload.target, on},
      ];
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
        /**
         * Which weapon type in the mount is firing. A mixed turret may only use
         * one type per round, so it has to be named; omitted for a uniform
         * mount, which has no choice to make.
         */
        firing_kind?: string;
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
      if (item.payload.firing_kind != null) {
        new_action.firing_kind = item.payload.firing_kind;
      }
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
      dropBoostsFrom(state[item.payload.shipName], ["Fire", "PointDefense"], item.payload.weapon_id);
      updateActions(state);
    },
    // Pilot actions have no reducer of their own -- they go straight to the
    // server -- so their boosts are dropped by an explicit dispatch from
    // `setCrewActions`. Local only, like toggleBoost: flushed on the next
    // Update / CaptainAction.
    dropBoosts: (
      state,
      item: PayloadAction<{ shipName: string; kinds: BoostTarget["kind"][]; weapon_id?: number }>
    ) => {
      dropBoostsFrom(state[item.payload.shipName], item.payload.kinds, item.payload.weapon_id);
    },
    updateFireCalledShot: (
      state,
      item: PayloadAction<{shipName: string; index: number; system: string | null}>
    ) => {
      state[item.payload.shipName].fire[item.payload.index].called_shot_system =
        item.payload.system;
      updateActions(state);
    },
    // How much of the rack to throw. The full salvo is the default and is
    // stored as undefined, so an untouched action looks exactly as it did
    // before short salvoes existed.
    updateFireSalvo: (
      state,
      item: PayloadAction<{shipName: string; index: number; size: number | null}>
    ) => {
      const action = state[item.payload.shipName]?.fire[item.payload.index];
      if (!action) {
        return;
      }
      if (item.payload.size == null) {
        delete action.salvo_size;
      } else {
        action.salvo_size = item.payload.size;
      }
      updateActions(state);
    },
    /**
     * How much of the Fire Control pool this shot draws.
     *
     * `computerFired` has the computer fire a mount with no gunner behind
     * it, which costs one point; `dm` is what it adds to the shot. Both
     * default to nothing and stay off the wire when unset.
     */
    updateFireControl: (
      state,
      item: PayloadAction<{shipName: string; index: number; dm?: number; computerFired?: boolean}>
    ) => {
      const action = state[item.payload.shipName]?.fire[item.payload.index];
      if (!action) {
        return;
      }
      if (item.payload.dm != null) {
        if (item.payload.dm === 0) {
          delete action.fire_control_dm;
        } else {
          action.fire_control_dm = item.payload.dm;
        }
      }
      if (item.payload.computerFired != null) {
        if (item.payload.computerFired) {
          action.computer_fired = true;
        } else {
          delete action.computer_fired;
        }
      }
      updateActions(state);
    },
    /**
     * Send the repair drones to a system, or call them back.
     *
     * One Auto-Repair point each, and no one's action: the computer runs
     * these itself (Core Rulebook p. 161).
     */
    setComputerRepair: (
      state,
      item: PayloadAction<{shipName: string; system: ShipSystem; repair: boolean}>
    ) => {
      state[item.payload.shipName] ??= newShipAction();
      const ship = state[item.payload.shipName];
      ship.computerRepairs ??= [];
      ship.computerRepairs = ship.computerRepairs.filter((system) => system !== item.payload.system);
      if (item.payload.repair) {
        ship.computerRepairs.push(item.payload.system);
      }
      updateActions(state);
    },
    /**
     * Point this mount's point defence at a neighbour, or back at our own
     * ship. Needs Point Defence software and the other ship close by; the
     * server checks both when the round resolves.
     */
    setPointDefenseWard: (
      state,
      item: PayloadAction<{shipName: string; weapon_id: number; protecting: string | null}>
    ) => {
      const action = state[item.payload.shipName]?.pointDefense.find(
        (pd) => pd.weapon_id === item.payload.weapon_id
      );
      if (!action) {
        return;
      }
      if (item.payload.protecting == null) {
        delete action.protecting;
      } else {
        action.protecting = item.payload.protecting;
      }
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
  dropBoosts,
  fireWeapon,
  pointDefenseWeapon,
  unfireWeapon,
  updateFireCalledShot,
  updateFireSalvo,
  updateFireControl,
  setComputerRepair,
  setPointDefenseWard,
  resetServer,
} = actionsSlice.actions;

export type ActionsReducer = ReturnType<typeof actionsSlice.reducer>;
export default actionsSlice.reducer;

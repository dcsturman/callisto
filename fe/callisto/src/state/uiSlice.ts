import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { FlightPath } from "lib/flightPath";
import { Entity } from 'lib/entities';
import { Event } from 'components/space/Effects';

const INITIAL_QUATERNION: [number, number, number, number] = [0, 0.7071, 0, -0.7071];

export interface UISlice {
    entityToShow: Entity | null;
    proposedPlan: FlightPath | null;
    showResults: boolean;
    events: Event[] | null;
    cameraPos: [number, number, number];
    cameraQuaternion: [number, number, number, number];
    gravityWells: boolean;
    jumpDistance: boolean;
    showRange: string | null;
    computerShipName: string | null;
    /**
     * The order the station cards are shown in, by card id.
     *
     * A player's own arrangement: which instruments they want at the top of
     * the column. Cards not named here follow in their natural order, so a
     * card added later appears rather than vanishing for anyone who has
     * arranged theirs.
     */
    cardOrder: string[];
    /**
     * Whether the Scenario Builder holds edits that have not been saved.
     *
     * Set by the scenario-mutating requests in serverManager, cleared on a
     * successful save and whenever a scenario is joined, created or left. Only
     * consulted in AppMode.ScenarioBuilder — a running game has no "save", so
     * nothing there can be unsaved.
     */
    scenarioDirty: boolean;
}

const initialState: UISlice  = {
    entityToShow: null,
    proposedPlan: null,
    showResults: false,
    events: null,
    cameraPos: [-100, 0, 0],
    cameraQuaternion: INITIAL_QUATERNION,
    gravityWells: false,
    jumpDistance: false,
    showRange: null,
    computerShipName: null,
    cardOrder: [],
    scenarioDirty: false,
}

export const uiSlice = createSlice({
  name: 'ui',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setEntityToShow: (state, action: PayloadAction<Entity | null>) => {
        state.entityToShow = action.payload;
    },
    setScenarioDirty: (state, action: PayloadAction<boolean>) => {
        state.scenarioDirty = action.payload;
    },
    setProposedPlan: (state, action: PayloadAction<FlightPath | null>) => {
        state.proposedPlan = action.payload;
    },
    setShowResults: (state, action: PayloadAction<boolean>) => {
        state.showResults = action.payload;
    },
    setEvents: (state, action: PayloadAction<Event[] | null>) => {
        state.events = action.payload;
    },
    // Removals work on the store's current list, by id. An animation that
    // finishes holds the list from when it started, and filtering that copy
    // would put back events other animations had already removed.
    removeEvent: (state, action: PayloadAction<number>) => {
        state.events = state.events?.filter((event) => event.id !== action.payload) ?? null;
    },
    clearMessageEvents: (state) => {
        state.events = state.events?.filter((event) => event.kind !== "Message") ?? null;
    },
    setCameraPos: (state, action: PayloadAction<{x: number, y: number, z: number}>) => {
        state.cameraPos = [action.payload.x, action.payload.y, action.payload.z];
    },
    setCameraQuaternion: (state, action: PayloadAction<[number, number, number, number]>) => {
        state.cameraQuaternion = [action.payload[0], action.payload[1], action.payload[2], action.payload[3]];
    },
    setGravityWells: (state, action: PayloadAction<boolean>) => { state.gravityWells = action.payload },
    setJumpDistance: (state, action: PayloadAction<boolean>) => { state.jumpDistance = action.payload },
    setShowRange: (state, action: PayloadAction<string | null>) => {
        state.showRange = action.payload;
    },
    setComputerShipName: (state, action: PayloadAction<string | null>) => {
        state.computerShipName = action.payload;
    },
    /**
     * Move a card one place earlier or later.
     *
     * `order` only holds what the player has arranged; the caller passes the
     * full list as it stands so a first move has something to reorder.
     */
    moveCard: (
      state,
      action: PayloadAction<{id: string; delta: number; current: string[]}>
    ) => {
      const order = state.cardOrder.length > 0 ? [...state.cardOrder] : [...action.payload.current];
      // A card the stored order has not seen yet: put the current list in
      // place first, so moving it lands where the player can see.
      for (const id of action.payload.current) {
        if (!order.includes(id)) {
          order.push(id);
        }
      }
      const from = order.indexOf(action.payload.id);
      const to = from + action.payload.delta;
      if (from < 0 || to < 0 || to >= order.length) {
        return;
      }
      const [moved] = order.splice(from, 1);
      order.splice(to, 0, moved);
      state.cardOrder = order;
    },
    /** Back to the order the cards come in. */
    resetCardOrder: (state) => {
      state.cardOrder = [];
    },
    resetServer: () => initialState,
  }
});

export const { setEntityToShow, setScenarioDirty, setProposedPlan, setShowResults, setEvents, removeEvent, clearMessageEvents, setCameraPos, setCameraQuaternion, setGravityWells, setJumpDistance, setShowRange, setComputerShipName, moveCard, resetCardOrder, resetServer } = uiSlice.actions;
export type UIReducer = ReturnType<typeof uiSlice.reducer>;
export default uiSlice.reducer;

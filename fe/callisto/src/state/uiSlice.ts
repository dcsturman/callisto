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
     * Whether the targets board leaves out our own side.
     *
     * A gunner is not shooting at their squadron, so the default is to
     * leave them off and keep the card for the ships that matter. A referee
     * reading the board for everyone can tick them back on.
     */
    showAlliesOnTargets: boolean;
    /**
     * Which ships' results to show, and which kinds.
     *
     * Empty means everything, which is how a round starts: a filter nobody
     * has touched should not be hiding anything. A line survives if any ship
     * it names is chosen and its kind is chosen, so picking a ship gives both
     * what it did and what was done to it.
     */
    resultShips: string[];
    resultCategories: string[];
    /**
     * Whether the Scenario Builder holds edits that have not been saved.
     *
     * Set by the scenario-mutating requests in serverManager, cleared on a
     * successful save and whenever a scenario is joined, created or left. Only
     * consulted in AppMode.ScenarioBuilder — a running game has no "save", so
     * nothing there can be unsaved.
     */
    scenarioDirty: boolean;
    /** How many saves have succeeded this session: for the tutorial to see one happen. */
    scenarioSaves: number;
    /** How many renames this player has asked for: for the tutorial, which
     *  cannot tell a rename from names alone if it came before the step. */
    renames: number;
    /**
     * The ships ticked for the Squadrons panel's Move.
     *
     * Held here rather than in the panel so the 3D view's box-select and the
     * roster can tick ships too: with a hundred of them, ticking one box at a
     * time is not a way to build squadrons.
     */
    fleetSelection: string[];
    /** The squadron last picked in the roster or the 3D view, which the
     *  Orders panel then addresses. */
    focusSquadron: { team: string | null; name: string } | null;
    /** Whether a squadron far from the camera draws as one labelled marker
     *  rather than a label per ship. */
    squadronMarkers: boolean;
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
    showAlliesOnTargets: false,
    resultShips: [],
    resultCategories: [],
    scenarioDirty: false,
    scenarioSaves: 0,
    renames: 0,
    fleetSelection: [],
    focusSquadron: null,
    squadronMarkers: true,
}

export const uiSlice = createSlice({
  name: 'ui',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setEntityToShow: (state, action: PayloadAction<Entity | null>) => {
        state.entityToShow = action.payload;
    },
    renamed: (state) => {
        state.renames += 1;
    },
    scenarioSaved: (state) => {
        state.scenarioDirty = false;
        state.scenarioSaves += 1;
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
    /** Tick a ship off or on in the results filter. */
    toggleResultShip: (state, action: PayloadAction<string>) => {
      state.resultShips = state.resultShips.includes(action.payload)
        ? state.resultShips.filter((name) => name !== action.payload)
        : [...state.resultShips, action.payload];
    },
    /** The same for a kind of result. */
    toggleResultCategory: (state, action: PayloadAction<string>) => {
      state.resultCategories = state.resultCategories.includes(action.payload)
        ? state.resultCategories.filter((kind) => kind !== action.payload)
        : [...state.resultCategories, action.payload];
    },
    /** Back to showing everything. */
    clearResultFilters: (state) => {
      state.resultShips = [];
      state.resultCategories = [];
    },
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
    setShowAlliesOnTargets: (state, action: PayloadAction<boolean>) => {
      state.showAlliesOnTargets = action.payload;
    },
    /** Back to the order the cards come in. */
    resetCardOrder: (state) => {
      state.cardOrder = [];
    },
    setFleetSelection: (state, action: PayloadAction<string[]>) => {
      state.fleetSelection = [...new Set(action.payload)];
    },
    /** Tick ships on, or off, keeping the rest. */
    tickShips: (state, action: PayloadAction<{ ships: string[]; on: boolean }>) => {
      const { ships, on } = action.payload;
      state.fleetSelection = on
        ? [...new Set([...state.fleetSelection, ...ships])]
        : state.fleetSelection.filter((name) => !ships.includes(name));
    },
    setFocusSquadron: (state, action: PayloadAction<{ team: string | null; name: string } | null>) => {
      state.focusSquadron = action.payload;
    },
    setSquadronMarkers: (state, action: PayloadAction<boolean>) => {
      state.squadronMarkers = action.payload;
    },
    resetServer: () => initialState,
  }
});

export const { setEntityToShow, setScenarioDirty, scenarioSaved, renamed, setProposedPlan, setShowResults, setEvents, removeEvent, clearMessageEvents, setCameraPos, setCameraQuaternion, setGravityWells, setJumpDistance, setShowRange, setComputerShipName, moveCard, resetCardOrder, setShowAlliesOnTargets, toggleResultShip, toggleResultCategory, clearResultFilters, setFleetSelection, tickShips, setFocusSquadron, setSquadronMarkers, resetServer } = uiSlice.actions;
export type UIReducer = ReturnType<typeof uiSlice.reducer>;
export default uiSlice.reducer;

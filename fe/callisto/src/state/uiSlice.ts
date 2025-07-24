import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { FlightPath } from "lib/flightPath";
import { Entity } from 'lib/entities';
import { Event } from 'components/space/Effects';

export interface UISlice {
    entityToShow: Entity | null;
    proposedPlan: FlightPath | null;
    showResults: boolean;
    events: Event[] | null;
    cameraPos: [number, number, number];
    gravityWells: boolean;
    jumpDistance: boolean;
    showRange: string | null;
    computerShipName: string | null;
}

const initialState: UISlice  = {
    entityToShow: null,
    proposedPlan: null,
    showResults: false,
    events: null,
    cameraPos: [-100, 0, 0],
    gravityWells: false,
    jumpDistance: false,
    showRange: null,
    computerShipName: null,
}

export const uiSlice = createSlice({
  name: 'ui',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setEntityToShow: (state, action: PayloadAction<Entity | null>) => {
        state.entityToShow = action.payload;
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
    setCameraPos: (state, action: PayloadAction<{x: number, y: number, z: number}>) => {
        state.cameraPos = [action.payload.x, action.payload.y, action.payload.z];
    },
    setGravityWells: (state, action: PayloadAction<boolean>) => { state.gravityWells = action.payload },
    setJumpDistance: (state, action: PayloadAction<boolean>) => { state.jumpDistance = action.payload },
    setShowRange: (state, action: PayloadAction<string | null>) => {
        state.showRange = action.payload;
    },
    setComputerShipName: (state, action: PayloadAction<string | null>) => {
        state.computerShipName = action.payload;
    },
    resetServer: () => initialState,
  }
});

export const { setEntityToShow, setProposedPlan, setShowResults, setEvents, setCameraPos, setGravityWells, setJumpDistance, setShowRange, setComputerShipName, resetServer } = uiSlice.actions;
export type UIReducer = ReturnType<typeof uiSlice.reducer>;
export default uiSlice.reducer;

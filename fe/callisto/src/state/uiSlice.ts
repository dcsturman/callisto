import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import * as THREE from 'three';
import { Entity, FlightPathResult, ViewControlParams } from 'lib/universal';
import { Effect } from 'components/space/Effects';

export interface UISlice {
    entityToShow: Entity | null;
    proposedPlan: FlightPathResult | null;
    showResults: boolean;
    events: Effect[] | null;
    cameraPos: THREE.Vector3;
    viewControls: ViewControlParams;
    showRange: string | null;
    computerShipName: string | null;
}

const initialState: UISlice  = {
    entityToShow: null,
    proposedPlan: null,
    showResults: false,
    events: null,
    cameraPos: new THREE.Vector3(-100, 0, 0),
    viewControls: {
        gravityWells: false,
        jumpDistance: false,
    },
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
    setProposedPlan: (state, action: PayloadAction<FlightPathResult | null>) => {
        state.proposedPlan = action.payload;
    },
    setShowResults: (state, action: PayloadAction<boolean>) => {
        state.showResults = action.payload;
    },
    setEvents: (state, action: PayloadAction<Effect[] | null>) => {
        state.events = action.payload;
    },
    setCameraPos: (state, action: PayloadAction<THREE.Vector3>) => {
        state.cameraPos = action.payload;
    },
    setViewControls: (state, action: PayloadAction<ViewControlParams>) => {
        state.viewControls = action.payload;
    },
    setShowRange: (state, action: PayloadAction<string | null>) => {
        state.showRange = action.payload;
    },
    setComputerShipName: (state, action: PayloadAction<string | null>) => {
        state.computerShipName = action.payload;
    },
  }
});

export const { setEntityToShow, setProposedPlan, setShowResults, setEvents, setCameraPos, setViewControls, setShowRange, setComputerShipName} = uiSlice.actions;

export default uiSlice.reducer;
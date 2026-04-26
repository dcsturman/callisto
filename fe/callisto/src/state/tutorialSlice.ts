import { createSlice, PayloadAction } from "@reduxjs/toolkit";

export enum AppMode {
  Game = "game",
  Tutorial = "tutorial",
  ScenarioBuilder = "scenario_builder",
}

export interface TutorialState {
  appMode: AppMode;
  stepIndex: number;
}

const initialState: TutorialState = {
  appMode: AppMode.Game,
  stepIndex: 0,
};

export const tutorialSlice = createSlice({
  name: "tutorial",
  initialState,
  reducers: {
    setAppMode: (state, action: PayloadAction<AppMode>) => {
      state.appMode = action.payload;
      if (action.payload !== AppMode.Tutorial) {
        state.stepIndex = 0;
      }
    },
    increment: (state) => {
      state.stepIndex++;
    },
    decrement: (state) => {
      state.stepIndex--;
    },
    reset: (state) => {
      state.stepIndex = 0;
    },
    resetServer: () => initialState,
  },
});

export type TutorialReducer = ReturnType<typeof tutorialSlice.reducer>;
export const { setAppMode, increment, decrement, reset, resetServer } =
  tutorialSlice.actions;

export default tutorialSlice.reducer;

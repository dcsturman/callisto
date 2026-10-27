import { createSlice, PayloadAction } from "@reduxjs/toolkit";

export enum AppMode {
  Game = "game",
  Tutorial = "tutorial",
  ScenarioBuilder = "scenario_builder",
}

export interface TutorialState {
  appMode: AppMode;
  /**
   * The lesson being played, by id. Kept when the app mode changes: leaving a
   * scenario switches the mode back to Game after the fact, and "Next lesson"
   * leaves one lesson and starts the next in the same moment.
   */
  lessonId: string | null;
  stepIndex: number;
  /** Open the scenario screen on the lesson list: we came back from a lesson. */
  showLessonList: boolean;
}

const initialState: TutorialState = {
  appMode: AppMode.Game,
  lessonId: null,
  stepIndex: 0,
  showLessonList: false,
};

export const tutorialSlice = createSlice({
  name: "tutorial",
  initialState,
  reducers: {
    setAppMode: (state, action: PayloadAction<AppMode>) => {
      // Only a change of mode starts the step count again: the mode is set
      // again on every reconnect, which must not restart a lesson.
      if (action.payload !== state.appMode && action.payload !== AppMode.Tutorial) {
        state.stepIndex = 0;
      }
      state.appMode = action.payload;
    },
    /** Start a lesson from its first step. */
    setLesson: (state, action: PayloadAction<string>) => {
      state.lessonId = action.payload;
      state.stepIndex = 0;
    },
    setShowLessonList: (state, action: PayloadAction<boolean>) => {
      state.showLessonList = action.payload;
    },
    increment: (state) => {
      state.stepIndex++;
    },
    decrement: (state) => {
      state.stepIndex = Math.max(0, state.stepIndex - 1);
    },
    reset: (state) => {
      state.stepIndex = 0;
    },
    resetServer: () => initialState,
  },
});

export type TutorialReducer = ReturnType<typeof tutorialSlice.reducer>;
export const { setAppMode, setLesson, setShowLessonList, increment, decrement, reset, resetServer } =
  tutorialSlice.actions;

export default tutorialSlice.reducer;

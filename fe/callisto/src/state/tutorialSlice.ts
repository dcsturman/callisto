import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface TutorialState {
    tutorialMode: boolean;
    runTutorial: boolean;
    stepIndex: number;
}

const initialState: TutorialState  = {
    tutorialMode: false,
    runTutorial: false,
    stepIndex: 0,
}

export const tutorialSlice = createSlice({
  name: 'tutorial',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setTutorialMode: (state, action: PayloadAction<boolean>) => {
        state.tutorialMode = action.payload;
    },
    setRunTutorial: (state, action: PayloadAction<boolean>) => {
        state.runTutorial = action.payload;
    },
    increment: state => {
        state.stepIndex++;
    },
    decrement: state => {
        state.stepIndex--;
    },
    reset: state => {
        state.stepIndex = 0;
    }
  }
});

export const { setTutorialMode, setRunTutorial, increment, decrement, reset } = tutorialSlice.actions;

export default tutorialSlice.reducer;
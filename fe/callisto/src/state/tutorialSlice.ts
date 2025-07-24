import { createSlice, PayloadAction } from '@reduxjs/toolkit';

export interface TutorialState {
    tutorialMode: boolean;
    stepIndex: number;
}

const initialState: TutorialState  = {
    tutorialMode: false,
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
    increment: state => {
        state.stepIndex++;
    },
    decrement: state => {
        state.stepIndex--;
    },
    reset: state => {
        state.stepIndex = 0;
    },
    resetServer: () => initialState,
  }
});

export type TutorialReducer = ReturnType<typeof tutorialSlice.reducer>;
export const { setTutorialMode, increment, decrement, reset, resetServer } = tutorialSlice.actions;

export default tutorialSlice.reducer;

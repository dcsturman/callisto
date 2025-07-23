import { configureStore, ThunkAction, Action } from "@reduxjs/toolkit";
import tutorialSlice from "./tutorialSlice";
import userSlice from "./userSlice";
import uiSlice from "./uiSlice";
import serverSlice from "./serverSlice";
import actionsSlice from "./actionsSlice";

const rootReducer = {
  tutorial: tutorialSlice,
  user: userSlice,
  ui: uiSlice,
  server: serverSlice,
  actions: actionsSlice,
}

export const store = configureStore({
  reducer: rootReducer,
})

export type AppDispatch = typeof store.dispatch
export type RootState = ReturnType<typeof store.getState>
export type AppThunk<ReturnType = void> = ThunkAction<
  ReturnType,
  RootState,
  unknown,
  Action<string>
>
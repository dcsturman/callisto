import { configureStore, ThunkAction, Action } from '@reduxjs/toolkit'
import tutorialSlice from './tutorial'
import userSlice from './userSlice'
import uiSlice from './uiSlice'
import serverSlice from './server'

const rootReducer = {
  tutorial: tutorialSlice,
  user: userSlice,
  ui: uiSlice,
  server: serverSlice,
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
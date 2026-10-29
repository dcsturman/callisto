import {configureStore, ThunkAction, Action, combineReducers} from "@reduxjs/toolkit";
import autoMergeLevel2 from "redux-persist/lib/stateReconciler/autoMergeLevel2";
import {persistStore, persistReducer, createTransform} from "redux-persist";
import sessionStorage from "redux-persist/lib/storage/session";
import tutorialSlice from "./tutorialSlice";
import userSlice from "./userSlice";
import uiSlice, { type UIReducer } from "./uiSlice";
import serverSlice from "./serverSlice";
import actionsSlice from "./actionsSlice";
import { resetServer as resetTutorial } from "./tutorialSlice";
import { resetServer as resetUI } from "./uiSlice";
import { resetServer as resetActions } from "./actionsSlice";

/**
 * The most results kept across a reload.
 *
 * The UI slice is written to session storage whenever it changes, and the
 * camera changes it every frame it moves. A big battle's round has thousands
 * of results, and serialising all of them on every frame of a camera move
 * stalled the view; a few thousand also come near the storage quota. A round
 * larger than this starts empty after a reload instead.
 */
export const MAX_PERSISTED_EVENTS = 300;

export const capEvents = createTransform(
  (ui: UIReducer) =>
    ui.events != null && ui.events.length > MAX_PERSISTED_EVENTS ? { ...ui, events: null } : ui,
  (ui: UIReducer) => ui,
  { whitelist: ["ui"] },
);

const sessionStorageConfig = {
  key: "root",
  blacklist: ["server"],
  storage: sessionStorage,
  stateReconciler: autoMergeLevel2,
  transforms: [capEvents],
};

//const storeConfig = (key: string) => { return {...sessionStorageConfig, key: key} };
// Persist everything but the server state (which will always come from the server)
// const rootReducer = combineReducers({
//   tutorial: persistReducer<TutorialReducer>(storeConfig("tutorial"), tutorialSlice),
//   user: persistReducer<UserReducer>(storeConfig("user"), userSlice),
//   ui: persistReducer<UIReducer>(storeConfig("ui"), uiSlice),
//   server: serverSlice,
//   actions: persistReducer<ActionsReducer>(storeConfig("actions"), actionsSlice),
// });

const rootReducer = combineReducers({
  tutorial: tutorialSlice,
  user: userSlice,
  ui: uiSlice,
  server: serverSlice,
  actions: actionsSlice,
});

export const store = configureStore({
  reducer: persistReducer<RootReducer>(sessionStorageConfig, rootReducer),
  devTools: process.env.NODE_ENV !== "production",
  middleware: (getDefaultMiddleware) =>
    getDefaultMiddleware({
      serializableCheck: {
        ignoredActions: ["persist/PERSIST", "persist/REHYDRATE"],
      },
    }),
});

export const persistor = persistStore(store);

export type AppDispatch = typeof store.dispatch;
export type RootState = ReturnType<typeof store.getState>;
export type RootReducer = ReturnType<typeof rootReducer>;
export type AppThunk<ReturnType = void> = ThunkAction<
  ReturnType,
  RootState,
  unknown,
  Action<string>
>;

export const resetState = () => (dispatch: AppDispatch) => {
  dispatch(resetTutorial());
  dispatch(resetUI());
  dispatch(resetActions());
};

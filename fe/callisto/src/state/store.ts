import {configureStore, ThunkAction, Action, combineReducers} from "@reduxjs/toolkit";
import autoMergeLevel2 from "redux-persist/lib/stateReconciler/autoMergeLevel2";
import {persistStore, persistReducer} from "redux-persist";
import sessionStorage from "redux-persist/lib/storage/session";
import tutorialSlice from "./tutorialSlice";
import userSlice from "./userSlice";
import uiSlice from "./uiSlice";
import serverSlice from "./serverSlice";
import actionsSlice from "./actionsSlice";
import { resetServer as resetTutorial } from "./tutorialSlice";
import { resetServer as resetUI } from "./uiSlice";
import { resetServer as resetActions } from "./actionsSlice";

const sessionStorageConfig = {
  key: "root",
  blacklist: ["server"],
  storage: sessionStorage,
  stateReconciler: autoMergeLevel2,
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

import { createSlice, PayloadAction } from "@reduxjs/toolkit";
import { ViewMode } from "lib/view";

export interface UserState {
  email: string | null;
  roles: ViewMode[];
  shipName: string | null;
  /** The team this player commands, as Fleet Command; null otherwise. */
  fleetTeam: string | null;
  joinedScenario: string | null;
}

const initialState: UserState = {
  email: null,
  roles: [ViewMode.General],
  shipName: null,
  fleetTeam: null,
  joinedScenario: null,
};

export const userSlice = createSlice({
  name: "user",
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setEmail: (state, action: PayloadAction<string | null>) => {
      state.email = action.payload;
    },
    setRoleShip: (state, action: PayloadAction<[ViewMode[], string | null]>) => {
      state.roles = action.payload[0];
      state.shipName = action.payload[1];
      state.fleetTeam = null;
    },
    /** Take Fleet Command of a team: no console of one's own, every ship on
     *  the team. */
    setFleetCommand: (state, action: PayloadAction<string>) => {
      state.roles = [ViewMode.FleetCommand];
      state.shipName = null;
      state.fleetTeam = action.payload;
    },
    setJoinedScenario: (state, action: PayloadAction<string | null>) => {
      state.joinedScenario = action.payload;
    },
  },
});

export const { setEmail, setRoleShip, setFleetCommand, setJoinedScenario } = userSlice.actions;
export type UserReducer = ReturnType<typeof userSlice.reducer>;
export default userSlice.reducer;

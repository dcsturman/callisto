import { createSlice, PayloadAction } from "@reduxjs/toolkit";
import { ViewMode } from "lib/view";

/** A squadron, named within its team. */
export interface SquadronSeat {
  team: string | null;
  name: string;
}

export interface UserState {
  email: string | null;
  roles: ViewMode[];
  shipName: string | null;
  /** The team this player commands, as Fleet Command; null otherwise. */
  fleetTeam: string | null;
  /** The squadron this player commands, as Squadron Command; null otherwise.
   *  Their own ship (`shipName`) is its flagship. */
  squadron: SquadronSeat | null;
  joinedScenario: string | null;
}

const initialState: UserState = {
  email: null,
  roles: [ViewMode.General],
  shipName: null,
  fleetTeam: null,
  squadron: null,
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
      state.squadron = null;
    },
    /** Take Fleet Command of a team: no console of one's own, every ship on
     *  the team. */
    setFleetCommand: (state, action: PayloadAction<string>) => {
      state.roles = [ViewMode.FleetCommand];
      state.shipName = null;
      state.fleetTeam = action.payload;
      state.squadron = null;
    },
    /** Take Squadron Command: every ship of one squadron, seated aboard its
     *  flagship. */
    setSquadronCommand: (state, action: PayloadAction<{ squadron: SquadronSeat; flagship: string }>) => {
      state.roles = [ViewMode.SquadronCommand];
      state.shipName = action.payload.flagship;
      state.fleetTeam = null;
      state.squadron = action.payload.squadron;
    },
    setJoinedScenario: (state, action: PayloadAction<string | null>) => {
      state.joinedScenario = action.payload;
    },
  },
});

export const { setEmail, setRoleShip, setFleetCommand, setSquadronCommand, setJoinedScenario } = userSlice.actions;
export type UserReducer = ReturnType<typeof userSlice.reducer>;
export default userSlice.reducer;

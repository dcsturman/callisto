import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { ViewMode } from 'lib/view';

export interface UserState {
    email: string | null;
    role: ViewMode;
    shipName: string | null;
    joinedScenario: string | null;
}

const initialState: UserState  = {
    email: null,
    role: ViewMode.General,
    shipName: null,
    joinedScenario: null,
}

export const userSlice = createSlice({
  name: 'user',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setEmail: (state, action: PayloadAction<string | null>) => {
        state.email = action.payload;
    },
    setRoleShip: (state, action: PayloadAction<[ViewMode, string | null]>) => {
        state.role = action.payload[0];
        state.shipName = action.payload[1];
    },
    setJoinedScenario: (state, action: PayloadAction<string | null>) => {
        state.joinedScenario = action.payload;
    },
  }
});

export const { setEmail, setRoleShip, setJoinedScenario } = userSlice.actions;
export type UserReducer = ReturnType<typeof userSlice.reducer>;
export default userSlice.reducer;
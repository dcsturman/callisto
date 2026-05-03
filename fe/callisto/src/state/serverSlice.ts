import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { ShipDesignTemplates } from 'lib/shipDesignTemplates';
import { EntityList, MetaData, defaultEntityList } from 'lib/entities';
import { UserList } from 'components/UserList';

// Pinned auth-banner codes mirrored from the backend. Any of these strings
// arriving as `{ "Error": "<code>" }` are routed to `authBanner` so the
// Authentication splash can show the right copy.
export type AuthBanner =
    | "NOT_AUTHORIZED"
    | "ALREADY_REGISTERED"
    | "REGISTRATION_FAILED"
    | "AUTH_FAILED";

export interface ServerState {
    authenticated: boolean;
    socketReady: boolean;
    entities: EntityList;
    templates: ShipDesignTemplates;
    users: UserList;
    activeScenarios: [string, string][];
    scenarioTemplates: [string, MetaData][];
    authBanner: AuthBanner | null;
}

const initialState: ServerState  = {
    authenticated: false,
    socketReady: false,
    entities: defaultEntityList(),
    templates: {},
    users: [],
    activeScenarios: [],
    scenarioTemplates: [],
    authBanner: null,
}

export const serverSlice = createSlice({
  name: 'server',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setAuthenticated: (state, action: PayloadAction<boolean>) => {
        state.authenticated = action.payload;
    },
    setSocketReady: (state, action: PayloadAction<boolean>) => {
        state.socketReady = action.payload;
    },
    setEntities: (state, action: PayloadAction<EntityList>) => {
        state.entities = action.payload;
    },
    setTemplates: (state, action: PayloadAction<ShipDesignTemplates>) => {
        state.templates = action.payload;
    },
    setUsers: (state, action: PayloadAction<UserList>) => {
        state.users = action.payload;
    },
    setScenarios: (state, action: PayloadAction<[[string, string][], [string, MetaData][]]> ) => {
        state.activeScenarios = action.payload[0];
        state.scenarioTemplates = action.payload[1];
    },
    setAuthBanner: (state, action: PayloadAction<AuthBanner | null>) => {
        state.authBanner = action.payload;
    }
  }
});

export const entitiesSelector = (state: { server: ServerState }) => state.server.entities;
export const templatesSelector = (state: { server: ServerState }) => state.server.templates;
export const authBannerSelector = (state: { server: ServerState }) => state.server.authBanner;

export const { setAuthenticated, setSocketReady, setEntities, setTemplates, setUsers, setScenarios, setAuthBanner } = serverSlice.actions;
export type ServerReducer = ReturnType<typeof serverSlice.reducer>;
export default serverSlice.reducer;

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

// One scenario file the server failed to parse. Surfaced post-login to the
// owner so they can see (and fix) their broken scenario instead of having
// it silently disappear from the picker.
export interface ScenarioLoadError {
    filename: string;
    error: string;
}

export interface ServerState {
    authenticated: boolean;
    socketReady: boolean;
    entities: EntityList;
    templates: ShipDesignTemplates;
    users: UserList;
    activeScenarios: [string, string][];
    scenarioTemplates: [string, MetaData][];
    authBanner: AuthBanner | null;
    scenarioLoadErrors: ScenarioLoadError[];
    /**
     * Whether this user may start games with AI crews. The AI costs money
     * every round, so the server allows it one user at a time; the client
     * offers the choice only to them.
     */
    aiAllowed: boolean;
    /**
     * Whether the game in play offers AI crews. When it does not, no AI
     * control is shown at all.
     */
    aiCrews: boolean;
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
    scenarioLoadErrors: [],
    aiAllowed: false,
    aiCrews: false,
}

export const serverSlice = createSlice({
  name: 'server',
  initialState,
  // The `reducers` field lets us define reducers and generate associated actions
  reducers: {
    setAuthenticated: (state, action: PayloadAction<boolean>) => {
        state.authenticated = action.payload;
    },
    setAiAllowed: (state, action: PayloadAction<boolean>) => {
        state.aiAllowed = action.payload;
    },
    setAiCrews: (state, action: PayloadAction<boolean>) => {
        state.aiCrews = action.payload;
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
    },
    setScenarioLoadErrors: (state, action: PayloadAction<ScenarioLoadError[]>) => {
        state.scenarioLoadErrors = action.payload;
    },
    dismissScenarioLoadErrors: (state) => {
        state.scenarioLoadErrors = [];
    }
  }
});

export const entitiesSelector = (state: { server: ServerState }) => state.server.entities;
export const templatesSelector = (state: { server: ServerState }) => state.server.templates;
export const authBannerSelector = (state: { server: ServerState }) => state.server.authBanner;
export const scenarioLoadErrorsSelector = (state: { server: ServerState }) => state.server.scenarioLoadErrors;

export const { setAuthenticated, setAiAllowed, setAiCrews, setSocketReady, setEntities, setTemplates, setUsers, setScenarios, setAuthBanner, setScenarioLoadErrors, dismissScenarioLoadErrors } = serverSlice.actions;
export const aiAllowedSelector = (state: { server: ServerState }) => state.server.aiAllowed;
export const aiCrewsSelector = (state: { server: ServerState }) => state.server.aiCrews;
export type ServerReducer = ReturnType<typeof serverSlice.reducer>;
export default serverSlice.reducer;

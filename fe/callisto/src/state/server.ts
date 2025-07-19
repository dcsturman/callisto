import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { EntityList, ShipDesignTemplates, MetaData } from 'lib/universal';
import { UserList } from 'components/UserList';
import { ActionType } from 'components/controls/Actions';

export interface ServerState {
    authenticated: boolean;
    socketReady: boolean;
    entities: EntityList;
    templates: ShipDesignTemplates;
    users: UserList;
    actions: ActionType;
    activeScenarios: [string, string][];
    scenarioTemplates: [string, MetaData][];
}

const initialState: ServerState  = {
    authenticated: false,
    socketReady: false,
    entities: new EntityList(),
    templates: {},
    users: [],
    actions: {},
    activeScenarios: [],
    scenarioTemplates: [],
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
    setActions: (state, action: PayloadAction<ActionType>) => {
        state.actions = action.payload;
    },
    setActiveScenarios: (state, action: PayloadAction<[string, string][]> ) => {
        state.activeScenarios = action.payload;
    },
    setScenarioTemplates: (state, action: PayloadAction<[string, MetaData][]> ) => {
        state.scenarioTemplates = action.payload;
    },
  }
});

export const { setAuthenticated, setSocketReady, setEntities, setTemplates, setUsers, setActions, setActiveScenarios, setScenarioTemplates } = serverSlice.actions;

export default serverSlice.reducer;
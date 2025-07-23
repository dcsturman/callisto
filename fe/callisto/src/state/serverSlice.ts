import { createSlice, PayloadAction } from '@reduxjs/toolkit';
import { ShipDesignTemplates } from 'lib/shipDesignTemplates';
import { EntityList, MetaData, defaultEntityList } from 'lib/entities';
import { UserList } from 'components/UserList';

export interface ServerState {
    authenticated: boolean;
    socketReady: boolean;
    entities: EntityList;
    templates: ShipDesignTemplates;
    users: UserList;
    activeScenarios: [string, string][];
    scenarioTemplates: [string, MetaData][];
}

const initialState: ServerState  = {
    authenticated: false,
    socketReady: false,
    entities: defaultEntityList(),
    templates: {},
    users: [],
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
    setScenarios: (state, action: PayloadAction<[[string, string][], [string, MetaData][]]> ) => {
        state.activeScenarios = action.payload[0];
        state.scenarioTemplates = action.payload[1];
    },
  }
});

export const entitiesSelector = (state: { server: ServerState }) => state.server.entities;
export const templatesSelector = (state: { server: ServerState }) => state.server.templates;

export const { setAuthenticated, setSocketReady, setEntities, setTemplates, setUsers, setScenarios } = serverSlice.actions;

export default serverSlice.reducer;
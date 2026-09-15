import {describe, expect, test, vi} from "vitest";

// The slice reports queued actions to the server as a side effect of most
// reducers. Boost bookkeeping is what is under test, so that one export is
// stubbed and the rest of the module is left real -- other imports reach into
// it too.
vi.mock("lib/serverManager", async (importOriginal) => ({
  ...(await importOriginal<typeof import("lib/serverManager")>()),
  updateActions: vi.fn(),
}));

import {actionsSlice, toggleBoost, dropBoosts, unfireWeapon, setSensorAction, setEngineerAction} from "state/actionsSlice";
import {DEFAULT_SENSOR_STATE} from "components/controls/Actions";

const reduce = actionsSlice.reducer;
const SHIP = "HMS Executor";

const boosted = (...targets: Parameters<typeof toggleBoost>[0]["target"][]) =>
  targets.reduce((state, target) => reduce(state, toggleBoost({shipName: SHIP, target})), {} as ReturnType<typeof reduce>);

const boostsOf = (state: ReturnType<typeof reduce>) => state[SHIP]?.leadershipCheck?.boosts ?? [];

describe("boosts follow the action they inspire", () => {
  test("deselecting Assist Gunner drops its boost, and asks the server to forget the check", () => {
    // The reported bug: the boost stayed pinned to an action that no longer existed.
    let state = boosted({kind: "AssistGunner", ship: SHIP});
    expect(boostsOf(state)).toHaveLength(1);

    state = reduce(state, dropBoosts({shipName: SHIP, kinds: ["AssistGunner"]}));
    expect(boostsOf(state)).toHaveLength(0);
    expect(state[SHIP].clearLeadership).toBe(true);
  });

  test("dropping one kind leaves the others alone", () => {
    let state = boosted({kind: "AssistGunner", ship: SHIP}, {kind: "Evade", ship: SHIP});
    state = reduce(state, dropBoosts({shipName: SHIP, kinds: ["AssistGunner"]}));
    expect(boostsOf(state)).toEqual([{kind: "Evade", ship: SHIP}]);
    expect(state[SHIP].clearLeadership).toBe(false);
  });

  test("unfiring a weapon drops only that mount's boost", () => {
    let state = boosted(
      {kind: "Fire", ship: SHIP, weapon_id: 0},
      {kind: "Fire", ship: SHIP, weapon_id: 1},
    );
    state = reduce(state, unfireWeapon({shipName: SHIP, weapon_id: 0}));
    expect(boostsOf(state)).toEqual([{kind: "Fire", ship: SHIP, weapon_id: 1}]);
  });

  test("clearing the sensor action drops the sensor boost but not a detection one", () => {
    // Detection is free and happens regardless, so its boost is not tied to
    // the sensor *action* and must survive.
    let state = boosted({kind: "Sensor", ship: SHIP}, {kind: "Detection", ship: SHIP, target: "Tai'ao"});
    state = reduce(state, setSensorAction({shipName: SHIP, action: DEFAULT_SENSOR_STATE}));
    expect(boostsOf(state)).toEqual([{kind: "Detection", ship: SHIP, target: "Tai'ao"}]);
  });

  test("clearing the engineer action drops the engineer boost", () => {
    let state = boosted({kind: "Engineer", ship: SHIP});
    state = reduce(state, setEngineerAction({shipName: SHIP, action: null}));
    expect(boostsOf(state)).toHaveLength(0);
    expect(state[SHIP].clearLeadership).toBe(true);
  });

  test("dropping a boost that is not there changes nothing", () => {
    const state = boosted({kind: "Evade", ship: SHIP});
    const after = reduce(state, dropBoosts({shipName: SHIP, kinds: ["AssistGunner"]}));
    expect(after).toEqual(state);
  });
});

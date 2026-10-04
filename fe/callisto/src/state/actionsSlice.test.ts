import {describe, expect, test, vi} from "vitest";

// The slice reports queued actions to the server as a side effect of most
// reducers. Boost bookkeeping is what is under test, so that one export is
// stubbed and the rest of the module is left real -- other imports reach into
// it too.
vi.mock("lib/serverManager", async (importOriginal) => ({
  ...(await importOriginal<typeof import("lib/serverManager")>()),
  updateActions: vi.fn(),
}));

import {actionsSlice, toggleBoost, dropBoosts, unfireWeapon, setSensorAction, setEngineerAction, setActions} from "state/actionsSlice";
import {DEFAULT_SENSOR_STATE, SensorAction} from "components/controls/Actions";

const reduce = actionsSlice.reducer;
const SHIP = "HMS Executor";

const boosted = (...targets: Parameters<typeof toggleBoost>[0]["target"][]) =>
  targets.reduce((state, target) => reduce(state, toggleBoost({shipName: SHIP, target})), {} as ReturnType<typeof reduce>);

const boostsOf = (state: ReturnType<typeof reduce>) => state[SHIP]?.leadershipCheck?.boosts ?? [];

describe("boosts reach the server", () => {
  // They used to live in the captain's browser until the end of the round, so
  // nobody else -- a player taking that seat, or the referee -- saw them.
  test("toggling a boost queues the actions with the server", async () => {
    const {updateActions} = await import("lib/serverManager");
    vi.mocked(updateActions).mockClear();

    reduce(undefined, toggleBoost({shipName: SHIP, target: {kind: "AssistGunner", ship: SHIP}}));
    expect(updateActions).toHaveBeenCalledTimes(1);
  });
});

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
    let state = boosted(
      {kind: "Sensor", ship: SHIP, operator: 0},
      {kind: "Detection", ship: SHIP, target: "Tai'ao"}
    );
    state = reduce(state, setSensorAction({shipName: SHIP, operator: 0, action: DEFAULT_SENSOR_STATE}));
    expect(boostsOf(state)).toEqual([{kind: "Detection", ship: SHIP, target: "Tai'ao"}]);
  });

  test("clearing the engineer action drops the engineer boost", () => {
    let state = boosted({kind: "Engineer", ship: SHIP, engineer: 0});
    state = reduce(state, setEngineerAction({shipName: SHIP, engineer: 0, action: null}));
    expect(boostsOf(state)).toHaveLength(0);
    expect(state[SHIP].clearLeadership).toBe(true);
  });

  test("dropping a boost that is not there changes nothing", () => {
    const state = boosted({kind: "Evade", ship: SHIP});
    const after = reduce(state, dropBoosts({shipName: SHIP, kinds: ["AssistGunner"]}));
    expect(after).toEqual(state);
  });
});

describe("one action each", () => {
  // Two operators are two people: withdrawing one's order leaves the other's.
  test("clearing one operator's action leaves a shipmate's standing", () => {
    let state = reduce(
      undefined,
      setSensorAction({
        shipName: SHIP,
        operator: 0,
        action: {action: SensorAction.JamMissiles, target: ""},
      })
    );
    state = reduce(
      state,
      setSensorAction({
        shipName: SHIP,
        operator: 1,
        action: {action: SensorAction.SensorLock, target: "Flayer"},
      })
    );
    state = reduce(state, setSensorAction({shipName: SHIP, operator: 0, action: DEFAULT_SENSOR_STATE}));

    expect(state[SHIP].sensors[0].action).toBe(SensorAction.None);
    expect(state[SHIP].sensors[1].action).toBe(SensorAction.SensorLock);
    // And the server is told whose order was withdrawn.
    expect(state[SHIP].clearSensors).toEqual([0]);
  });

  test("each engineer keeps their own job", () => {
    let state = reduce(
      undefined,
      setEngineerAction({shipName: SHIP, engineer: 0, action: {kind: "OverloadDrive"}})
    );
    state = reduce(
      state,
      setEngineerAction({shipName: SHIP, engineer: 1, action: {kind: "Repair", system: "Sensors"}})
    );

    expect(state[SHIP].engineers[0]).toEqual({kind: "OverloadDrive"});
    expect(state[SHIP].engineers[1]).toEqual({kind: "Repair", system: "Sensors"});
  });

  test("a boost follows the person whose action it inspires", () => {
    let state = reduce(
      undefined,
      setSensorAction({
        shipName: SHIP,
        operator: 1,
        action: {action: SensorAction.SensorLock, target: "Flayer"},
      })
    );
    state = reduce(state, toggleBoost({shipName: SHIP, target: {kind: "Sensor", ship: SHIP, operator: 1}}));
    expect(boostsOf(state)).toHaveLength(1);

    // Withdrawing the other operator's (absent) action leaves it alone.
    state = reduce(state, setSensorAction({shipName: SHIP, operator: 0, action: DEFAULT_SENSOR_STATE}));
    expect(boostsOf(state)).toHaveLength(1);

    // Withdrawing theirs takes it.
    state = reduce(state, setSensorAction({shipName: SHIP, operator: 1, action: DEFAULT_SENSOR_STATE}));
    expect(boostsOf(state)).toHaveLength(0);
  });
});

describe("two consoles on one ship agree", () => {
  // The referee has no ship of their own, so the boost checkboxes write
  // against whichever console is open; the guard used to name
  // `user.shipName`, which is null for them. Then holding the local list
  // outright was worse: a second console that had ticked one box kept
  // showing one box while the server, and the console that set them, had
  // three.
  test("a ship still mid-turn keeps a tick the server has not echoed yet", () => {
    let state = boosted(
      {kind: "Engineer", ship: SHIP, engineer: 0},
      {kind: "Fire", ship: SHIP, weapon_id: 0},
    );

    // The snapshot in flight knows only about the engineer's.
    const stale = {
      [SHIP]: {
        ...state[SHIP],
        leadershipCheck: {boosts: [{kind: "Engineer" as const, ship: SHIP, engineer: 0}]},
        pendingBoosts: [],
      },
    };
    state = reduce(state, setActions({parsed: stale, rolledShips: [SHIP]}));
    expect(boostsOf(state)).toHaveLength(2);
  });

  test("a console that ticked nothing takes the server's list whole", () => {
    // This browser has one boost and has heard back about it; the captain's
    // own console has since added two more.
    let state = boosted({kind: "Evade", ship: SHIP});
    const echoed = {
      [SHIP]: {...state[SHIP], leadershipCheck: {boosts: [{kind: "Evade" as const, ship: SHIP}]}, pendingBoosts: []},
    };
    state = reduce(state, setActions({parsed: echoed, rolledShips: [SHIP]}));

    const fromTheOtherConsole = {
      [SHIP]: {
        ...state[SHIP],
        leadershipCheck: {
          boosts: [
            {kind: "Evade" as const, ship: SHIP},
            {kind: "Sensor" as const, ship: SHIP, operator: 0},
            {kind: "Fire" as const, ship: SHIP, weapon_id: 0},
          ],
        },
        pendingBoosts: [],
      },
    };
    state = reduce(state, setActions({parsed: fromTheOtherConsole, rolledShips: [SHIP]}));
    expect(boostsOf(state)).toHaveLength(3);
  });

  test("a tick retires once the server says it back, and stops overriding", () => {
    let state = boosted({kind: "Fire", ship: SHIP, weapon_id: 0});
    const echoed = {
      [SHIP]: {
        ...state[SHIP],
        leadershipCheck: {boosts: [{kind: "Fire" as const, ship: SHIP, weapon_id: 0}]},
        pendingBoosts: [],
      },
    };
    state = reduce(state, setActions({parsed: echoed, rolledShips: [SHIP]}));
    expect(state[SHIP].pendingBoosts).toHaveLength(0);

    // Somebody else unticks it. Nothing of ours is waiting, so it goes.
    const withoutIt = {
      [SHIP]: {...state[SHIP], leadershipCheck: {boosts: []}, pendingBoosts: []},
    };
    state = reduce(state, setActions({parsed: withoutIt, rolledShips: [SHIP]}));
    expect(boostsOf(state)).toHaveLength(0);
  });

  test("once the round is over the server's list wins", () => {
    let state = boosted({kind: "Fire", ship: SHIP, weapon_id: 0});
    const after = {[SHIP]: {...state[SHIP], leadershipCheck: null, pendingBoosts: []}};
    state = reduce(state, setActions({parsed: after, rolledShips: []}));
    expect(boostsOf(state)).toHaveLength(0);
  });
});

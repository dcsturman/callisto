// @vitest-environment jsdom
import {describe, it, expect, beforeEach, afterEach} from "vitest";
import * as React from "react";
import {createRoot, Root} from "react-dom/client";
import {act} from "react";
import {Provider} from "react-redux";

(globalThis as unknown as {IS_REACT_ACT_ENVIRONMENT?: boolean}).IS_REACT_ACT_ENVIRONMENT = true;

import {Stations} from "components/controls/Stations";
import {store} from "state/store";
import {setRoleShip} from "state/userSlice";
import {setEntities} from "state/serverSlice";
import {ViewMode} from "lib/view";
import {Ship} from "lib/entities";
import {createCrew} from "components/controls/CrewBuilder";

let container: HTMLDivElement;
let root: Root;

const ship = {
  name: "HMS Executor",
  position: [0, 0, 0],
  velocity: [0, 0, 0],
  plan: [[[0, 0, 0], 50000], null],
  design: "HMS Executor",
  current_hull: 80,
  current_armor: 2,
  current_power: 260,
  current_maneuver: 6,
  current_jump: 2,
  current_fuel: 100,
  current_crew: 20,
  current_sensors: "Advanced",
  active_weapons: [],
  dodge_thrust: 0,
  assist_gunners: false,
  can_jump: false,
  sensor_locks: [],
  crew: {...createCrew(), pilot: 3, sensors: [4]},
} as unknown as Ship;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(
    setEntities({
      ships: [ship],
      missiles: [],
      planets: [],
      metadata: {name: "test", description: ""},
      filename: "",
    })
  );
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = (roles: ViewMode[], shipName: string | null) => {
  act(() => {
    store.dispatch(setRoleShip([roles, shipName]));
  });
  act(() => {
    root.render(
      <Provider store={store}>
        <Stations ship={ship} />
      </Provider>
    );
  });
};

const cardTitles = () =>
  Array.from(container.querySelectorAll(".station-card-title")).map((node) => node.textContent);

describe("station cards", () => {
  it("gives the referee every station", () => {
    render([ViewMode.General], null);
    expect(cardTitles()).toContain("Captain");
    expect(cardTitles()).toContain("Pilot");
    expect(cardTitles()).toContain("Sensors");
    expect(cardTitles()).toContain("Gunner");
    expect(cardTitles()).toContain("Engineer");
  });

  it("gives a specialist only their own", () => {
    render([ViewMode.Sensors], "HMS Executor");
    expect(cardTitles()).toEqual(["Sensors"]);
  });

  it("gives a player covering two seats both", () => {
    render([ViewMode.Pilot, ViewMode.Gunner], "HMS Executor");
    expect(cardTitles()).toEqual(["Pilot", "Gunner"]);
  });
});

// @vitest-environment jsdom
import {describe, it, expect, beforeEach, afterEach} from "vitest";
import * as React from "react";
import {createRoot, Root} from "react-dom/client";
import {act} from "react";
import {Provider} from "react-redux";

(globalThis as unknown as {IS_REACT_ACT_ENVIRONMENT?: boolean}).IS_REACT_ACT_ENVIRONMENT = true;

import {Stations, orderCards} from "components/controls/Stations";
import {ShipSummary} from "components/controls/ShipSummary";
import {store} from "state/store";
import {setRoleShip} from "state/userSlice";
import {setEntities} from "state/serverSlice";
import {setComputerShipName} from "state/uiSlice";
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
    expect(cardTitles()).toEqual(["Pilot", "Gunner", "Targets", "Incoming"]);
  });
});

describe("the Ships roster", () => {
  /**
   * A referee has no ship of their own, but when they open one to give it
   * orders they are working that hull -- so the roster measures ranges from
   * it, exactly as it would for the player who flies it.
   */
  it("measures range from the selected ship when the viewer has none", () => {
    store.dispatch(
      setEntities({
        ships: [ship, {...ship, name: "Quarry", position: [5_000_000, 0, 0]} as unknown as Ship],
        missiles: [],
        planets: [],
        metadata: {name: "test", description: ""},
        filename: "",
      })
    );
    store.dispatch(setRoleShip([[ViewMode.General], null]));
    store.dispatch(setComputerShipName("HMS Executor"));

    act(() => {
      root.render(
        <Provider store={store}>
          <ShipSummary />
        </Provider>
      );
    });

    const text = container.textContent ?? "";
    expect(text).toContain("range from HMS Executor");
    // And the other ship's distance is shown rather than a dash.
    expect(text).toMatch(/5,000|5000/);
  });

  it("shows no range column when nothing is selected and the viewer has no ship", () => {
    store.dispatch(setRoleShip([[ViewMode.General], null]));
    store.dispatch(setComputerShipName(null));

    act(() => {
      root.render(
        <Provider store={store}>
          <ShipSummary />
        </Provider>
      );
    });

    expect(container.textContent ?? "").not.toContain("range");
  });
});

describe("arranging the console", () => {
  /**
   * A player's arrangement is their own, and a card added in a later version
   * must still appear -- so anything the stored order has not seen sorts
   * last rather than being dropped.
   */
  it("puts cards in the stored order, with unknown ones after", () => {
    const cards = [{id: "pilot"}, {id: "gunner"}, {id: "targets"}, {id: "incoming"}];
    expect(orderCards(cards, ["incoming", "pilot"]).map((c) => c.id)).toEqual([
      "incoming",
      "pilot",
      "gunner",
      "targets",
    ]);
  });

  it("leaves the natural order alone when nothing has been arranged", () => {
    const cards = [{id: "pilot"}, {id: "gunner"}];
    expect(orderCards(cards, []).map((c) => c.id)).toEqual(["pilot", "gunner"]);
  });
});

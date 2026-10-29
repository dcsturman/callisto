// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sent = vi.hoisted(() => ({
  adds: [] as { name: string; position: number[]; design: string }[],
  squadrons: [] as [string[], string | null][],
}));
vi.mock("lib/serverManager", () => ({
  addShip: (ship: { name: string; position: number[]; design: string }) => sent.adds.push(ship),
  setSquadron: (ships: string[], squadron: string | null) => sent.squadrons.push([ships, squadron]),
  setOrders: () => {},
  setShipController: () => {},
}));

import { AddShip } from "components/controls/AddShip";
import { store } from "state/store";
import { setEntities, setTemplates } from "state/serverSlice";
import { Ship } from "lib/entities";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";

let container: HTMLDivElement;
let root: Root;

const fighter = {
  name: "Light Fighter",
  displacement: 10,
  hull: 4,
  armor: 2,
  maneuver: 6,
  jump: 0,
  power: 15,
  fuel: 1,
  crew: 1,
  sensors: "Improved",
  stealth: null,
  countermeasures: null,
  computer: 5,
  weapons: [{ kind: "Pulse", mount: "FixedMount" }],
  tl: 12,
};

const lanceShip = (name: string) =>
  ({ name, squadron: "Lance", design: "Light Fighter", position: [0, 0, 0], velocity: [0, 0, 0] }) as unknown as Ship;

beforeEach(() => {
  sent.adds.length = 0;
  sent.squadrons.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(setTemplates({ "Light Fighter": fighter } as unknown as ShipDesignTemplates));
  store.dispatch(
    setEntities({ ships: [], missiles: [], planets: [], metadata: { name: "test", description: "" }, filename: "" }),
  );
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const render = () =>
  act(() => {
    root.render(
      <Provider store={store}>
        <AddShip />
      </Provider>,
    );
  });

const setValue = (element: HTMLInputElement, value: string) =>
  act(() => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event("input", { bubbles: true }));
  });

const input = (selector: string) => container.querySelector(selector) as HTMLInputElement;

const fill = (count: string, squadron: string) => {
  act(() => (container.querySelector(".accordion-header") as HTMLElement).click());
  setValue(input('input[name="name"]'), "Lance");
  setValue(input('input[name="xpos"]'), "1000");
  setValue(input('input[aria-label="How many to add"]'), count);
  setValue(input('input[aria-label="Squadron for the new ships"]'), squadron);
  setValue(input('input[aria-label="Spacing in km"]'), "100");
};

const submit = () =>
  act(() => (container.querySelector("form#add-ship") as HTMLFormElement).requestSubmit());

describe("AddShip, several at once", () => {
  it("adds N of the design in rows, named in turn, and puts them in a squadron", () => {
    render();
    fill("6", "Lance");
    expect((container.querySelector('input[type="submit"]') as HTMLInputElement).value).toBe("Add 6");
    submit();
    expect(sent.adds.map((ship) => ship.name)).toEqual([
      "Lance 01",
      "Lance 02",
      "Lance 03",
      "Lance 04",
      "Lance 05",
      "Lance 06",
    ]);
    expect(sent.adds.every((ship) => ship.design === "Light Fighter")).toBe(true);
    // Five abreast 100 km apart around x = 1,000 km, then a second row.
    expect(sent.adds[0].position).toEqual([950_000, -200_000, 0]);
    expect(sent.adds[5].position).toEqual([1_050_000, 0, 0]);
    expect(sent.squadrons).toEqual([[sent.adds.map((ship) => ship.name), "Lance"]]);
  });

  it("refuses a batch that would overfill the squadron", () => {
    store.dispatch(
      setEntities({
        ships: Array.from({ length: 18 }, (_, i) => lanceShip(`Old ${i}`)),
        missiles: [],
        planets: [],
        metadata: { name: "test", description: "" },
        filename: "",
      }),
    );
    render();
    fill("3", "Lance");
    submit();
    expect(sent.adds).toEqual([]);
    expect(sent.squadrons).toEqual([]);
    expect(container.textContent).toContain("A squadron holds at most 20 ships; Lance would have 21.");
  });
});

// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// Queued orders go to the server; there is none here.
vi.mock("lib/serverManager", () => ({ updateActions: () => {} }));

// The store before the component, as in the app.
import { store } from "state/store";
import { EngineerTasks } from "components/controls/EngineerTasks";
import { resetServer, setEngineerAction } from "state/actionsSlice";
import { Ship, ShipSystem } from "lib/entities";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(resetServer());
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const crit = Array(11).fill(0);
crit[ShipSystem.Maneuver] = 1;
crit[ShipSystem.Crew] = 2;
crit[ShipSystem.Fuel] = 1;
const ship = { name: "Lyonesse II", crit_level: crit, can_jump: true } as unknown as Ship;

const menu = (engineer: number) => {
  act(() =>
    root.render(
      <Provider store={store}>
        <EngineerTasks ship={ship} engineer={engineer} />
      </Provider>,
    ),
  );
  return container.querySelector("select") as HTMLSelectElement;
};

describe("the engineer's menu", () => {
  it("offers repairs only of what can be repaired", () => {
    const options = Array.from(menu(0).options).map((option) => option.textContent ?? "");
    expect(options.filter((text) => text.startsWith("Repair"))).toEqual(["Repair Maneuver Drive (Crit Level 1)"]);
  });

  it("is locked on a power switch, for that engineer only", () => {
    act(() => {
      store.dispatch(
        setEngineerAction({
          shipName: ship.name,
          engineer: 1,
          action: { kind: "SetPower", system: { Weapon: 0 }, online: false },
        }),
      );
    });
    const busy = menu(1);
    expect(busy.disabled).toBe(true);
    expect(busy.selectedOptions[0].textContent).toBe("Power: weapon 1 off");
    expect(menu(0).disabled).toBe(false);
  });
});

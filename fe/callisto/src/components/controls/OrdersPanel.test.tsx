// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sent = vi.hoisted(() => ({ orders: [] as [string, string, string][] }));
vi.mock("lib/serverManager", () => ({
  setOrders: (ship: string, orders: string, scope = "Ship") => sent.orders.push([ship, orders, scope]),
}));

import { OrdersPanel } from "components/controls/OrdersPanel";
import { store } from "state/store";
import { setEntities } from "state/serverSlice";
import { setFleetCommand, setSquadronCommand } from "state/userSlice";
import { Ship } from "lib/entities";

let container: HTMLDivElement;
let root: Root;

const ship = (name: string, team: string, squadron?: string, extra: Partial<Ship> = {}) =>
  ({ name, team, squadron, design: "Light Fighter", ...extra }) as unknown as Ship;

beforeEach(() => {
  sent.orders.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(
    setEntities({
      ships: [
        ship("Lance 1", "Blue", "Lance", { fleet_orders: "Hold the gate.", squadron_orders: "Fighters first." }),
        ship("Lance 2", "Blue", "Lance", { fleet_orders: "Hold the gate.", squadron_orders: "Fighters first." }),
        ship("Hesperus", "Blue", undefined, { fleet_orders: "Hold the gate." }),
        ship("Spear 1", "Red"),
      ],
      missiles: [],
      planets: [],
      metadata: { name: "test", description: "" },
      filename: "",
    }),
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
        <OrdersPanel />
      </Provider>,
    );
  });

const setValue = (element: HTMLTextAreaElement | HTMLSelectElement, value: string) =>
  act(() => {
    const proto = element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLTextAreaElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  });

const options = () =>
  Array.from(container.querySelectorAll('select[aria-label="Give orders to"] option')).map((o) => o.textContent);

describe("OrdersPanel", () => {
  it("offers a fleet commander their fleet, squadrons and ships, with the orders in force", () => {
    store.dispatch(setFleetCommand("Blue"));
    render();
    expect(options()).toEqual(["Fleet ✓", "Squadron Lance ✓", "Hesperus", "Lance 1", "Lance 2"]);
    expect((container.querySelector("textarea") as HTMLTextAreaElement).value).toBe("Hold the gate.");
    expect(container.textContent).toContain("Squadron Lance: Fighters first.");
  });

  it("gives the fleet's orders through one of its ships", () => {
    store.dispatch(setFleetCommand("Blue"));
    render();
    setValue(container.querySelector("textarea") as HTMLTextAreaElement, "Destroy them.");
    act(() =>
      (Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Give orders") as HTMLElement).click(),
    );
    expect(sent.orders).toEqual([["Lance 1", "Destroy them.", "Fleet"]]);
  });

  it("lists ships given the same orders as one line, withdrawn together", () => {
    store.dispatch(
      setEntities({
        ships: ["Spear 01", "Spear 02", "Spear 03", "Spear 04"].map((name) =>
          ship(name, "Red", undefined, { orders: "Fighter screen." }),
        ),
        missiles: [],
        planets: [],
        metadata: { name: "test", description: "" },
        filename: "",
      }),
    );
    store.dispatch(setFleetCommand("Red"));
    render();
    const items = Array.from(container.querySelectorAll(".orders-panel-list li"));
    expect(items).toHaveLength(1);
    expect(items[0].textContent).toContain("Spear 01, Spear 02 and 2 more: Fighter screen.");
    act(() =>
      (Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Withdraw") as HTMLElement).click(),
    );
    expect(sent.orders).toHaveLength(4);
  });

  it("offers a squadron commander only their squadron and its ships", () => {
    store.dispatch(setSquadronCommand({ squadron: { team: "Blue", name: "Lance" }, flagship: "Lance 1" }));
    render();
    expect(options()).toEqual(["Squadron Lance ✓", "Lance 1", "Lance 2"]);
  });
});

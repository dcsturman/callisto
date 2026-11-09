// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sent = vi.hoisted(() => ({ states: [] as Record<string, { land: string | null; clearLand: boolean }>[] }));
vi.mock("lib/serverManager", () => ({
  updateActions: (state: unknown) => sent.states.push(JSON.parse(JSON.stringify(state))),
}));

// The store before the component: entering the slice ring through a slice
// leaves the store with no `actions` reducer.
import { store } from "state/store";
import { LandingControl } from "components/controls/LandingControl";
import { setEntities, setTemplates } from "state/serverSlice";
import { setRoleShip } from "state/userSlice";
import { resetServer, setLand } from "state/actionsSlice";
import { Ship } from "lib/entities";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";
import { ViewMode } from "lib/view";

let container: HTMLDivElement;
let root: Root;

const templates = {
  "Strike Carrier": {
    name: "Strike Carrier",
    displacement: 75_000,
    hull: 30_000,
    crew: 1_000,
    features: [{ name: "Docking space (10 t) x80", kind: { DockingSpace: { craft_tons: 10, count: 80 } } }],
    weapons: [],
  },
  "Light Fighter": { name: "Light Fighter", displacement: 10, hull: 4, crew: 1, weapons: [] },
} as unknown as ShipDesignTemplates;

const deck = { berths: 80, berths_free: 60, launch_per_round: 10, recover_per_round: 10, tube_tons: 100, deck_tons: 100 };

const ship = (name: string, extra: Partial<Ship> = {}) =>
  ({
    name,
    design: "Light Fighter",
    team: "Blue",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    plan: [[[0, 0, 0], 0], null],
    current_hull: 4,
    current_maneuver: 9,
    current_crew: 1,
    crew: { pilot: 2, sensors: [], engineers: [], gunnery: [], leadership: 0 },
    ...extra,
  }) as unknown as Ship;

const carrier = (name: string, extra: Partial<Ship> = {}) =>
  ship(name, { design: "Strike Carrier", current_hull: 30_000, current_crew: 1_000, flight_deck: deck, ...extra });

const gale = (extra: Partial<Ship> = {}) =>
  ship("Gale 01", { carrier: "Sirocco", squadron: "Wind Wing 1", position: [100_000, 0, 0], ...extra });

const show = (craft: Ship, ships: Ship[]) => {
  store.dispatch(
    setEntities({ ships: [craft, ...ships], missiles: [], planets: [], metadata: { name: "test", description: "" }, filename: "" }),
  );
  act(() => {
    root.render(
      <Provider store={store}>
        <LandingControl ship={craft} />
      </Provider>,
    );
  });
};

const click = (text: string) => {
  const button = [...container.querySelectorAll("button")].find((b) => b.textContent?.trim() === text);
  if (button == null) {
    throw new Error(`no button "${text}" in: ${container.textContent}`);
  }
  act(() => button.click());
};

beforeEach(() => {
  sent.states.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(setTemplates(templates));
  store.dispatch(resetServer());
  store.dispatch(setRoleShip([[ViewMode.Pilot], "Gale 01"]));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("LandingControl", () => {
  it("offers her side's carriers, home first, and never the enemy's", () => {
    show(gale(), [carrier("Mistral"), carrier("Sirocco"), carrier("Bastion", { team: "Red" }), ship("Gale 02")]);
    const options = [...container.querySelectorAll("option")].map((o) => o.textContent);
    expect(options).toEqual(["Sirocco (home) · 60 berths free", "Mistral · 60 berths free"]);
  });

  it("shows every gate passing and the check's odds for a craft at rest alongside", () => {
    show(gale(), [carrier("Sirocco")]);
    const gates = [...container.querySelectorAll(".landing-gates li")];
    expect(gates.every((g) => g.classList.contains("landing-gate-ok"))).toBe(true);
    expect(container.querySelector(".odds-dm")?.textContent).toBe("2D +2");
    expect(container.querySelector(".odds-dm")?.getAttribute("title")).toBe("pilot +2");
    expect(container.querySelector(".odds-chance")?.textContent).toBe("72%");
    expect(container.textContent).toContain("Land on Sirocco");
  });

  it("marks the gate that fails, and still lets the order be given", () => {
    show(gale({ position: [3_000_000, 0, 0] }), [carrier("Sirocco")]);
    const bad = [...container.querySelectorAll(".landing-gate-bad")].map((g) => g.textContent);
    expect(bad).toEqual(["✗ within Short at the end of the round (3,000 km)"]);
    expect(container.textContent).toContain("Land on Sirocco (not yet)");
  });

  it("sends Land, and withdraws it with ClearLand", () => {
    show(gale(), [carrier("Sirocco")]);
    click("Land on Sirocco");
    expect(sent.states[sent.states.length - 1]["Gale 01"].land).toBe("Sirocco");
    act(() => {
      store.dispatch(setLand({ shipName: "Gale 01", carrier: "Sirocco" }));
    });
    expect(container.textContent).toContain("Landing on Sirocco: stands until she is aboard");
    click("Withdraw");
    const last = sent.states[sent.states.length - 1]["Gale 01"];
    expect(last.land).toBeNull();
    expect(last.clearLand).toBe(true);
  });

  it("shows nothing for a craft with no carrier to land on", () => {
    show(gale(), [ship("Gale 02")]);
    expect(container.textContent).toBe("");
  });
});

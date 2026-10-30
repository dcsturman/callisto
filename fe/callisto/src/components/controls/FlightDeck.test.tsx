// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

// The slice reports every change to the server; what it reports is what the
// card is judged on.
const sent = vi.hoisted(() => ({ states: [] as Record<string, { launch: string[] | null; clearLaunch: boolean }>[] }));
vi.mock("lib/serverManager", () => ({
  updateActions: (state: unknown) => sent.states.push(JSON.parse(JSON.stringify(state))),
}));

// The store before the component: entering the slice ring through a slice
// leaves the store with no `actions` reducer.
import { store } from "state/store";
import { FlightDeck } from "components/controls/FlightDeck";
import { setEntities, setTemplates } from "state/serverSlice";
import { setRoleShip } from "state/userSlice";
import { resetServer, setLaunch } from "state/actionsSlice";
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
    features: [
      { name: "Docking space (10 t) x80", kind: { DockingSpace: { craft_tons: 10, count: 80 } } },
      { name: "Launch tube (100 t)", kind: { LaunchTube: { craft_tons: 100 } }, power: 100 },
      { name: "Recovery deck (100 t)", kind: { RecoveryDeck: { craft_tons: 100 } }, power: 100 },
    ],
    weapons: [],
  },
  "Light Fighter": { name: "Light Fighter", displacement: 10, hull: 4, weapons: [] },
} as unknown as ShipDesignTemplates;

const craft = (name: string, squadron: string, extra: Partial<Ship> = {}) =>
  ({
    name,
    design: "Light Fighter",
    team: "Blue",
    squadron,
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    plan: [[[0, 0, 0], 0], null],
    current_hull: 4,
    rounds_aboard: 2,
    crew: { pilot: 1, sensors: [], engineers: [], gunnery: [], leadership: 0 },
    ...extra,
  }) as unknown as Ship;

const wing = (prefix: string, n: number, squadron: string) =>
  Array.from({ length: n }, (_, i) => craft(`${prefix} ${String(i + 1).padStart(2, "0")}`, squadron));

const sirocco = (embarked: Ship[], extra: Partial<Ship> = {}) =>
  ({
    name: "Sirocco",
    design: "Strike Carrier",
    team: "Blue",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    plan: [[[0, 0, 0], 0], null],
    current_hull: 30_000,
    crew: { pilot: 1, sensors: [], engineers: [], gunnery: [], leadership: 2 },
    flight_deck: { berths: 80, berths_free: 80 - embarked.length, launch_per_round: 10, recover_per_round: 10, tube_tons: 100, deck_tons: 100 },
    embarked,
    ...extra,
  }) as unknown as Ship;

const show = (ship: Ship) => {
  store.dispatch(
    setEntities({ ships: [ship], missiles: [], planets: [], metadata: { name: "test", description: "" }, filename: "" }),
  );
  act(() => {
    root.render(
      <Provider store={store}>
        <FlightDeck ship={ship} />
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
  store.dispatch(setRoleShip([[ViewMode.Captain], "Sirocco"]));
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("FlightDeck", () => {
  it("lists the craft aboard by design and squadron, with the deck's rates", () => {
    show(sirocco([...wing("Gale", 12, "Wind Wing 1"), ...wing("Zephyr", 3, "Wind Wing 2")]));
    const rows = [...container.querySelectorAll(".flight-deck-group")].map((row) => row.textContent?.replace(/\s+/g, " ").trim());
    expect(rows[0]).toContain("Light Fighter · Wind Wing 1");
    expect(rows[0]).toContain("12");
    expect(rows[1]).toContain("Light Fighter · Wind Wing 2");
    expect(container.textContent).toContain("Berths 15 of 80 used, 65 free.");
    expect(container.textContent).toContain("Tubes launch 10 a round");
  });

  it("caps a launch at the tubes' rate and says the rest wait", () => {
    show(sirocco(wing("Gale", 12, "Wind Wing 1")));
    click("Pick wing");
    expect(container.textContent).toContain("12 picked: 10 launch this round, 2 wait for next round's order.");
    click("Launch");
    const last = sent.states[sent.states.length - 1];
    expect(last.Sirocco.launch).toHaveLength(12);
    expect(last.Sirocco.launch?.[0]).toBe("Gale 01");
  });

  it("leaves a craft already on her way out of the pick", () => {
    show(
      sirocco(wing("Gale", 4, "Wind Wing 1"), {
        flight_ops: [{ craft: "Gale 02", kind: "BerthLaunch", rounds_left: 2 }],
      }),
    );
    expect(container.textContent).toContain("Gale 02: leaving her berth, 2 rounds to go");
    click("Pick all");
    expect(container.textContent).toContain("3 picked");
  });

  it("withdraws a queued launch with an empty list", () => {
    show(sirocco(wing("Gale", 2, "Wind Wing 1")));
    act(() => {
      store.dispatch(setLaunch({ shipName: "Sirocco", craft: ["Gale 01"] }));
    });
    expect(container.textContent).toContain("Launch queued: 1 craft");
    click("Withdraw");
    const last = sent.states[sent.states.length - 1];
    expect(last.Sirocco.launch).toBeNull();
    expect(last.Sirocco.clearLaunch).toBe(true);
  });

  it("says when the carrier is held still and has no tube", () => {
    show(
      sirocco(wing("Gale", 2, "Wind Wing 1"), {
        pinned: true,
        flight_deck: { berths: 80, berths_free: 78, launch_per_round: 0, recover_per_round: 10, tube_tons: 0, deck_tons: 100 },
      }),
    );
    expect(container.textContent).toContain("Held still this round by a berth operation");
    expect(container.textContent).toContain("No working launch tube");
    click("Pick all");
    expect(container.textContent).toContain("all through the berths, D3 rounds each");
  });

  it("is read-only for someone who is not the captain", () => {
    store.dispatch(setRoleShip([[ViewMode.Gunner], "Sirocco"]));
    show(sirocco(wing("Gale", 2, "Wind Wing 1")));
    expect([...container.querySelectorAll("button")].map((b) => b.textContent?.trim())).not.toContain("Launch");
    expect(container.textContent).toContain("Wind Wing 1");
  });
});

// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { SquadronRoster } from "components/controls/SquadronRoster";
import { store } from "state/store";
import { setEntities, setTemplates } from "state/serverSlice";
import { setFleetSelection, setFocusSquadron } from "state/uiSlice";
import { Ship } from "lib/entities";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";

let container: HTMLDivElement;
let root: Root;

const ship = (name: string, team: string, squadron?: string, extra: Partial<Ship> = {}) =>
  ({
    name,
    team,
    squadron,
    design: "Light Fighter",
    current_hull: 10,
    position: [0, 0, 0],
    ...extra,
  }) as unknown as Ship;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(setTemplates({ "Light Fighter": { hull: 10 } } as unknown as ShipDesignTemplates));
  store.dispatch(setFleetSelection([]));
  store.dispatch(setFocusSquadron(null));
  store.dispatch(
    setEntities({
      ships: [
        ship("Lance 1", "Blue", "Lance", {
          controller: "AI",
          ai_report: { round: 2, status: "Ready", rationale: "Lance, step 1 of 2: engage Light Fighter. Closing." },
        }),
        ship("Lance 2", "Blue", "Lance", { current_hull: 0 }),
        ship("Hesperus", "Blue"),
        ship("Spear 1", "Red", "Spear"),
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

const render = (onlyTeam: string | null) =>
  act(() => {
    root.render(
      <Provider store={store}>
        <SquadronRoster onlyTeam={onlyTeam} />
      </Provider>,
    );
  });

const rows = () => Array.from(container.querySelectorAll("tr.roster-row")).map((row) => row.textContent ?? "");
const button = (label: string, within: Element = container) =>
  Array.from(within.querySelectorAll("button")).find((b) => b.textContent === label) as HTMLElement;

describe("SquadronRoster", () => {
  it("gives a fleet commander a line a squadron of their own side: ships, hull and the AI's step", () => {
    render("Blue");
    expect(rows()).toHaveLength(2);
    expect(rows()[0]).toContain("Lance");
    expect(rows()[0]).toContain("2");
    expect(rows()[0]).toContain("50%");
    expect(rows()[1]).toContain("No squadron");
    expect(container.textContent).toContain("step 1 of 2: engage Light Fighter");
    expect(container.textContent).not.toContain("Spear");
  });

  it("shows the referee every side", () => {
    render(null);
    expect(container.textContent).toContain("Spear");
    expect(container.querySelectorAll(".roster-team-name")).toHaveLength(2);
  });

  it("selects a squadron: ticks its ships and points the orders at it", () => {
    render("Blue");
    act(() => button("Select", container.querySelector("tr.roster-row") as Element).click());
    expect(store.getState().ui.fleetSelection).toEqual(["Lance 1", "Lance 2"]);
    expect(store.getState().ui.focusSquadron).toEqual({ team: "Blue", name: "Lance" });
    expect(container.querySelector("tr.roster-row-focus")?.textContent).toContain("Lance");
  });

  it("folds open to the squadron's ships, each one to tick", () => {
    render("Blue");
    expect(container.querySelectorAll("tr.roster-ship")).toHaveLength(0);
    act(() => (container.querySelector('button[aria-label="Show the ships of Lance"]') as HTMLElement).click());
    const ships = Array.from(container.querySelectorAll("tr.roster-ship"));
    expect(ships.map((row) => row.textContent)).toEqual([
      expect.stringContaining("Lance 1"),
      expect.stringContaining("Lance 2"),
    ]);
    act(() => (container.querySelector('input[aria-label="Tick Lance 2"]') as HTMLInputElement).click());
    expect(store.getState().ui.fleetSelection).toEqual(["Lance 2"]);
  });
});

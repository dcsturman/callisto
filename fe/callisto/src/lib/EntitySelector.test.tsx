// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { store } from "state/store";
import { setEntities } from "state/serverSlice";
import { Ship } from "lib/entities";

let container: HTMLDivElement;
let root: Root;

const ship = (name: string, extra: Partial<Ship> = {}) =>
  ({ name, team: "Blue", design: "Light Fighter", position: [0, 0, 0], velocity: [0, 0, 0], ...extra }) as unknown as Ship;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

describe("EntitySelector", () => {
  it("offers the ships on the plot and never the craft in a carrier's berths", () => {
    // A craft aboard travels inside her carrier, not in `ships`, so she
    // cannot be a target, a lock or a course. This holds that line.
    store.dispatch(
      setEntities({
        ships: [
          ship("Sirocco", { embarked: [ship("Gale 01", { carrier: "Sirocco" }), ship("Gale 02", { carrier: "Sirocco" })] }),
          ship("Hesperus", { team: "Red" }),
        ],
        missiles: [],
        planets: [],
        metadata: { name: "test", description: "" },
        filename: "",
      }),
    );
    act(() => {
      root.render(
        <Provider store={store}>
          <EntitySelector filter={[EntitySelectorType.Ship]} current={null} setChoice={() => {}} />
        </Provider>,
      );
    });
    const options = [...container.querySelectorAll("option")].map((o) => o.textContent);
    expect(options).toEqual(["", "Sirocco", "Hesperus"]);
  });
});

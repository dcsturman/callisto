// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { IncomingBoard } from "components/controls/IncomingBoard";
import { store } from "state/store";
import { setEntities } from "state/serverSlice";
import { Missile, Ship } from "lib/entities";

let container: HTMLDivElement;
let root: Root;

const ship = (name: string, x: number) =>
  ({ name, team: name === "Us" ? "Blue" : "Red", design: "Light Fighter", position: [x, 0, 0], velocity: [0, 0, 0] }) as unknown as Ship;

const salvo = (name: string, source: string, target: string, count?: number) =>
  ({
    name,
    source,
    target,
    position: [1_000, 0, 0],
    velocity: [0, 0, 0],
    acceleration: [0, 0, 0],
    burns: 10,
    ...(count === undefined ? {} : { count }),
  }) as unknown as Missile;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
});

const counts = (heading: string) => {
  const headings = [...container.querySelectorAll(".incoming-heading")];
  const at = headings.findIndex((h) => h.textContent === heading);
  const list = headings[at]?.nextElementSibling;
  return [...(list?.querySelectorAll(".incoming-count") ?? [])].map((c) => c.textContent);
};

describe("IncomingBoard", () => {
  it("counts the missiles in each salvo, not the salvoes", () => {
    const us = ship("Us", 0);
    store.dispatch(
      setEntities({
        ships: [us, ship("Them", 2_000)],
        // Twelve and a lone one inbound, landing together; seven of ours out.
        missiles: [
          salvo("Them::Us::0", "Them", "Us", 12),
          salvo("Them::Us::1", "Them", "Us"),
          salvo("Us::Them::2", "Us", "Them", 7),
        ],
        planets: [],
        actions: [],
      } as never),
    );
    act(() =>
      root.render(
        <Provider store={store}>
          <IncomingBoard ship={us} />
        </Provider>,
      ),
    );
    expect(counts("Salvoes inbound")).toEqual(["13"]);
    expect(counts("Ours in flight")).toEqual(["7"]);
  });
});

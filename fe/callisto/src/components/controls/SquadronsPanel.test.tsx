// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { Provider } from "react-redux";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const sent = vi.hoisted(() => ({ moves: [] as [string[], string | null][], renames: [] as string[][] }));
vi.mock("lib/serverManager", () => ({
  setSquadron: (ships: string[], squadron: string | null) => sent.moves.push([ships, squadron]),
  renameSquadron: (team: string | null, from: string, to: string) => sent.renames.push([team ?? "", from, to]),
}));

import { SquadronsPanel } from "components/controls/SquadronsPanel";
import { store } from "state/store";
import { setEntities } from "state/serverSlice";
import { Ship } from "lib/entities";

let container: HTMLDivElement;
let root: Root;

const ship = (name: string, team: string, squadron?: string) =>
  ({ name, team, squadron, design: "Light Fighter" }) as unknown as Ship;

beforeEach(() => {
  sent.moves.length = 0;
  sent.renames.length = 0;
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  store.dispatch(
    setEntities({
      ships: [ship("Lance 1", "Blue", "Lance"), ship("Lance 2", "Blue"), ship("Lance 3", "Blue"), ship("Spear 1", "Red")],
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
        <SquadronsPanel onlyTeam={onlyTeam} />
      </Provider>,
    );
  });

const open = () => act(() => (container.querySelector(".accordion-header") as HTMLElement).click());

const setValue = (element: HTMLInputElement | HTMLSelectElement, value: string) =>
  act(() => {
    const proto = element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, "value")?.set?.call(element, value);
    element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? "change" : "input", { bubbles: true }));
  });

describe("SquadronsPanel", () => {
  it("lists a fleet commander's own team, grouped by squadron", () => {
    render("Blue");
    open();
    const text = container.textContent ?? "";
    expect(text).toContain("Lance (1/20)");
    expect(text).toContain("No squadron (2)");
    expect(text).not.toContain("Spear 1");
  });

  it("moves every ticked ship at once, into a new squadron", () => {
    render("Blue");
    open();
    const looseHeader = container.querySelector('input[aria-label="Select every ship in no squadron"]') as HTMLInputElement;
    act(() => looseHeader.click());
    setValue(container.querySelector('select[aria-label="Move the selected ships to"]') as HTMLSelectElement, "\u0000new");
    setValue(container.querySelector('input[aria-label="New squadron\'s name"]') as HTMLInputElement, "Spear");
    act(() => (Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Move") as HTMLElement).click());
    expect(sent.moves).toEqual([[["Lance 2", "Lance 3"], "Spear"]]);
  });

  it("disbands a squadron", () => {
    render("Blue");
    open();
    act(() => (Array.from(container.querySelectorAll("button")).find((b) => b.textContent === "Disband") as HTMLElement).click());
    expect(sent.moves).toEqual([[["Lance 1"], null]]);
  });
});

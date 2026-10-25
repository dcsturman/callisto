// @vitest-environment jsdom
import { describe, it, expect, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";
import { ScenarioSide } from "lib/entities";
import {
  SideCrews,
  SideCrewChoice,
  aiShipsOf,
  initialCrews,
} from "components/scenarios/SideCrews";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

const SIDES: ScenarioSide[] = [
  {
    team: "Blue",
    ships: [
      { name: "Cicada", ai: false },
      { name: "Invigilator", ai: false },
    ],
  },
  { team: "Red", ships: [{ name: "HMS Executor", ai: true }] },
  {
    team: "Unaligned",
    ships: [
      { name: "Hulk", ai: true },
      { name: "Trader", ai: false },
    ],
  },
];

describe("initialCrews", () => {
  it("starts each side from what its file says", () => {
    expect(initialCrews(SIDES)).toEqual({
      Blue: { mode: "Human", aiShips: [] },
      Red: { mode: "AI", aiShips: ["HMS Executor"] },
      Unaligned: { mode: "Mixed", aiShips: ["Hulk"] },
    });
  });
});

describe("aiShipsOf", () => {
  it("hands a whole AI side to the AI, a mixed side's ticked ships, and no human side's", () => {
    const crews: SideCrewChoice = {
      Blue: { mode: "AI", aiShips: [] },
      Red: { mode: "Human", aiShips: ["HMS Executor"] },
      Unaligned: { mode: "Mixed", aiShips: ["Trader"] },
    };
    expect(aiShipsOf(SIDES, crews)).toEqual(["Cicada", "Invigilator", "Trader"]);
  });
});

describe("SideCrews", () => {
  let root: Root | null = null;
  let container: HTMLDivElement | null = null;
  afterEach(() => {
    act(() => root?.unmount());
    container?.remove();
  });

  const Harness: React.FC<{ onChange: (crews: SideCrewChoice) => void }> = ({ onChange }) => {
    const [crews, setCrews] = React.useState(initialCrews(SIDES));
    return (
      <SideCrews
        sides={SIDES}
        crews={crews}
        setCrews={(next) => {
          setCrews(next);
          onChange(next);
        }}
      />
    );
  };

  it("offers a ship-by-ship choice only on a mixed side", () => {
    let latest: SideCrewChoice = {};
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => root!.render(<Harness onChange={(crews) => (latest = crews)} />));

    const ships = () => Array.from(container!.querySelectorAll(".side-crews-ships input")).length;
    expect(ships()).toBe(2); // only Unaligned is mixed to begin with

    const blueMixed = container.querySelector<HTMLInputElement>('input[name="crew-Blue"][value="Mixed"]')!;
    act(() => blueMixed.click());
    expect(ships()).toBe(4);

    const invigilator = Array.from(container.querySelectorAll<HTMLLabelElement>(".side-crews-ships label"))
      .find((label) => label.textContent === "Invigilator")!
      .querySelector("input")!;
    act(() => invigilator.click());
    expect(aiShipsOf(SIDES, latest)).toEqual(["Invigilator", "HMS Executor", "Hulk"]);
  });
});

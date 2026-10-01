// @vitest-environment jsdom
import {describe, expect, it, beforeEach, afterEach, vi} from "vitest";
import * as React from "react";
import {createRoot, Root} from "react-dom/client";
import {act} from "react";

(globalThis as unknown as {IS_REACT_ACT_ENVIRONMENT?: boolean}).IS_REACT_ACT_ENVIRONMENT = true;

// The pickers talk to the server on change; the socket is not what is under
// test here.
vi.mock("lib/serverManager", async (importOriginal) => ({
  ...(await importOriginal<typeof import("lib/serverManager")>()),
  setCrewOnDuty: vi.fn(),
}));

import {EngineerPicker, SensorOperatorPicker} from "components/controls/OnDutyPicker";
import {createCrew, createEngineer} from "components/controls/CrewBuilder";
import {Ship} from "lib/entities";
import {setCrewOnDuty} from "lib/serverManager";

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  container.remove();
  vi.mocked(setCrewOnDuty).mockClear();
});

const shipWith = (crew: Partial<ReturnType<typeof createCrew>>): Ship =>
  ({name: "Executor", crew: {...createCrew(), ...crew}}) as Ship;

describe("choosing who is on a station", () => {
  it("says nothing when there is only one of them", () => {
    act(() => root.render(<SensorOperatorPicker ship={shipWith({sensors: [3]})} />));
    expect(container.querySelector("select")).toBeNull();

    act(() =>
      root.render(<EngineerPicker ship={shipWith({engineers: [createEngineer()]})} />)
    );
    expect(container.querySelector("select")).toBeNull();
  });

  it("offers each operator by position and skill", () => {
    act(() => root.render(<SensorOperatorPicker ship={shipWith({sensors: [3, 1]})} />));
    const options = Array.from(container.querySelectorAll("option"));
    expect(options.map((option) => option.textContent)).toEqual(["#1 · skill 3", "#2 · skill 1"]);
  });

  it("lists an engineer by the skills they are rated in", () => {
    const engineers = [
      {...createEngineer(), maneuver: 2, power: 3},
      {...createEngineer()},
    ];
    act(() => root.render(<EngineerPicker ship={shipWith({engineers})} />));
    const options = Array.from(container.querySelectorAll("option"));
    expect(options[0].textContent).toBe("#1 · M2 P3");
    expect(options[1].textContent).toBe("#2 · unrated");
  });

  it("sends the choice to the server", () => {
    act(() => root.render(<SensorOperatorPicker ship={shipWith({sensors: [3, 1]})} />));
    const select = container.querySelector("select")!;
    act(() => {
      select.value = "1";
      select.dispatchEvent(new Event("change", {bubbles: true}));
    });
    expect(setCrewOnDuty).toHaveBeenCalledWith("Executor", {sensor_operator: 1});
  });
});

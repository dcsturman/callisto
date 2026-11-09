// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";

(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { RoundSummary, RoundSummaryView } from "components/space/RoundSummary";

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
});

const fighter = (name: string, by: string) => ({
  ship: { name, team: "Red" as const },
  by: { name: by, team: "Blue" as const },
  squadron: "Bastion 1",
  tons: 10,
});

const SUMMARY: RoundSummary = {
  losses: [
    { ship: { name: "Cleon", team: "Red" }, by: { name: "Sirocco", team: "Blue" }, tons: 75000 },
    fighter("Bastion 1-01", "Intrepid"),
    fighter("Bastion 1-02", "Intrepid"),
    fighter("Bastion 1-03", "Dauntless"),
  ],
  damaged: [
    {
      ship: { name: "Hadrian", team: "Blue" },
      hull_lost: 758,
      hull: 30492,
      max_hull: 31250,
      criticals: 2,
      by: [{ name: "Cleon", team: "Red" }],
      tons: 50000,
    },
  ],
  fire: [
    { team: "Blue", attacks: 1776, hits: 803, hull: 13702, criticals: 377, intercepted: 306, kills: 4 },
    { team: "Red", attacks: 110, hits: 78, hull: 758, criticals: 2, intercepted: 0, kills: 0 },
  ],
};

describe("the round summary", () => {
  it("rolls small craft up by squadron, with who got them", () => {
    act(() => root.render(<RoundSummaryView summary={SUMMARY} onShip={() => {}} />));
    const text = container.textContent ?? "";
    expect(text).toContain("Red lost 4");
    expect(text).toContain("Bastion 1 · 3 craft");
    expect(text).toContain("Intrepid ×2");
    // The capital is her own row, by whoever took the most hull off her.
    expect(text).toMatch(/Cleon\s*by Sirocco/);
    expect(text).toContain("−758 · 30,492/31,250");
    expect(text).toContain("2 crits");
    expect(text).toContain("1,776");
    expect(text).toContain("45%");
  });

  it("hands a ship's name on, to open her lines", () => {
    const opened: string[] = [];
    act(() => root.render(<RoundSummaryView summary={SUMMARY} onShip={(name) => opened.push(name)} />));
    const hadrian = [...container.querySelectorAll("button.summary-ship")].find((b) => b.textContent === "Hadrian");
    act(() => (hadrian as HTMLButtonElement).click());
    expect(opened).toEqual(["Hadrian"]);
  });

  it("says so when nobody fired", () => {
    act(() => root.render(<RoundSummaryView summary={null} onShip={() => {}} />));
    expect(container.textContent).toContain("Nobody fired on anybody this round.");
  });
});

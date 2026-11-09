import { describe, expect, it } from "vitest";
import { isBox, selectableShips, shipsInBox } from "lib/boxSelect";
import { Ship } from "lib/entities";

const ship = (name: string, x: number, y: number, team?: string) =>
  ({ name, team, position: [x, y, 0] }) as unknown as Ship;

// A flat projection: metres straight to pixels, and anything with negative x
// "behind the camera".
const toScreen = (position: [number, number, number]): [number, number] | null =>
  position[0] < 0 ? null : [position[0], position[1]];

describe("shipsInBox", () => {
  const ships = [ship("In", 50, 50), ship("Edge", 100, 100), ship("Out", 150, 50), ship("Behind", -10, 50)];

  it("takes the ships whose markers are inside, whichever way the box was dragged", () => {
    expect(shipsInBox(ships, toScreen, { x0: 0, y0: 0, x1: 100, y1: 100 })).toEqual(["In", "Edge"]);
    expect(shipsInBox(ships, toScreen, { x0: 100, y0: 100, x1: 0, y1: 0 })).toEqual(["In", "Edge"]);
  });

  it("never takes a ship behind the camera", () => {
    expect(shipsInBox(ships, toScreen, { x0: -1000, y0: -1000, x1: 1000, y1: 1000 })).not.toContain("Behind");
  });
});

describe("isBox", () => {
  it("treats a twitch as a click, not a box", () => {
    expect(isBox({ x0: 10, y0: 10, x1: 12, y1: 40 })).toBe(false);
    expect(isBox({ x0: 10, y0: 10, x1: 30, y1: 40 })).toBe(true);
  });
});

describe("selectableShips", () => {
  const ships = [ship("Blue 1", 0, 0, "Blue"), ship("Red 1", 0, 0, "Red")];

  it("gives a fleet commander their own side, the referee everyone, others nothing", () => {
    expect(selectableShips(ships, false, "Blue").map((s) => s.name)).toEqual(["Blue 1"]);
    expect(selectableShips(ships, true, null)).toHaveLength(2);
    expect(selectableShips(ships, false, null)).toEqual([]);
  });
});

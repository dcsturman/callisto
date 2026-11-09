import { describe, expect, it } from "vitest";
import { missilePositions } from "lib/missileSwarm";
import { Missile } from "lib/entities";

describe("missilePositions", () => {
  it("puts each missile where the scene draws it", () => {
    const missile = (position: number[]) => ({ position }) as unknown as Missile;
    expect(Array.from(missilePositions([missile([1e6, 2e6, 0]), missile([0, -3e6, 5e5])]))).toEqual([
      1, 2, 0, 0, -3, 0.5,
    ]);
  });
});

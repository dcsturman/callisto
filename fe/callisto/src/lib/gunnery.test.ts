import {describe, expect, test} from "vitest";

import {Band, RANGE_MOD, maxBand, mountClass, reaches} from "lib/gunnery";
import {Weapon} from "lib/weapon";

const turret = (kind: string, count = 1): Weapon =>
  ({mount: {Turret: count}, guns: Array.from({length: count}, () => ({kind}))}) as unknown as Weapon;
const bay = (kind: string, size: string): Weapon =>
  ({mount: {Bay: size}, guns: [{kind}]}) as unknown as Weapon;
const barbette = (kind: string): Weapon => ({mount: "Barbette", guns: [{kind}]}) as unknown as Weapon;

describe("what a mount can reach", () => {
  test("a beam laser dies past Medium, a pulse laser past Long", () => {
    expect(reaches(turret("Beam"), "Medium")).toBe(true);
    expect(reaches(turret("Beam"), "Long")).toBe(false);
    expect(reaches(turret("Pulse"), "Long")).toBe(true);
    expect(reaches(turret("Pulse"), "Very Long")).toBe(false);
  });

  test("a particle barbette reaches Very Long, and a large bay to Distant", () => {
    expect(reaches(barbette("Particle"), "Very Long")).toBe(true);
    expect(reaches(barbette("Particle"), "Distant")).toBe(false);
    expect(reaches(bay("Particle", "Large"), "Distant")).toBe(true);
  });

  test("a launcher has no reach of its own: the salvo flies to the target", () => {
    expect(reaches(turret("Missile"), "Distant")).toBe(true);
    expect(maxBand(turret("Missile"))).toBeNull();
  });

  test("a mixed turret reaches as far as its longest gun", () => {
    const mixed = {mount: {Turret: 3}, guns: [{kind: "Beam"}, {kind: "Pulse"}, {kind: "Sand"}]} as unknown as Weapon;
    expect(maxBand(mixed)).toBe("Long");
    expect(reaches(mixed, "Long")).toBe(true);
  });
});

describe("range modifiers", () => {
  test("match the book's table", () => {
    const expected: [Band, number][] = [
      ["Short", 1],
      ["Medium", 0],
      ["Long", -2],
      ["Very Long", -4],
      ["Distant", -6],
    ];
    for (const [band, dm] of expected) {
      expect(RANGE_MOD[band]).toBe(dm);
    }
  });
});

describe("mount classes", () => {
  test("are the names the tables are keyed by", () => {
    expect(mountClass({Turret: 3} as never)).toBe("Turret");
    expect(mountClass({Bay: "Medium"} as never)).toBe("Medium");
    expect(mountClass("Barbette" as never)).toBe("Barbette");
  });
});

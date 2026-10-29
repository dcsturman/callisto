import { describe, it, expect } from "vitest";

import { expandGroups, groupWeapons } from "lib/hardpoints";
import { weaponMountPower } from "lib/power";
import { armament, compressedWeapons } from "lib/shipDesignTemplates";
import { Weapon, batteryLimit, createWeapon, fullSalvo, weaponToString } from "lib/weapon";

const beams = (count?: number): Weapon => ({ ...createWeapon("Beam", { Turret: 3 }), ...(count ? { count } : {}) });

describe("weapon batteries", () => {
  it("are named with their mounts", () => {
    expect(weaponToString(beams())).toBe("Triple Beam Turret");
    expect(weaponToString(beams(16))).toBe("Triple Beam Turret ×16 (battery)");
  });

  it("follow the hull's limit", () => {
    expect([500, 5_000, 75_000, 500_000].map(batteryLimit)).toEqual([4, 8, 16, 32]);
  });

  it("throw every launcher's salvo as one", () => {
    const bays = { ...createWeapon("Missile", { Bay: "Small" }), count: 16 };
    expect(fullSalvo(bays, "Missile")).toBe(16 * 12);
  });

  it("draw power for every mount", () => {
    const barbette = createWeapon("Particle", "Barbette");
    expect(weaponMountPower({ ...barbette, count: 8 })).toBe(8 * weaponMountPower(barbette));
  });

  it("give the gunner one button per battery, carrying its mounts", () => {
    const formed = [...Array(12).fill(beams(16)), beams(8)];
    const buttons = Object.values(compressedWeapons(formed));
    expect(buttons.map((b) => [b.total, b.mounts])).toEqual([
      [12, 16],
      [1, 8],
    ]);
  });

  it("read as one line of mounts in a design", () => {
    const formed = [...Array(12).fill(beams(16)), beams(8), createWeapon("Particle", "Barbette")];
    expect(armament(formed)).toEqual([
      { name: "Triple Beam Turret", mounts: 200, batteries: 13 },
      { name: "Particle Barbette", mounts: 1, batteries: 0 },
    ]);
  });

  it("survive the Add Ship editor: one row of mounts, split back by the hull", () => {
    const formed = [...Array(12).fill(beams(16)), beams(8)];
    const rows = groupWeapons(formed, Array(13).fill(2));
    expect(rows).toHaveLength(1);
    expect(rows[0].count).toBe(200);
    expect(rows[0].battery).toBe(true);
    const { weapons, gunnery } = expandGroups(rows, 75_000);
    expect(weapons).toEqual(formed);
    expect(gunnery).toEqual(Array(13).fill(2));
  });

  it("leave single mounts as they were", () => {
    const plain = [beams(), beams(), createWeapon("Particle", "Barbette")];
    const rows = groupWeapons(plain);
    expect(rows.map((r) => [r.count, r.battery])).toEqual([
      [2, undefined],
      [1, undefined],
    ]);
    expect(expandGroups(rows, 75_000).weapons).toEqual(plain);
  });
});

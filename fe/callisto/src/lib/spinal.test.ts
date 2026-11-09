import {describe, expect, it} from "vitest";

import {checkAllowance, mountClassOf, WeaponGroup} from "lib/hardpoints";
import {reaches} from "lib/gunnery";
import {weaponMountPower} from "lib/power";
import {armament} from "lib/shipDesignTemplates";
import {
  shortBandOf,
  spinalHardpoints,
  spinalLabel,
  spinalOf,
  spinalProblem,
  spinalRangeDm,
  spinalSizeDm,
  spinalTons,
} from "lib/spinal";
import {Weapon, mountToString, weaponToString} from "lib/weapon";

const meson = (multiple: number, tl?: number): Weapon => ({
  kind: "Meson",
  mount: {Spinal: tl == null ? multiple : {multiple, tl}},
});

describe("spinal mounts, as the server rules them (High Guard pp. 34-36)", () => {
  it("reads both wire shapes", () => {
    expect(spinalOf({Spinal: 3})).toEqual({multiple: 3});
    expect(spinalOf({Spinal: {multiple: 2, tl: 12}})).toEqual({multiple: 2, tl: 12});
    expect(spinalOf({Bay: "Large"})).toBeNull();
    expect(spinalOf("Barbette")).toBeNull();
  });

  it("gives the published designs their tonnage", () => {
    // Wind (HG p. 266), Ghalalk (p. 243), Azhanti (p. 258), Tigress (p. 284).
    expect(spinalTons("Meson", 3, 15)).toBe(18_000);
    expect(spinalTons("Particle", 1, 14)).toBe(2_800);
    expect(spinalTons("Particle", 2, 12)).toBe(6_300);
    expect(spinalTons("Meson", 11, 15)).toBe(66_000);
    expect(spinalTons("Meson", 1, 11)).toBeNull();
    expect(spinalHardpoints(6_375)).toBe(64);
  });

  it("refuses what the server would", () => {
    expect(spinalProblem("Meson", {multiple: 3}, 75_000, 15)).toBeNull();
    expect(spinalProblem("Meson", {multiple: 3}, 35_999, 15)).toMatch(/half/);
    expect(spinalProblem("Meson", {multiple: 13}, 1_000_000, 15)).toMatch(/largest/);
    expect(spinalProblem("Meson", {multiple: 1, tl: 11}, 100_000, 15)).toMatch(/TL12/);
    expect(spinalProblem("Beam", {multiple: 1}, 100_000, 15)).not.toBeNull();
  });

  it("takes the size and close-range DMs", () => {
    expect(spinalSizeDm(1_999)).toBeNull();
    expect(spinalSizeDm(2_000)).toBe(-8);
    expect(spinalSizeDm(5_000)).toBe(-8);
    expect(spinalSizeDm(5_001)).toBe(-4);
    expect(spinalSizeDm(10_000)).toBe(-4);
    expect(spinalSizeDm(10_001)).toBe(0);
    expect(shortBandOf(999)).toBe("Adjacent");
    expect(shortBandOf(10_000)).toBe("Close");
    expect(shortBandOf(10_001)).toBe("Short");
    expect(spinalRangeDm(9_999, "Adjacent")).toBe(-4);
    expect(spinalRangeDm(50_000, "Close")).toBe(-4);
    expect(spinalRangeDm(50_001, "Short")).toBe(-4);
    expect(spinalRangeDm(250_001, "Adjacent")).toBe(-10);
  });

  it("is named, reached and powered by its multiple", () => {
    expect(weaponToString(meson(3))).toBe("Meson Spinal Mount (3×)");
    expect(mountToString({Spinal: 3})).toBe("Spinal Mount (3×)");
    expect(spinalLabel("Meson", {multiple: 3}, 15)).toBe("Spinal – Meson (TL15) – 3×");
    expect(spinalLabel("Particle", {multiple: 2, tl: 12}, 14)).toBe("Spinal – Particle (TL12) – 2×");
    expect(mountClassOf({Spinal: 3})).toBe("Spinal");
    expect(reaches(meson(3), "Long")).toBe(true);
    expect(reaches(meson(3), "Very Long")).toBe(false);
    expect(weaponMountPower(meson(3))).toBe(3_000);
    expect(weaponMountPower({kind: "Railgun", mount: {Spinal: 2}})).toBe(1_000);
  });

  it("reads in a design summary as a record sheet writes it", () => {
    expect(armament([meson(3)], 15)).toEqual([
      {name: "Spinal – Meson (TL15) – 3× of 18,000 tons", mounts: 1, batteries: 0},
    ]);
  });

  it("costs one Hardpoint per 100 tons, and is checked against the hull", () => {
    const group = (multiple: number): WeaponGroup => ({
      count: 1,
      mount: {Spinal: multiple},
      kind: "Meson",
      gunnery: 0,
      modifiers: [],
    });
    const wind = checkAllowance([group(3)], 75_000, 15);
    expect(wind.used).toBe(180);
    expect(wind.rowProblems).toEqual([null]);
    const small = checkAllowance([group(3)], 30_000, 15);
    expect(small.rowProblems[0]).toMatch(/half/);
  });
});

import {describe, expect, test} from "vitest";

import {bySpare, canReload, describeLoad, mountCapacity, reloadLabel, spareGunners} from "lib/magazine";
import {Weapon} from "lib/weapon";

const turret = (...kinds: string[]): Weapon =>
  ({mount: {Turret: kinds.length}, guns: kinds.map((kind) => ({kind}))}) as unknown as Weapon;
const single = (kind: string, mount: unknown, count?: number): Weapon =>
  ({kind, mount, ...(count == null ? {} : {count})}) as unknown as Weapon;

// The same figures the server's `a_mount_holds_the_books_load` checks.
describe("what a mount holds", () => {
  test("a turret holds twelve missiles however many racks share it, four on a Firmpoint", () => {
    expect(mountCapacity(turret("Missile"), 200).missiles).toBe(12);
    expect(mountCapacity(turret("Missile", "Missile", "Missile"), 200).missiles).toBe(12);
    expect(mountCapacity(turret("Missile"), 50).missiles).toBe(4);
    expect(mountCapacity(single("Missile", "FixedMount"), 50).missiles).toBe(4);
  });

  test("sandcasters hold twenty canisters a turret, four on a Firmpoint", () => {
    expect(mountCapacity(turret("Sand", "Sand", "Sand"), 400).sand).toBe(20);
    expect(mountCapacity(turret("Sand"), 50).sand).toBe(4);
  });

  test("barbettes and bays hold the book's salvos", () => {
    expect(mountCapacity(single("Missile", "Barbette"), 1000).missiles).toBe(25);
    expect(mountCapacity(single("Torpedo", "Barbette"), 70).torpedoes).toBe(3);
    expect(mountCapacity(single("Missile", {Bay: "Small"}), 400).missiles).toBe(144);
    expect(mountCapacity(single("Missile", {Bay: "Large"}), 400).missiles).toBe(1440);
    expect(mountCapacity(single("Torpedo", {Bay: "Small"}), 400).torpedoes).toBe(36);
  });

  test("a mixed turret holds each kind's load, a battery all its mounts', a laser nothing", () => {
    expect(mountCapacity(turret("Missile", "Sand", "Beam"), 400)).toEqual({missiles: 12, torpedoes: 0, sand: 20});
    expect(mountCapacity({...turret("Missile", "Missile", "Missile"), count: 4}, 3000).missiles).toBe(48);
    expect(mountCapacity(turret("Beam"), 400)).toEqual({missiles: 0, torpedoes: 0, sand: 0});
  });
});

describe("reloading", () => {
  const full = {missiles: 12, torpedoes: 0, sand: 0};
  test("a mount can be reloaded below its load, with stores to do it", () => {
    expect(canReload(full, {missiles: 6, torpedoes: 0, sand: 0}, {missiles: 30, torpedoes: 0, sand: 0})).toBe(true);
    expect(canReload(full, full, {missiles: 30, torpedoes: 0, sand: 0})).toBe(false);
    expect(canReload(full, {missiles: 0, torpedoes: 0, sand: 0}, {missiles: 0, torpedoes: 0, sand: 40})).toBe(false);
  });

  test("the label says what is loaded and what is in stores", () => {
    const loaded = {missiles: 6, torpedoes: 0, sand: 0};
    expect(reloadLabel(full, loaded, {missiles: 30, torpedoes: 0, sand: 0})).toBe("Reload (6 of 12 loaded; 30 in stores)");
    expect(describeLoad(full, loaded)).toBe("6 of 12 missiles");
  });

  test("spare gunners are the gunners beyond the mounts, and go to Reloads in the order chosen", () => {
    expect(spareGunners([1, 1, 1, 1, 1], 4)).toBe(1);
    expect(spareGunners([1, 1], 4)).toBe(0);
    expect(spareGunners(undefined, 4)).toBe(0);
    expect(bySpare([3, 2], 1, 3)).toBe(true);
    expect(bySpare([3, 2], 1, 2)).toBe(false);
    expect(bySpare([3, 2], 1, 0)).toBe(false);
  });
});

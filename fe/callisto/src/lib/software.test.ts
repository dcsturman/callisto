import {describe, expect, test} from "vitest";

import {
  Software,
  alwaysRunning,
  bandwidthOf,
  bandwidthUsed,
  isLevelled,
  processingFor,
  softwareLabel,
} from "lib/software";

const sw = (kind: Software["kind"], level = 0): Software => ({kind, level});

describe("bandwidth", () => {
  test("costs come from the book's tables", () => {
    expect(bandwidthOf(sw("JumpControl", 2))).toBe(10);
    expect(bandwidthOf(sw("Evade", 3))).toBe(25);
    expect(bandwidthOf(sw("FireControl", 5))).toBe(25);
    expect(bandwidthOf(sw("AdvancedFireControl", 1))).toBe(15);
    expect(bandwidthOf(sw("BroadSpectrumEw"))).toBe(12);
  });

  test("free software is always running", () => {
    expect(alwaysRunning(sw("Library"))).toBe(true);
    expect(alwaysRunning(sw("Manoeuvre"))).toBe(true);
    expect(alwaysRunning(sw("Intellect"))).toBe(true);
    expect(alwaysRunning(sw("Evade", 1))).toBe(false);
  });

  test("HMS Executor cannot run everything she owns", () => {
    // Evade/1 + Fire Control/2 + Jump Control/2 is 30 on a Computer/20.
    const loadout = [sw("Evade", 1), sw("FireControl", 2), sw("JumpControl", 2)];
    expect(bandwidthUsed(loadout)).toBe(30);
    expect(bandwidthUsed(loadout.slice(0, 2))).toBe(20);
  });
});

describe("naming and levels", () => {
  test("levelled packages carry their level, the rest do not", () => {
    expect(softwareLabel(sw("FireControl", 2))).toBe("Fire Control/2");
    expect(softwareLabel(sw("Library"))).toBe("Library");
    expect(isLevelled("Evade")).toBe(true);
    expect(isLevelled("ScreenOptimiser")).toBe(false);
    // Virtual Gunner starts at /0, and the book writes it that way.
    expect(isLevelled("VirtualGunner")).toBe(true);
    expect(softwareLabel(sw("VirtualGunner", 0))).toBe("Virtual Gunner/0");
  });
});

describe("a /bis computer", () => {
  test("is worth +5 for Jump Control and nothing else", () => {
    expect(processingFor(5, true, "JumpControl")).toBe(10);
    expect(processingFor(5, true, "Evade")).toBe(5);
    expect(processingFor(5, false, "JumpControl")).toBe(5);
  });
});

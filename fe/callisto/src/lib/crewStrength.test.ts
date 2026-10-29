import {describe, expect, test} from "vitest";

import {Ship} from "lib/entities";
import {ShipDesignTemplate} from "lib/shipDesignTemplates";
import {
  crewDm,
  crewLossDm,
  crewPercent,
  hullPointsOptions,
  jamStations,
  liveMounts,
  radiationDm,
} from "lib/crewStrength";

describe("crew losses (High Guard p. 27)", () => {
  test("the DM applies unless the crew is above the threshold", () => {
    expect(crewLossDm(100, 100)).toBe(0);
    expect(crewLossDm(91, 100)).toBe(0);
    expect(crewLossDm(90, 100)).toBe(-1);
    expect(crewLossDm(71, 100)).toBe(-1);
    expect(crewLossDm(70, 100)).toBe(-2);
    expect(crewLossDm(26, 100)).toBe(-2);
    expect(crewLossDm(25, 100)).toBe(-4);
    expect(crewLossDm(0, 100)).toBe(-4);
    expect(crewLossDm(0, 0)).toBe(0);
  });

  test("the percentage rounds down and a crewless design is whole", () => {
    expect(crewPercent(930, 1000)).toBe(93);
    expect(crewPercent(2, 3)).toBe(66);
    expect(crewPercent(0, 0)).toBe(100);
  });
});

describe("radiation (High Guard p. 31)", () => {
  test("a step per 10% of hull, per 40% with shielding, which mesons ignore", () => {
    expect(radiationDm(1000, 99, 0, false)).toBe(0);
    expect(radiationDm(1000, 100, 0, false)).toBe(-1);
    expect(radiationDm(1000, 200, 150, false)).toBe(-3);
    expect(radiationDm(1000, 399, 0, true)).toBe(0);
    expect(radiationDm(1000, 400, 100, true)).toBe(-2);
    expect(radiationDm(0, 400, 100, true)).toBe(0);
  });

  test("the ship's DM names both causes", () => {
    const ship = {current_crew: 500, radiation_hull_lost: 400, active_weapons: []} as unknown as Ship;
    const design = {crew: 1000, hull: 1000, radiation_shielding: true} as ShipDesignTemplate;
    expect(crewDm(ship, design)).toEqual({losses: -2, radiation: -1, total: -3});
    expect(crewDm(ship, undefined).total).toBe(0);
  });
});

describe("size rules", () => {
  test("one sensor station per full 7,500 tons, never fewer than one", () => {
    expect(jamStations(400)).toBe(1);
    expect(jamStations(14_999)).toBe(1);
    expect(jamStations(15_000)).toBe(2);
    expect(jamStations(75_000)).toBe(10);
  });

  test("a battery's live mounts are what the criticals left, none when it is off", () => {
    const ship = {active_weapons: [true, true, false], mounts_down: [0, 3]} as unknown as Ship;
    expect(liveMounts(ship, 0, 12)).toBe(12);
    expect(liveMounts(ship, 1, 12)).toBe(9);
    expect(liveMounts(ship, 2, 12)).toBe(0);
  });

  test("hull points follow High Guard's bands and modifiers", () => {
    expect(hullPointsOptions(400)).toContain(160);
    expect(hullPointsOptions(1000)).toContain(440);
    expect(hullPointsOptions(600)).toContain(216);
    expect(hullPointsOptions(75_000)).toContain(41_250);
    expect(hullPointsOptions(500_000)).toContain(333_333);
    expect(hullPointsOptions(360)).not.toContain(160);
  });
});

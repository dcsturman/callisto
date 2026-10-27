import { describe, it, expect } from "vitest";
import { roundsToImpact } from "lib/missileEta";

const still = { position: [0, 0, 0] as [number, number, number], velocity: [0, 0, 0] as [number, number, number] };
const missile = (km: number, closingMs: number, burns?: number) => ({
  position: [0, km * 1000, 0] as [number, number, number],
  velocity: [0, -closingMs, 0] as [number, number, number],
  burns,
});

describe("roundsToImpact", () => {
  it("counts the missile's thrust: 6,000 km out at 2 km/s hits this round", () => {
    expect(roundsToImpact(missile(6000, 2000), still)).toBe(1);
  });

  it("takes three rounds from rest at 50,000 km", () => {
    // Half of 10 G times t squared reaches 50,000 km at about 1,010 s.
    expect(roundsToImpact(missile(50000, 0), still)).toBe(3);
  });

  it("coasts once the burns are spent", () => {
    // No thrust left: 6,000 km at 2 km/s is 3,000 s, nine rounds of 360 s.
    expect(roundsToImpact(missile(6000, 2000, 0), still)).toBe(9);
  });

  it("gives no answer for a missile that will not arrive", () => {
    // Opening fast, with no thrust left to turn it round.
    expect(roundsToImpact(missile(6000, -5000, 0), still)).toBeNull();
  });
});

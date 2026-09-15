import {describe, expect, test} from "vitest";
import {Acceleration} from "lib/entities";
import {G, TURN_IN_SECONDS} from "lib/universal";
import {formatKm, formatRange, projectPosition, rangeBetween} from "lib/range";

type Vec3 = [number, number, number];
const ZERO: Vec3 = [0, 0, 0];

/** A plan of one segment lasting longer than a turn, the ordinary case. */
const burn = (
  g: number,
  duration = 50_000,
): [Acceleration, Acceleration | null] => [[[g, 0, 0], duration], null];

const drift: [Acceleration, Acceleration | null] = [[[0, 0, 0], 50_000], null];

/** Only position and velocity and plan are read, so this is all a test needs. */
const ship = (
  position: Vec3,
  velocity: Vec3,
  plan: [Acceleration, Acceleration | null],
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
): any => ({name: "s", position, velocity, plan});

describe("projectPosition", () => {
  test("a drifting ship just keeps going", () => {
    const p = projectPosition([0, 0, 0], [100, 0, 0], drift, TURN_IN_SECONDS);
    expect(p[0]).toBeCloseTo(100 * TURN_IN_SECONDS, 6);
  });

  test("a burn from rest covers 1/2 a t squared", () => {
    const p = projectPosition([0, 0, 0], ZERO, burn(1), TURN_IN_SECONDS);
    expect(p[0]).toBeCloseTo(0.5 * G * TURN_IN_SECONDS ** 2, 3);
  });

  test("plan accelerations are in G, not m/s squared", () => {
    // The distinction is invisible unless you check the magnitude: the server
    // sends m/s^2 and `serverManager` divides by G on receipt.
    const oneG = projectPosition([0, 0, 0], ZERO, burn(1), TURN_IN_SECONDS)[0];
    const sixG = projectPosition([0, 0, 0], ZERO, burn(6), TURN_IN_SECONDS)[0];
    expect(sixG / oneG).toBeCloseTo(6, 6);
    expect(oneG).toBeGreaterThan(600_000); // ~636 km, not ~65 km
  });

  test("the second segment is flown once the first runs out", () => {
    // Half a turn accelerating, half decelerating: the ship ends the turn
    // back at the speed it started, having still moved forward.
    const half = TURN_IN_SECONDS / 2;
    const plan: [Acceleration, Acceleration | null] = [
      [[1, 0, 0], half],
      [[-1, 0, 0], half],
    ];
    const both = projectPosition([0, 0, 0], ZERO, plan, TURN_IN_SECONDS)[0];

    // Applying only the first segment for the whole turn -- the naive version
    // this test exists to rule out -- travels strictly further.
    const naive = projectPosition([0, 0, 0], ZERO, burn(1), TURN_IN_SECONDS)[0];
    expect(both).toBeLessThan(naive);
    // Accelerating then braking symmetrically ends at rest, and the two halves
    // cover the same ground: 2 * (1/2 G half^2). Exactly half the naive figure.
    expect(both).toBeCloseTo(G * half ** 2, 3);
    expect(both).toBeCloseTo(naive / 2, 3);
  });

  test("a plan shorter than the turn coasts out the remainder", () => {
    const half = TURN_IN_SECONDS / 2;
    const short = projectPosition([0, 0, 0], ZERO, burn(1, half), TURN_IN_SECONDS);
    // Accelerate for half a turn, then carry that velocity for the other half.
    const reached = 0.5 * G * half ** 2;
    expect(short[0]).toBeCloseTo(reached + G * half * half, 3);
  });
});

describe("rangeBetween", () => {
  test("both ships move, not just the observer", () => {
    // Head-on: each closes the gap, so the projection must halve twice as fast
    // as either one alone would manage.
    const a = ship([0, 0, 0], [1000, 0, 0], drift);
    const b = ship([10_000_000, 0, 0], [-1000, 0, 0], drift);
    const closed = rangeBetween(a, b);
    expect(closed.now).toBeCloseTo(10_000_000, 3);
    expect(closed.next).toBeCloseTo(10_000_000 - 2 * 1000 * TURN_IN_SECONDS, 3);

    // With the target held still, only the observer's half counts.
    const still = ship([10_000_000, 0, 0], ZERO, drift);
    expect(rangeBetween(a, still).next).toBeCloseTo(
      10_000_000 - 1000 * TURN_IN_SECONDS,
      3,
    );
  });

  test("a proposed plan overrides the observer's committed one", () => {
    const a = ship([0, 0, 0], ZERO, drift);
    const b = ship([10_000_000, 0, 0], ZERO, drift);
    expect(rangeBetween(a, b).next).toBeCloseTo(10_000_000, 3);

    // Same ships, but the pilot is considering a burn towards the target.
    const considering = rangeBetween(a, b, burn(2));
    expect(considering.now).toBeCloseTo(10_000_000, 3);
    expect(considering.next).toBeLessThan(10_000_000);
  });

  test("the target's own plan is always its committed one", () => {
    // A proposed plan is this client's intention and says nothing about the
    // other ship, which is choosing its burn at the same moment.
    const a = ship([0, 0, 0], ZERO, drift);
    const closing = ship([10_000_000, 0, 0], ZERO, burn(-2));
    const withTargetBurning = rangeBetween(a, closing);
    expect(withTargetBurning.next).toBeLessThan(10_000_000);
  });
});

describe("formatting", () => {
  test("close ranges round to the kilometre, distant ones to ten", () => {
    expect(formatKm(2_375_400)).toBe("2,375");
    expect(formatKm(48_912_000)).toBe("48,910");
  });

  test("a reading reads as now, then an estimate", () => {
    expect(formatRange({now: 2_375_000, next: 496_000})).toBe("2,375 → ~496");
  });

  test("closing and opening need no arrow: the second number says it", () => {
    expect(formatRange({now: 9_000_000, next: 4_000_000})).toContain("9,000 → ~4,000");
    expect(formatRange({now: 4_000_000, next: 9_000_000})).toContain("4,000 → ~9,000");
  });
});

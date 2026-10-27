import { describe, it, expect } from "vitest";
import * as THREE from "three";
import { bearing, burnAlong, sideView } from "lib/bearing";
import { SCALE } from "lib/universal";

type Vec = [number, number, number];

describe("bearing", () => {
  it("points from one place to the other, with the distance", () => {
    const result = bearing([0, 0, 0], [3000, 0, 4000]);
    expect(result?.metres).toBe(5000);
    expect(result?.direction).toEqual([0.6, 0, 0.8]);
  });

  it("has no direction between two things in the same place", () => {
    expect(bearing([1, 2, 3], [1, 2, 3])).toBeNull();
  });

  it("burns toward or away at the thrust given", () => {
    expect(burnAlong([0.6, 0, 0.8], 5, 1)).toEqual([3, 0, 4]);
    expect(burnAlong([0.6, 0, 0.8], 5, -1)).toEqual([-3, -0, -4]);
  });
});

describe("sideView", () => {
  const check = (a: Vec, b: Vec) => {
    const view = sideView(a, b, 75);
    const eye = new THREE.Vector3(...view.position);
    const start = new THREE.Vector3(...a).multiplyScalar(SCALE);
    const end = new THREE.Vector3(...b).multiplyScalar(SCALE);
    const middle = start.clone().add(end).multiplyScalar(0.5);
    const line = end.clone().sub(start);
    // Square to the line, through its middle.
    expect(Math.abs(eye.clone().sub(middle).normalize().dot(line.clone().normalize()))).toBeLessThan(1e-9);
    // Looking at the middle: the camera's forward (-z) points there.
    const forward = new THREE.Vector3(0, 0, -1).applyQuaternion(new THREE.Quaternion(...view.quaternion));
    expect(forward.dot(middle.clone().sub(eye).normalize())).toBeCloseTo(1, 9);
    // Both ends in view: each within half the 75-degree field of the forward line.
    for (const end_ of [start, end]) {
      const angle = forward.angleTo(end_.clone().sub(eye));
      expect(angle).toBeLessThan((75 * Math.PI) / 360);
    }
  };

  it("looks square at a level line", () => check([0, 0, 0], [20_000_000, 0, 0]));
  it("looks square at a slanted line", () => check([1e6, 2e6, -3e6], [-4e7, 1e7, 2e7]));
  it("looks square at a line straight up", () => check([0, 0, 0], [0, 5e7, 0]));
});

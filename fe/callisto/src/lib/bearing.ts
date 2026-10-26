import * as THREE from "three";
import { SCALE } from "lib/universal";

type Vec = [number, number, number];

/**
 * Which way `to` lies from `from`, as a unit vector, and how far, in metres.
 * Positions only: no velocity, no acceleration, nothing about where either
 * will be. Null if they are in the same place.
 */
export function bearing(from: Vec, to: Vec): { direction: Vec; metres: number } | null {
  const offset = to.map((value, i) => value - from[i]) as Vec;
  const metres = Math.hypot(...offset);
  if (metres === 0) return null;
  return { direction: offset.map((value) => value / metres) as Vec, metres };
}

/**
 * A burn straight toward (`sign` 1) or straight away from (`sign` -1) a
 * direction, at `thrust` G, rounded to hundredths for the Set Accel boxes.
 */
export function burnAlong(direction: Vec, thrust: number, sign: 1 | -1): Vec {
  return direction.map((value) => Math.round(value * thrust * sign * 100) / 100) as Vec;
}

/**
 * A camera looking side-on at the line between two points: from a point
 * square to the line through its middle, far enough back that both ends are
 * in view, with "up" as near the scene's up as the line allows. The way to
 * judge distances along that line by eye, such as whether a ship is past a
 * planet's 100-diameter limit, which any view along the line flattens.
 *
 * Positions in metres; the result is in scene units, as the camera uses.
 */
export function sideView(
  a: Vec,
  b: Vec,
  fovDegrees: number,
): { position: Vec; quaternion: [number, number, number, number] } {
  const start = new THREE.Vector3(...a).multiplyScalar(SCALE);
  const end = new THREE.Vector3(...b).multiplyScalar(SCALE);
  const middle = start.clone().add(end).multiplyScalar(0.5);
  const line = end.clone().sub(start);
  const length = line.length();

  // Square to the line, and level if the line allows it: across it from the
  // scene's up. A line straight up or down takes another side instead.
  const worldUp = new THREE.Vector3(0, 1, 0);
  let side = new THREE.Vector3().crossVectors(line, worldUp);
  if (side.lengthSq() < 1e-12 * Math.max(1, length * length)) {
    side = new THREE.Vector3().crossVectors(line, new THREE.Vector3(1, 0, 0));
  }
  if (side.lengthSq() === 0) side = new THREE.Vector3(0, 0, 1);
  side.normalize();

  // Back far enough that the whole line, and a margin, fills the view.
  const halfAngle = (fovDegrees * Math.PI) / 360;
  const distance = Math.max(0.05, (length * 0.65) / Math.tan(halfAngle));
  const eye = middle.clone().add(side.clone().multiplyScalar(distance));

  // A camera looks down its own -z: rotate it to face the middle.
  const up = new THREE.Vector3().crossVectors(side, line).normalize();
  const look = new THREE.Matrix4().lookAt(eye, middle, up.lengthSq() > 0 ? up : worldUp);
  const quaternion = new THREE.Quaternion().setFromRotationMatrix(look);
  return {
    position: [eye.x, eye.y, eye.z],
    quaternion: [quaternion.x, quaternion.y, quaternion.z, quaternion.w],
  };
}

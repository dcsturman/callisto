import {Acceleration, Ship} from "lib/entities";
import {G, TURN_IN_SECONDS} from "lib/universal";
import {vectorDistance} from "lib/Util";

type Vec3 = [number, number, number];

/**
 * Where a ship will be after `seconds` of flying its plan.
 *
 * A flight plan is up to two `(acceleration, duration)` segments, and a
 * computed bang-bang path routinely makes the first one shorter than a turn --
 * so this walks the segments rather than applying the first for the whole
 * duration. Getting that wrong would give the right answer most of the time and
 * a confidently wrong one exactly when a ship is doing something interesting.
 *
 * Plan accelerations arrive in G (converted on receipt in `serverManager`)
 * while positions and velocities are in metres, hence the multiply.
 */
export const projectPosition = (
  position: Vec3,
  velocity: Vec3,
  plan: [Acceleration, Acceleration | null],
  seconds: number,
): Vec3 => {
  const p: Vec3 = [...position];
  const v: Vec3 = [...velocity];
  let remaining = seconds;

  for (const segment of [plan[0], plan[1]]) {
    if (segment == null || remaining <= 0) {
      break;
    }
    const [accel, duration] = segment;
    const t = Math.min(duration, remaining);
    for (let i = 0; i < 3; i++) {
      const a = accel[i] * G;
      p[i] += v[i] * t + 0.5 * a * t * t;
      v[i] += a * t;
    }
    remaining -= t;
  }

  // A plan shorter than the turn means the ship coasts out the remainder.
  if (remaining > 0) {
    for (let i = 0; i < 3; i++) {
      p[i] += v[i] * remaining;
    }
  }
  return p;
};

/** Range now and one turn out, both in metres. */
export interface RangeReading {
  now: number;
  next: number;
}

/**
 * Range between two ships, now and projected a turn ahead.
 *
 * Both ships move: range is a relative quantity, so projecting only one of them
 * would be worse than not projecting at all.
 *
 * `observerPlan` lets the caller substitute the plan the pilot is currently
 * *considering* for the one the ship is committed to, so the number responds
 * while a burn is being dialled in -- which is the whole point of showing it.
 * The target's side can only ever use its last committed plan: moves resolve
 * simultaneously here, so nobody knows what the other ship is about to do. The
 * projection is therefore an estimate and should be presented as one.
 */
export const rangeBetween = (
  observer: Ship,
  target: Ship,
  observerPlan?: [Acceleration, Acceleration | null] | null,
): RangeReading => ({
  now: vectorDistance(observer.position, target.position),
  next: vectorDistance(
    projectPosition(
      observer.position,
      observer.velocity,
      observerPlan ?? observer.plan,
      TURN_IN_SECONDS,
    ),
    projectPosition(
      target.position,
      target.velocity,
      target.plan,
      TURN_IN_SECONDS,
    ),
  ),
});

/**
 * Metres as kilometres, rounded to something a person can read.
 *
 * Sub-kilometre precision is noise at these scales and makes a column ragged,
 * so close ranges round to the kilometre and distant ones to ten. The units are
 * named once in the column header rather than on every row.
 */
export const formatKm = (metres: number): string => {
  const km = metres / 1000;
  const rounded = km < 10000 ? Math.round(km) : Math.round(km / 10) * 10;
  return rounded.toLocaleString("en-US");
};

/**
 * A range reading as one cell: where it is, and where it will be.
 *
 * The projection carries a `~` because the other ship chooses its next burn at
 * the same moment you choose yours. Direction needs no arrow or colour -- a
 * second number smaller than the first is closing, and that reads at a glance
 * without competing with the team colours on the ship names.
 */
export const formatRange = (reading: RangeReading): string =>
  `${formatKm(reading.now)} → ~${formatKm(reading.next)}`;

/**
 * How to draw a sphere's true outline as a circle facing the camera.
 *
 * The obvious circle -- radius `r`, in the plane through the centre -- is not
 * the outline. The eye's tangent cone touches the sphere on a smaller circle,
 * radius `r·cos(θ)`, displaced `r·sin(θ)` toward the camera, where
 * `sin(θ) = r / d`. The great circle sits further back, so it projects
 * *smaller* than the real outline; a point just inside the sphere can then
 * land outside the drawn ring. Far away the two agree, but with the camera a
 * couple of radii out the great circle under-draws by around a tenth, which
 * was enough to put ships at 49,000 km visibly outside a 50,000 km ring.
 *
 * Returns the scale to apply to an `r`-radius circle and how far to push it
 * toward the camera, or `null` when the camera is inside the sphere and there
 * is no outline to draw.
 */
export const sphereSilhouette = (
  radius: number,
  cameraDistance: number,
): {scale: number; offset: number} | null => {
  if (cameraDistance <= radius) {
    return null;
  }
  const sin = radius / cameraDistance;
  return {
    scale: Math.sqrt(1 - sin * sin),
    offset: radius * sin,
  };
};


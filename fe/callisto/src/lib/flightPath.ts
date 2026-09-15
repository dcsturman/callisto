export type Acceleration = [[number, number, number], number];

/**
 * Which rung of the navigation computer's ladder produced a plan. Mirrors the
 * Rust `PathMode`. Absent from anything sent before the ladder existed, and
 * every such plan was a rendezvous.
 */
export type CourseMode = "Intercept" | "Pursuit" | "Shadow";

export interface FlightPath {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  plan: [Acceleration, Acceleration | null];
  mode?: CourseMode;
}

export const createFlightPath = (
  path: [number, number, number][],
  end_velocity: [number, number, number],
  plan: [Acceleration, Acceleration | null]
): FlightPath => {
  return {path, end_velocity, plan};
};
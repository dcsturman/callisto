export type Acceleration = [[number, number, number], number];

export interface FlightPath {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  plan: [Acceleration, Acceleration | null];
}

export const createFlightPath = (
  path: [number, number, number][],
  end_velocity: [number, number, number],
  plan: [Acceleration, Acceleration | null]
): FlightPath => {
  return {path, end_velocity, plan};
};
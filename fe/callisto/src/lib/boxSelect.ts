import { Ship } from "lib/entities";

/** A rectangle dragged on the screen, in client pixels, corners either way round. */
export interface Box {
  x0: number;
  y0: number;
  x1: number;
  y1: number;
}

/** Too small a drag to be a box: a click with a twitch in it. */
export const MIN_BOX = 4;

/** Whether a drag is big enough to count as a box. */
export const isBox = (box: Box): boolean => Math.abs(box.x1 - box.x0) >= MIN_BOX && Math.abs(box.y1 - box.y0) >= MIN_BOX;

/**
 * The ships of `ships` whose markers fall inside `box`.
 *
 * `toScreen` places a position (metres) on the screen in client pixels, or
 * gives null for one behind the camera, which no box can hold.
 */
export const shipsInBox = (
  ships: Ship[],
  toScreen: (position: [number, number, number]) => [number, number] | null,
  box: Box,
): string[] => {
  const left = Math.min(box.x0, box.x1);
  const right = Math.max(box.x0, box.x1);
  const top = Math.min(box.y0, box.y1);
  const bottom = Math.max(box.y0, box.y1);
  return ships
    .filter((ship) => {
      const at = toScreen(ship.position);
      return at != null && at[0] >= left && at[0] <= right && at[1] >= top && at[1] <= bottom;
    })
    .map((ship) => ship.name);
};

/**
 * The ships this player may tick: every ship for the referee, their own
 * team's for a fleet commander, and none for anyone else.
 */
export const selectableShips = (ships: Ship[], referee: boolean, fleetTeam: string | null): Ship[] =>
  fleetTeam != null ? ships.filter((ship) => ship.team === fleetTeam) : referee ? ships : [];

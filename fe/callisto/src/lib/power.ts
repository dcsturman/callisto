/**
 * Power arithmetic, kept apart from `entities.ts` because that module reaches
 * into the component tree and cannot be imported from a plain unit test.
 */

/** The parts of a ship this module needs. */
export interface PoweredShip {
  current_power: number;
  /** Power an ion hit is currently suppressing; absent when none is. */
  ion_power_loss?: number;
}

/**
 * Power the ship can actually use, after any ion suppression.
 *
 * The server tracks ion damage apart from `current_power` so that a repair
 * cannot undo an ion hit -- which means anything asking what a ship can do now
 * has to subtract it. Reading `current_power` directly shows a ship at full
 * power in the same round its power was drained.
 */
export const availablePower = (ship: PoweredShip): number =>
  Math.max(0, ship.current_power - (ship.ion_power_loss ?? 0));

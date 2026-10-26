import { G } from "lib/universal";
import { TURN_IN_SECONDS } from "lib/universal";

/** A missile's thrust, as the server flies it (missile.rs MAX_MISSILE_ACCELERATION). */
export const MISSILE_ACCELERATION = 10 * G;

/** Rounds of thrust a missile starts with (missile.rs DEFAULT_BURN). */
export const DEFAULT_BURNS = 10;

/** No answer past this many rounds: a missile that far off is not arriving. */
const HORIZON = 99;

type Vec = [number, number, number];

/**
 * In how many rounds a missile reaches its target: 1 is this round.
 *
 * The missile is taken to close along the line to its target, burning at
 * 10 G for the rounds of thrust it has left and coasting after, against a
 * target holding its course. The server re-aims every round, so this is an
 * estimate, but a fair one. Leaving out the thrust, as this used to, said a
 * salvo 6,000 km out at 2 km/s was nine rounds away when it would hit this
 * round. Null if it would not arrive within HORIZON rounds.
 */
export function roundsToImpact(
  missile: { position: Vec; velocity: Vec; burns?: number },
  target: { position: Vec; velocity: Vec },
): number | null {
  const toTarget = target.position.map((p, i) => p - missile.position[i]);
  const distance = Math.hypot(...toTarget);
  if (distance === 0) return 1;
  const along = toTarget.map((c) => c / distance);
  // Closing speed: the missile's speed relative to the target, along the line.
  let speed = missile.velocity.reduce((sum, v, i) => sum + (v - target.velocity[i]) * along[i], 0);
  const burns = missile.burns ?? DEFAULT_BURNS;
  let covered = 0;
  for (let round = 1; round <= HORIZON; round += 1) {
    const accel = round <= burns ? MISSILE_ACCELERATION : 0;
    covered += speed * TURN_IN_SECONDS + 0.5 * accel * TURN_IN_SECONDS * TURN_IN_SECONDS;
    speed += accel * TURN_IN_SECONDS;
    if (covered >= distance) return round;
  }
  return null;
}

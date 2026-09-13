import { Ship } from "./entities";

/**
 * Whether `observer` currently detects the ship named `targetName`.
 *
 * `contacts` is omitted from the wire when empty, so an absent list means the
 * ship detects nothing rather than "unknown" -- a blind ship is a real state
 * and has to be representable. A ship always "detects" itself.
 */
export const hasContact = (
  observer: Ship | null | undefined,
  target: Ship,
): boolean => {
  if (observer == null) {
    return true;
  }
  if (observer.name === target.name) {
    return true;
  }
  // A squadron shares a plot as a matter of course, so team-mates never have to
  // be found. Mirrors `Ship::detects` on the server -- if these two disagree,
  // the display says one thing and the rules do another.
  if (observer.team != null && observer.team === target.team) {
    return true;
  }
  return (observer.contacts ?? []).includes(target.name);
};

/**
 * Whether a ship should be shown and acted on as undetected.
 *
 * Gating is always relative to the ship whose console is open, not to the
 * logged-in player: in a single-ship view that is your ship, and in the GM's
 * all-ships view it is whichever ship is currently being commanded. Either way
 * the question "can this ship shoot that one" has the same answer.
 *
 * With no ship selected at all there is nobody to be blind, so nothing is
 * dimmed.
 */
export const isUndetected = (
  observer: Ship | null | undefined,
  target: Ship,
): boolean => observer != null && !hasContact(observer, target);

/**
 * The edge of Distant, in metres: 50,000 km.
 *
 * Mirrors the last entry of `RANGE_BANDS` on the server. Past it, High Guard
 * has everything as undifferentiated blips, so there is nothing to find and no
 * point offering to look.
 */
const DISTANT_METRES = 50_000_000;

/** Whether `target` is close enough to `observer` to be found at all. */
export const withinDistant = (observer: Ship, target: Ship): boolean => {
  const [ax, ay, az] = observer.position;
  const [bx, by, bz] = target.position;
  const dx = ax - bx;
  const dy = ay - by;
  const dz = az - bz;
  return Math.sqrt(dx * dx + dy * dy + dz * dz) <= DISTANT_METRES;
};

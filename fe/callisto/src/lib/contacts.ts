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

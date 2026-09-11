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
  targetName: string,
): boolean => {
  if (observer == null) {
    return true;
  }
  if (observer.name === targetName) {
    return true;
  }
  return (observer.contacts ?? []).includes(targetName);
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
  targetName: string,
): boolean => observer != null && !hasContact(observer, targetName);

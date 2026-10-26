import { ReactNode } from "react";
import { EntityList, Ship } from "lib/entities";
import { UISlice } from "state/uiSlice";
import { UserState } from "state/userSlice";

/** What a step's check can see: the game as the player sees it. */
export interface TutorialContext {
  entities: EntityList;
  ui: UISlice;
  user: UserState;
}

/**
 * One thing to read, or one thing to do.
 *
 * A step with a `task` and a `done` check waits for the player to do it, and
 * moves on by itself when they have. A step with neither is read and passed
 * with Next.
 */
export interface TutorialStep {
  id: string;
  title: string;
  /** The explanation. */
  body: ReactNode;
  /** What to do, shown as the instruction. */
  task?: ReactNode;
  /**
   * Whether the task is done. `start` is the game as it was when the step
   * began, for checks like "two more rounds" or "the plan has changed".
   */
  done?: (now: TutorialContext, start: TutorialContext) => boolean;
  /**
   * The `data-tutorial` markers of what to point at. Several, since one
   * thing can appear in more than one place.
   */
  target?: string[];
}

export interface Lesson {
  id: string;
  title: string;
  /** One line for the lesson list. */
  summary: string;
  /** The scenario file the lesson plays in. */
  scenario: string;
  steps: TutorialStep[];
}

export const findShipIn = (entities: EntityList, name: string): Ship | undefined =>
  entities.ships.find((ship) => ship.name === name);

/** Distance between two ships in km, or Infinity if either is missing. */
export const kmBetween = (entities: EntityList, a: string, b: string): number => {
  const [one, two] = [findShipIn(entities, a), findShipIn(entities, b)];
  if (one == null || two == null) return Infinity;
  return Math.hypot(...one.position.map((p, i) => p - two.position[i])) / 1000;
};

/** A ship's plan, as something to compare. */
export const planOf = (entities: EntityList, name: string): string =>
  JSON.stringify(findShipIn(entities, name)?.plan ?? null);

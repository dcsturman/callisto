import { ReactNode } from "react";
import { EntityList, Ship } from "lib/entities";
import { UISlice } from "state/uiSlice";
import { UserState } from "state/userSlice";
import { ActionsState } from "state/actionsSlice";

/** What a step's check can see: the game as the player sees it. */
export interface TutorialContext {
  entities: EntityList;
  ui: UISlice;
  user: UserState;
  /** The crew's queued orders for this round, by ship. */
  actions: ActionsState;
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
  /**
   * Whether this step belongs in the lesson at all, judged once from the game
   * as the lesson began. For what not every server has: AI crews need an AI
   * service, and without one the lesson teaches around them.
   */
  when?: (lessonStart: TutorialContext) => boolean;
}

export interface Lesson {
  id: string;
  title: string;
  /** One line for the lesson list. */
  summary: string;
  /** The scenario file the lesson plays in. */
  scenario: string;
  /** Played in the Scenario Builder rather than as a game. */
  builder?: boolean;
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

/** Whether a round has been played since the step began. */
export const roundPlayed = (now: TutorialContext, start: TutorialContext): boolean =>
  (now.entities.round ?? 1) > (start.entities.round ?? 1);

/** A ship's queued fire orders. */
export const fireOrders = (now: TutorialContext, ship: string) => now.actions[ship]?.fire ?? [];

/** How hurt a ship is: hull lost plus every level of every critical hit. */
export const hurt = (entities: EntityList, name: string): number => {
  const ship = findShipIn(entities, name);
  if (ship == null) return 0;
  return -ship.current_hull + (ship.crit_level ?? []).reduce((sum, level) => sum + level, 0);
};

/**
 * Whether a fire order the ship had when the step began has been called off.
 * An order that was never there does not count: that would pass the step
 * before the player has done anything.
 */
export const withdrawn = (
  now: TutorialContext,
  start: TutorialContext,
  ship: string,
  which: (order: ReturnType<typeof fireOrders>[number]) => boolean,
): boolean => fireOrders(start, ship).some(which) && !fireOrders(now, ship).some(which);

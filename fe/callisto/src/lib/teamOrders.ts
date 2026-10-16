import type { Ship } from "lib/entities";

/** A team's standing orders, as its ships carry them. */
export interface TeamOrders {
  /**
   * The team's orders: the text most of its ships carry, or "" when none has
   * any. Orders live on ships; a team's are just the ones its ships share.
   */
  orders: string;
  /** Ships carrying the team's orders, or none at all (they take the team's). */
  following: string[];
  /** Ships given orders of their own. */
  own: string[];
}

const textOf = (ship: Ship): string => (ship.orders ?? "").trim();

/** What a team's ships have been ordered, read as the team's orders plus exceptions. */
export const teamOrders = (ships: Ship[]): TeamOrders => {
  const counts = new Map<string, number>();
  ships.map(textOf).filter((text) => text !== "").forEach((text) => counts.set(text, (counts.get(text) ?? 0) + 1));
  // The most common wins; a tie goes to whichever came first.
  const orders = [...counts.entries()].reduce<[string, number]>(
    (best, entry) => (entry[1] > best[1] ? entry : best),
    ["", 0],
  )[0];
  const follows = (ship: Ship) => textOf(ship) === "" || textOf(ship) === orders;
  return {
    orders,
    following: ships.filter(follows).map((ship) => ship.name),
    own: ships.filter((ship) => !follows(ship)).map((ship) => ship.name),
  };
};

/**
 * The ships new team orders go to: every ship following the team's orders. A
 * ship given her own keeps them -- the escort told to stay by the flagship
 * should not lose that because the squadron's orders changed.
 */
export const shipsTakingTeamOrders = (ships: Ship[]): string[] => teamOrders(ships).following;

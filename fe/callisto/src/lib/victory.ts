import { Ship } from "lib/entities";

/** A side as it was last seen: its name, and whether it was trying to get away. */
export interface SideSeen {
  label: string;
  escaping: boolean;
}

/** Which side a ship is on: its team, or the ship alone if it has none. */
export const sideKey = (ship: Ship): string => ship.team ?? `ship:${ship.name}`;

/** What a side is called on the victory screen. */
const sideLabel = (ship: Ship): string => (ship.team != null ? `${ship.team} Team` : ship.name);

/**
 * Remember every side that has had a ship in the scenario, as last seen.
 * A side that has gone stays remembered: that is how its going is noticed.
 */
export const rememberSides = (seen: Map<string, SideSeen>, ships: Ship[]): Map<string, SideSeen> => {
  const next = new Map(seen);
  for (const ship of ships) {
    const prior = next.get(sideKey(ship));
    next.set(sideKey(ship), {
      label: sideLabel(ship),
      // A side is escaping if any of its ships was: they share a goal.
      escaping: ship.goal === "escape" || (prior?.escaping ?? false),
    });
  }
  return next;
};

/** How the scenario has ended, if it has. */
export interface Outcome {
  /** The one side with ships left, or null if none has any. */
  winner: string | null;
  /** The sides that have none left. */
  gone: SideSeen[];
  /** Whether every side that is gone was trying to get away: an escape. */
  escaped: boolean;
  /** The headline: "Red Team Victorious!", "Blue Team escaped!", ... */
  headline: string;
}

/**
 * Whether the scenario is over: at least two sides have fought it and no
 * more than one has a ship left. A side is gone when its every ship has
 * been destroyed or has jumped out; nothing on the wire tells those apart,
 * so a side whose goal was Escape is taken to have got away.
 */
export const outcome = (seen: Map<string, SideSeen>, ships: Ship[]): Outcome | null => {
  if (seen.size < 2) {
    return null;
  }
  const left = new Set(ships.map(sideKey));
  if (left.size > 1) {
    return null;
  }
  const [winnerKey] = [...left];
  const winner = winnerKey == null ? null : (seen.get(winnerKey)?.label ?? null);
  const gone = [...seen.entries()].filter(([key]) => !left.has(key)).map(([, side]) => side);
  const escaped = gone.length > 0 && gone.every((side) => side.escaping);
  const headline = escaped
    ? `${gone.map((side) => side.label).join(" and ")} escaped!`
    : winner != null
      ? `${winner} Victorious!`
      : "Mutual Destruction";
  return { winner, gone, escaped, headline };
};

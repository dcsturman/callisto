import { EntityList, Ship } from "lib/entities";
import { fromPost, postOf } from "lib/posts";

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
  /** What happened, when it is not that a side was wiped out. */
  detail?: string;
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

/**
 * Whether a scenario's round limit has ended it: its rounds are up (the
 * round now being played is past the limit). A Hold side with a ship still
 * at its post has held, and won; if none has, the post fell, and the sides
 * that were not holding have won. Null before the limit, or in a scenario
 * without one.
 */
export const timeUp = (entities: EntityList, seen: Map<string, SideSeen>): Outcome | null => {
  const limit = entities.metadata?.rounds;
  if (limit == null || (entities.round ?? 1) <= limit) {
    return null;
  }
  const holders = new Map<string, { label: string; where: string; held: boolean }>();
  for (const ship of entities.ships.filter((ship) => ship.goal === "hold")) {
    const key = sideKey(ship);
    const post = postOf(entities, ship);
    const prior = holders.get(key);
    holders.set(key, {
      label: seen.get(key)?.label ?? (ship.team != null ? `${ship.team} Team` : ship.name),
      where: prior?.where ?? post?.where ?? "its post",
      held: (prior?.held ?? false) || (fromPost(entities, ship)?.inside ?? false),
    });
  }
  const held = [...holders.entries()].find(([, side]) => side.held);
  if (held != null) {
    const [key, side] = held;
    const others = [...seen.entries()].filter(([other]) => other !== key).map(([, s]) => s);
    return {
      winner: side.label,
      gone: others,
      escaped: false,
      headline: `${side.label} held ${side.where}!`,
      detail: `The ${limit} rounds are up, and ${side.label} still holds ${side.where}.`,
    };
  }
  const attackers = [...seen.entries()].filter(([key]) => !holders.has(key)).map(([, s]) => s.label);
  const headline = attackers.length > 0 ? `${attackers.join(" and ")} Victorious!` : "Time is up";
  return {
    winner: attackers[0] ?? null,
    gone: [...holders.values()].map((side) => ({ label: side.label, escaping: false })),
    escaped: false,
    headline,
    detail: `The ${limit} rounds are up, and ${[...holders.values()].map((side) => side.label).join(" and ")} no longer ${holders.size === 1 ? "holds its" : "hold their"} post.`,
  };
};

import { Departure, EntityList, MetaData, Ship } from "lib/entities";
import { fromPost, postOf } from "lib/posts";

/** A side as it was last seen: its name, whether it was trying to get away, and its ships. */
export interface SideSeen {
  label: string;
  escaping: boolean;
  /** Every ship the side has had in the scenario. */
  ships: string[];
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
      ships: prior?.ships.includes(ship.name) ? prior.ships : [...(prior?.ships ?? []), ship.name],
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

/** What the escape rules read from the game: who has jumped out, and the scenario's own number. */
export interface EscapeContext {
  departed?: Departure[];
  metadata?: Pick<MetaData, "escape_needed">;
}

/** The side a departed ship was on, as `sideKey` names it. */
const departureKey = (departure: Departure): string => departure.team ?? `ship:${departure.name}`;

/** How many of a side's ships got away: jumped out safely, not misjumped. */
const gotAway = (key: string, context?: EscapeContext): number =>
  (context?.departed ?? []).filter((departure) => departure.safe && departureKey(departure) === key).length;

/**
 * How many of a side's ships must get away for it to have escaped: the
 * scenario's number if it sets one, otherwise more than two-thirds of them.
 */
export const escapeNeeded = (shipCount: number, context?: EscapeContext): number =>
  Math.min(shipCount, context?.metadata?.escape_needed ?? Math.floor((2 * shipCount) / 3) + 1);

/** "3 of 4 ships got away", for an escaping side. */
const awayLine = (side: SideSeen, got: number): string =>
  `${got} of ${side.ships.length} ${side.label} ${side.ships.length === 1 ? "ship" : "ships"} got away`;

/**
 * Whether the scenario is over: at least two sides have fought it and no
 * more than one has a ship left. A side is gone when its every ship has
 * been destroyed or has jumped out.
 *
 * An Escape side that is gone escaped if enough of its ships jumped out
 * safely (see `escapeNeeded`); short of that, the side that stopped them has
 * won. Without `context`, nobody is known to have jumped.
 */
export const outcome = (seen: Map<string, SideSeen>, ships: Ship[], context?: EscapeContext): Outcome | null => {
  if (seen.size < 2) {
    return null;
  }
  const left = new Set(ships.map(sideKey));
  if (left.size > 1) {
    return null;
  }
  const [winnerKey] = [...left];
  const winner = winnerKey == null ? null : (seen.get(winnerKey)?.label ?? null);
  const goneEntries = [...seen.entries()].filter(([key]) => !left.has(key));
  const gone = goneEntries.map(([, side]) => side);
  const escapes = goneEntries
    .filter(([, side]) => side.escaping)
    .map(([key, side]) => {
      const got = gotAway(key, context);
      return { side, got, made: got >= escapeNeeded(side.ships.length, context) };
    });
  const escaped = gone.length > 0 && escapes.length === gone.length && escapes.every((escape) => escape.made);
  if (escaped) {
    return {
      winner,
      gone,
      escaped,
      headline: `${gone.map((side) => side.label).join(" and ")} escaped!`,
      detail: `${escapes.map((escape) => awayLine(escape.side, escape.got)).join("; ")}.`,
    };
  }
  const failed = escapes.filter((escape) => !escape.made);
  return {
    winner,
    gone,
    escaped,
    headline: winner != null ? `${winner} Victorious!` : "Mutual Destruction",
    detail:
      failed.length === 0
        ? undefined
        : `Only ${failed
            .map(
              (escape) =>
                `${awayLine(escape.side, escape.got)}, of the ${escapeNeeded(escape.side.ships.length, context)} needed`,
            )
            .join("; ")}.`,
  };
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
  // An Escape side still in the fight when time runs out: escaped if enough
  // have got away; a draw if enough are still alive, counting those that got
  // away, since the attackers did not stop them and they did not get out;
  // otherwise the attackers have won.
  const left = new Set(entities.ships.map(sideKey));
  const escaping = [...seen.entries()].find(([key, side]) => side.escaping && left.has(key));
  if (escaping != null) {
    const [key, side] = escaping;
    const got = gotAway(key, entities);
    const inFight = entities.ships.filter((ship) => sideKey(ship) === key).length;
    const needed = escapeNeeded(side.ships.length, entities);
    const others = [...seen.entries()].filter(([other]) => other !== key).map(([, s]) => s);
    const when = `The ${limit} rounds are up: ${awayLine(side, got)}, ${inFight} still in the fight, and ${needed} needed to escape.`;
    if (got >= needed) {
      return { winner: side.label, gone: others, escaped: true, headline: `${side.label} escaped!`, detail: when };
    }
    if (got + inFight >= needed) {
      return { winner: null, gone: [], escaped: false, headline: "A draw", detail: when };
    }
    const attackers = others.map((s) => s.label);
    return {
      winner: attackers[0] ?? null,
      gone: [side],
      escaped: false,
      headline: attackers.length > 0 ? `${attackers.join(" and ")} Victorious!` : "Time is up",
      detail: when,
    };
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
    gone: [...holders.values()].map((side) => ({ label: side.label, escaping: false, ships: [] })),
    escaped: false,
    headline,
    detail: `The ${limit} rounds are up, and ${[...holders.values()].map((side) => side.label).join(" and ")} no longer ${holders.size === 1 ? "holds its" : "hold their"} post.`,
  };
};

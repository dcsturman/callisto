import {Missile, Ship} from "lib/entities";
import {Team} from "lib/teams";
import {TURN_IN_SECONDS} from "lib/universal";
import {vectorDistance} from "lib/Util";
import {hasContact} from "lib/contacts";

/**
 * What another ship is doing to us: watching, or aiming.
 *
 * A lock is the serious one -- it is DM+2 on every shot at us and the thing a
 * pilot wants to hear about -- so the two are kept apart rather than merged
 * into "they know we are here".
 */
export interface Watcher {
  name: string;
  team: Team | null;
  /** They hold a sensor contact on us. */
  contact: boolean;
  /** They hold a sensor lock on us, which is a contact and then some. */
  lock: boolean;
}

/**
 * Who has eyes on `observer`, worst first.
 *
 * Read off the same fields the rules use: a ship's `contacts` are what it has
 * found and its `sensor_locks` are what it is holding. Team-mates are left
 * out -- a squadron shares a plot by definition, and listing your own side as
 * "watching you" would bury the ships that matter.
 */
export const watchersOf = (observer: Ship, ships: Ship[]): Watcher[] =>
  ships
    .filter((ship) => ship.name !== observer.name)
    .filter((ship) => observer.team == null || ship.team !== observer.team)
    .map((ship) => ({
      name: ship.name,
      team: ship.team ?? null,
      contact: (ship.contacts ?? []).includes(observer.name),
      lock: (ship.sensor_locks ?? []).includes(observer.name),
    }))
    .filter((watcher) => watcher.contact || watcher.lock)
    .sort((a, b) => Number(b.lock) - Number(a.lock) || a.name.localeCompare(b.name));

/** One side's view of one contact. */
export interface PictureCell {
  /** The ship doing the looking. */
  ship: string;
  /** Whether it holds this contact. */
  held: boolean;
  /** Whether it is holding a lock on it. */
  lock: boolean;
}

/** A contact, and which of our ships can see it. */
export interface PictureRow {
  contact: string;
  team: Team | null;
  cells: PictureCell[];
  /** True when at least one of ours is missing it -- the gap worth closing. */
  gap: boolean;
}

/**
 * The squadron's sensor picture: every ship on our side against everything any
 * of us has found.
 *
 * This is the hand-off manager's view. A gap in a row is a ship that cannot
 * shoot, cannot be told to, and may not know it -- which is precisely what the
 * sensop is for.
 */
export const teamPicture = (observer: Ship, ships: Ship[]): {ours: Ship[]; rows: PictureRow[]} => {
  const ours =
    observer.team == null
      ? [observer]
      : ships
          .filter((ship) => ship.team === observer.team)
          .sort((a, b) =>
            a.name === observer.name ? -1 : b.name === observer.name ? 1 : a.name.localeCompare(b.name)
          );

  const known = new Set<string>();
  for (const ship of ours) {
    for (const contact of ship.contacts ?? []) {
      known.add(contact);
    }
  }
  // Our own ships are not contacts to be shared; a squadron always knows where
  // its own are.
  for (const ship of ours) {
    known.delete(ship.name);
  }

  const rows: PictureRow[] = [...known]
    .sort((a, b) => a.localeCompare(b))
    .map((contact) => {
      const target = ships.find((ship) => ship.name === contact) ?? null;
      const cells = ours.map((ship) => ({
        ship: ship.name,
        held: target == null ? false : hasContact(ship, target),
        lock: (ship.sensor_locks ?? []).includes(contact),
      }));
      return {
        contact,
        team: target?.team ?? null,
        cells,
        gap: cells.some((cell) => !cell.held),
      };
    });

  return {ours, rows};
};

/** One side's incoming missiles from a single shooter. */
export interface Salvo {
  /** Who threw them, as the missiles themselves record it. */
  source: string;
  /** How many are still flying. */
  count: number;
  /** The nearest one, in metres. */
  nearest: number;
  /** Rounds until the nearest one arrives, if it holds its course. */
  roundsOut: number;
}

/**
 * Missiles on their way to `observer`, grouped by whoever threw them.
 *
 * Grouped because a salvo is how the crew thinks about them -- "six from
 * Thrasher, two rounds out" -- and because the missiles' own names are
 * bookkeeping nobody wants read aloud.
 *
 * `roundsOut` is the nearest missile's distance over its closing speed, so it
 * assumes both keep their course. It is a warning, not a prophecy: a missile
 * under thrust arrives sooner.
 */
export const incomingSalvoes = (observer: Ship, missiles: Missile[]): Salvo[] => {
  const inbound = missiles.filter((missile) => missile.target === observer.name);
  const bySource = new Map<string, Missile[]>();
  for (const missile of inbound) {
    const source = missile.source === "" ? "unknown" : missile.source;
    bySource.set(source, [...(bySource.get(source) ?? []), missile]);
  }

  return [...bySource.entries()]
    .map(([source, salvo]) => {
      const distances = salvo.map((missile) => vectorDistance(missile.position, observer.position));
      const nearest = Math.min(...distances);
      const closest = salvo[distances.indexOf(nearest)];
      const closing = Math.hypot(
        closest.velocity[0] - observer.velocity[0],
        closest.velocity[1] - observer.velocity[1],
        closest.velocity[2] - observer.velocity[2]
      );
      const roundsOut =
        closing <= 0 ? Number.POSITIVE_INFINITY : Math.max(1, Math.ceil(nearest / (closing * TURN_IN_SECONDS)));
      return {source, count: salvo.length, nearest, roundsOut};
    })
    .sort((a, b) => a.nearest - b.nearest);
};

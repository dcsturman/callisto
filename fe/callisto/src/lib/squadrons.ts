import { Ship } from "lib/entities";

/** The most ships a squadron may hold: what one pass of the AI's network
 *  commands. The server enforces it too. */
export const MAX_SQUADRON = 20;

/** One squadron of a team, as the editors list it. */
export interface Squadron {
  name: string;
  ships: string[];
}

/** Every ship of `team`'s squadron `name`, in name order. */
export const squadronShips = (ships: Ship[], team: string | null | undefined, name: string): string[] =>
  ships
    .filter((ship) => (ship.team ?? null) === (team ?? null) && ship.squadron === name)
    .map((ship) => ship.name)
    .sort();

/** `team`'s squadrons, by name. Squadrons of the same name on two teams are
 *  two squadrons, so a team is always asked for. */
export const teamSquadrons = (ships: Ship[], team: string | null | undefined): Squadron[] => {
  const byName = new Map<string, string[]>();
  ships
    .filter((ship) => (ship.team ?? null) === (team ?? null) && ship.squadron != null)
    .forEach((ship) => {
      const name = ship.squadron as string;
      byName.set(name, [...(byName.get(name) ?? []), ship.name]);
    });
  return [...byName.entries()]
    .map(([name, members]) => ({ name, ships: members.sort() }))
    .sort((a, b) => a.name.localeCompare(b.name));
};

/** How many ships `team`'s squadron `name` would hold with `moving` put in
 *  it: its ships and the movers, each counted once. */
export const sizeAfterMove = (
  ships: Ship[],
  team: string | null | undefined,
  name: string,
  moving: string[],
): number => new Set([...squadronShips(ships, team, name), ...moving]).size;

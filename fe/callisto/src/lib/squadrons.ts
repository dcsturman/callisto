import { Ship } from "lib/entities";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";

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

/** The share of their designs' hull `ships` have left, 0 to 1, or null when
 *  no design among them is known. */
export const hullShare = (ships: Ship[], templates: ShipDesignTemplates): number | null => {
  let current = 0;
  let max = 0;
  ships.forEach((ship) => {
    const hull = templates[ship.design]?.hull;
    if (hull != null && hull > 0) {
      current += Math.max(0, ship.current_hull);
      max += hull;
    }
  });
  return max > 0 ? current / max : null;
};

/** The middle of `ships`, in metres. */
export const centroid = (ships: Ship[]): [number, number, number] => {
  const sum = ships.reduce<[number, number, number]>(
    (acc, ship) => [acc[0] + ship.position[0], acc[1] + ship.position[1], acc[2] + ship.position[2]],
    [0, 0, 0],
  );
  const n = Math.max(1, ships.length);
  return [sum[0] / n, sum[1] / n, sum[2] / n];
};

/**
 * What the AI says squadron `name` is doing, from its ships' reports: "step 1
 * of 2: engage Light Fighter; ...", "deciding…", or null when none of them is
 * AI-crewed or has reported.
 *
 * The squadron executor opens every ship's rationale with her squadron's step
 * ("Lance, step 1 of 2: ..." or "Lance: ..."), then her own reasons after a
 * full stop; only the squadron's part is wanted here.
 */
export const squadronStep = (ships: Ship[], name: string | null): string | null => {
  const reports = ships.flatMap((ship) => (ship.ai_report == null ? [] : [ship.ai_report]));
  if (reports.length === 0) {
    return null;
  }
  if (reports.some((report) => report.status === "Deciding")) {
    return "deciding…";
  }
  const rationale = reports.find((report) => report.rationale != null && report.rationale !== "")?.rationale;
  if (rationale == null) {
    return reports.some((report) => report.status === "NoAnswer") ? "no answer; keeping last orders" : null;
  }
  const own =
    name != null && (rationale.startsWith(`${name}, `) || rationale.startsWith(`${name}: `))
      ? rationale.slice(name.length + 2)
      : rationale;
  const end = own.indexOf(". ");
  return end < 0 ? own.replace(/\.$/, "") : own.slice(0, end);
};

/**
 * `count` new ship names from `base`: "Lance 01", "Lance 02", ... skipping
 * any already `taken`. Two digits up to 99, so they sort as they read.
 */
export const batchNames = (base: string, count: number, taken: string[]): string[] => {
  const stem = base.trim().replace(/\s+\d+$/, "");
  const width = count > 99 ? String(count).length : 2;
  const used = new Set(taken);
  const names: string[] = [];
  for (let n = 1; names.length < count; n++) {
    const name = `${stem} ${String(n).padStart(width, "0")}`;
    if (!used.has(name)) {
      names.push(name);
    }
  }
  return names;
};

/** Ships to a row when a batch is laid out: five abreast, as the scenarios
 *  that were laid out by hand do. */
const ROW = 5;

/**
 * Where each of a batch of `count` ships goes: rows of five, `spacing`
 * metres apart, centred on `centre` in the x-y plane, the first row at the
 * front.
 */
export const gridPositions = (
  centre: [number, number, number],
  count: number,
  spacing: number,
): [number, number, number][] => {
  const rows = Math.ceil(count / ROW);
  return Array.from({ length: count }, (_, i) => {
    const row = Math.floor(i / ROW);
    const inRow = Math.min(ROW, count - row * ROW);
    const column = i % ROW;
    return [
      centre[0] + (row - (rows - 1) / 2) * spacing,
      centre[1] + (column - (inRow - 1) / 2) * spacing,
      centre[2],
    ];
  });
};

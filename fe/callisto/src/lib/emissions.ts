import {Ship} from "lib/entities";

/**
 * What a ship is giving away.
 *
 * Mirrors the server's `Emissions` table (`rules_tables.rs`): each row is a DM
 * an enemy sensor operator adds to their check to find this ship, and the rows
 * stack. Kept here so the sensop can see the figure before anyone rolls
 * against it, rather than reading it out of the results afterwards.
 */
export interface EmissionTerm {
  /** The row's name, as the detection line in the results calls it. */
  name: string;
  /** Its DM. */
  value: number;
  /** What the crew would do about it, or null when nothing can be done. */
  remedy: string | null;
}

/** Highest DM worth drawing a scale to: past this a ship is simply obvious. */
export const EMISSION_SCALE_MAX = 14;

/** The thrust a ship is applying, in whole G -- the loudest of its burns. */
export const thrustInG = (ship: Ship): number => {
  const magnitudes = [ship.plan[0], ship.plan[1]].flatMap((burn) =>
    burn == null ? [] : [Math.hypot(burn[0][0], burn[0][1], burn[0][2])]
  );
  return Math.floor(Math.max(0, ...magnitudes));
};

/**
 * Every row of this ship's emission profile, in the server's order, including
 * the zeroes -- a sensop wants to see that the transponder is off, not just
 * that it is missing from a list.
 *
 * `firing` is what the ship has queued this round, since that is what it will
 * give away when the round resolves.
 */
export const emissionTerms = (ship: Ship, firing: boolean): EmissionTerm[] => [
  {
    name: "active sensors",
    value: (ship.active_sensors ?? true) ? 2 : 0,
    remedy: "go dark",
  },
  {name: "thrust", value: thrustInG(ship), remedy: "burn softer"},
  {
    name: "power plant",
    value: (ship.current_power ?? 0) > 0 ? 1 : 0,
    remedy: null,
  },
  {name: "firing", value: firing ? 2 : 0, remedy: "hold fire"},
  {
    name: "damage heat",
    value: (ship.crit_level ?? []).reduce((total, level) => total + level, 0),
    remedy: null,
  },
  {
    name: "transmitting",
    value: ship.transmitting ? 6 : 0,
    remedy: "stop transmitting",
  },
];

/** The total DM an enemy adds to find this ship. */
export const emissionTotal = (terms: EmissionTerm[]): number =>
  terms.reduce((total, term) => total + term.value, 0);

/** How loud a ship is, in four steps. */
export type EmissionLevel = "quiet" | "faint" | "loud" | "blazing";

/**
 * The band a total falls in. The steps are chosen against what the rows cost:
 * a dark, drifting ship sits in `quiet`, running active sensors alone reaches
 * `faint`, and a lit transponder puts any ship in `blazing` by itself -- which
 * is the point the sensop is there to make.
 */
export const emissionLevel = (total: number): EmissionLevel => {
  if (total <= 2) return "quiet";
  if (total <= 5) return "faint";
  if (total <= 9) return "loud";
  return "blazing";
};

/** What each step is called on screen. */
export const EMISSION_LEVEL_LABELS: {[level in EmissionLevel]: string} = {
  quiet: "Running dark",
  faint: "Faint",
  loud: "Loud",
  blazing: "Lit up",
};

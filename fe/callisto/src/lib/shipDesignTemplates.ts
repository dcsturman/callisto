import {
  Weapon,
  CompressedWeapon,
  mountName,
  spinalOf,
  weaponToString,
  weaponKinds,
  weaponMounts,
} from "./weapon";
import {spinalLabel, spinalTl, spinalTons} from "./spinal";
// Type-only: `lib/entities` sits in an import cycle with the Redux slices, and
// a value import from here would drag this module into it.
import type {Ship} from "./entities";
// Type-only for the same reason: CrewBuilder reaches back into `lib/entities`.
import type {Crew} from "components/controls/CrewBuilder";
import {ShipFeature} from "lib/power";
import {Software} from "lib/software";

/**
 * Readable names for screen types, which travel the wire as Rust enum variant
 * names -- so without this a referee sees "NuclearDamper".
 */
const SCREEN_LABELS: {[kind: string]: string} = {
  Meson: "Meson Screen",
  NuclearDamper: "Nuclear Damper",
};

/** The name to show a referee for a screen, and its count if more than one. */
export const describeScreens = (screens: string[] | undefined): string[] => {
  if (screens == null || screens.length === 0) {
    return [];
  }
  const counts = new Map<string, number>();
  screens.forEach((screen) => counts.set(screen, (counts.get(screen) ?? 0) + 1));
  return Array.from(counts.entries()).map(([kind, total]) => {
    const name = SCREEN_LABELS[kind] ?? kind;
    return total > 1 ? `${name} x${total}` : name;
  });
};

export interface ShipDesignTemplate {
  name: string;
  displacement: number;
  /**
   * Purchase price in millions of credits, from the design's data sheet.
   * Absent where it has not been confirmed. For building and balancing
   * scenarios by price.
   */
  cost_mcr?: number;
  hull: number;
  armor: number;
  maneuver: number;
  jump: number;
  power: number;
  fuel: number;
  crew: number;
  sensors: string;
  stealth: string | null;
  countermeasures: string | null;
  computer: number;
  /**
   * The crew this ship flies with, for a design that is one particular ship
   * rather than a class -- HMS Executor is one hull with one crew.
   *
   * Absent on class designs, where there is no such thing as "the" crew. Fields
   * the server omits when zero or empty (leadership, screen_gunnery) come back
   * missing, so merge onto a `createCrew()` base rather than using it raw.
   */
  crew_skills?: Crew | null;
  weapons: Weapon[];
  /** Directed defensive systems. Omitted from the wire when the design has none. */
  screens?: string[];
  /**
   * Everything else the hull is fitted with: a holographic hull, repair
   * drones. The ones that draw Power are switchable from the engineer's
   * power board; the rest are simply aboard.
   */
  features?: ShipFeature[];
  /** The software the design is sold with. */
  software?: Software[];
  /** Jump Control Specialisation: +5 Processing for Jump Control only. */
  computer_bis?: boolean;
  /** Hardened against ion weapons. */
  computer_fib?: boolean;
  tl: number;
  // Both are free-form and optional on the Rust side (`Option<String>`, omitted
  // from the wire when unset).  Used only to organize the design picker.
  role?: string | null;
  source?: string | null;
}

export const defaultShipDesignTemplate = () => {
  return {
    name: "",
    displacement: 0,
    hull: 0,
    armor: 0,
    maneuver: 0,
    jump: 0,
    power: 0,
    fuel: 0,
    crew: 0,
    sensors: "",
    stealth: null,
    countermeasures: null,
    computer: 0,
    crew_skills: null,
    weapons: [],
    tl: 0,
  };
};

export const compressedWeapons = (weapons: Weapon[] | null) => {
  const initial_acc: CompressedWeapon = {};

  if (weapons === null) {
    return initial_acc;
  }

  return weapons.reduce((accumulator, weapon) => {
    const weapon_name = weaponToString(weapon);
    if (accumulator[weapon_name]) {
      accumulator[weapon_name].total += 1;
    } else {
      accumulator[weapon_name] = {
        kind: weapon.kind ?? weaponKinds(weapon)[0] ?? "",
        mount: weapon.mount,
        total: 1,
        mounts: weaponMounts(weapon),
        // Carried so callers can rebuild the mount faithfully; without it a
        // mixed turret reads back as whatever its first gun happens to be.
        guns: weapon.guns,
      };
    }
    return accumulator;
  }, initial_acc);
};

/** One line of a design's armament: every mount of one kind, however many
 *  batteries they are split into. */
export interface ArmamentLine {
  name: string;
  mounts: number;
  batteries: number;
}

/**
 * A design's armament for reading, by mount: "200 Triple Beam Turrets in 13
 * batteries" rather than one line per battery size. A spinal is written as a
 * record sheet writes it, with the design's TL where it states none:
 * "Spinal – Meson (TL15) – 3× of 18,000 tons".
 */
export const armament = (weapons: Weapon[] | null, designTl: number = 0): ArmamentLine[] => {
  const lines = new Map<string, ArmamentLine>();
  for (const weapon of weapons ?? []) {
    const name = spinalName(weapon, designTl) ?? mountName(weapon);
    const line = lines.get(name) ?? {name, mounts: 0, batteries: 0};
    line.mounts += weaponMounts(weapon);
    if (weaponMounts(weapon) > 1) {
      line.batteries += 1;
    }
    lines.set(name, line);
  }
  return [...lines.values()];
};

/** "Spinal – Meson (TL15) – 3× of 18,000 tons", or null for any other mount. */
export const spinalName = (weapon: Weapon, designTl: number): string | null => {
  const spinal = spinalOf(weapon.mount);
  if (spinal == null) {
    return null;
  }
  const kind = weapon.kind ?? weaponKinds(weapon)[0] ?? "";
  const tons = spinalTons(kind, spinal.multiple, spinalTl(spinal, designTl));
  const size = tons == null ? "" : ` of ${tons.toLocaleString()} tons`;
  return `${spinalLabel(kind, spinal, designTl)}${size}`;
};

// Find the weapon_id of the nth with a given name.  This is part of going
// backwards from compress weapons to the actual weapon IDs (as the server has
// no idea about compressed weapons).
export const findNthWeapon = (weapons: Weapon[], weapon_name: string, n: number) => {
  for (let count = 0; count < weapons.length; count++) {
    if (weaponToString(weapons[count]) === weapon_name) {
      n -= 1;
      if (n === 0) {
        return count;
      }
    }
  }
  return -1;
};

export const getWeaponName = (weapons: Weapon[], weapon_id: number) => {
  const weapon = weapons[weapon_id];
  return weapon === undefined ? "" : weaponToString(weapon);
};

export type ShipDesignTemplates = {[key: string]: ShipDesignTemplate};

/**
 * A ship's actual armament: its own weapons when it has been given some,
 * otherwise the ones its design comes with.
 *
 * Mirrors `Ship::weapons()` on the server.  Every weapon read in the UI must go
 * through here, or a ship with custom armament will display its design's
 * weapons instead of its own.
 */
export const shipWeapons = (
  ship: Ship | null,
  templates: ShipDesignTemplates,
): Weapon[] => {
  if (ship === null) {
    return [];
  }
  return ship.weapons ?? templates[ship.design]?.weapons ?? [];
};

/** "MCr 274.25", or null for a design with no confirmed price. */
export const formatMcr = (cost: number | undefined | null): string | null =>
  cost == null ? null : `MCr ${cost.toLocaleString(undefined, { maximumFractionDigits: 2 })}`;

import { BaySize, Weapon, WeaponMount } from "./weapon";

// Hardpoint and Firmpoint accounting, per High Guard pp. 26 and 31.
//
// This is a UI affordance only: the engine has no notion of a hardpoint and
// never validates armament against tonnage, power or cost.  The editor shows
// the referee when a ship's armament breaks the book rules; it does not stop
// them.

export type AllowanceKind = "hardpoints" | "firmpoints";

export interface Allowance {
  kind: AllowanceKind;
  total: number;
}

/**
 * Weapon-mount allowance for a hull of the given displacement.
 *
 * Ships of 100 tons or more get one Hardpoint per 100 tons.  Smaller craft get
 * Firmpoints instead, on a three-step band rather than a ratio — which is why
 * this is not simply `floor(tons / 100)`.
 */
export function allowanceForDisplacement(displacement: number): Allowance {
  if (displacement >= 100) {
    return { kind: "hardpoints", total: Math.floor(displacement / 100) };
  }
  if (displacement < 35) {
    return { kind: "firmpoints", total: 1 };
  }
  if (displacement < 70) {
    return { kind: "firmpoints", total: 2 };
  }
  return { kind: "firmpoints", total: 3 };
}

/**
 * How many points a single mount consumes.
 *
 * A turret costs 1 regardless of whether it is single, double or triple — the
 * turret is the hardpoint, not the guns in it.  A Large Bay costs 5.  On small
 * craft a Barbette consumes three Firmpoints.
 */
export function mountCost(mount: WeaponMount, kind: AllowanceKind): number {
  if (typeof mount === "object" && "Bay" in mount) {
    return mount.Bay === "Large" ? 5 : 1;
  }
  if (mount === "Barbette") {
    return kind === "firmpoints" ? 3 : 1;
  }
  return 1;
}

export function isTurret(mount: WeaponMount): boolean {
  return typeof mount === "object" && "Turret" in mount;
}

function turretSize(mount: WeaponMount): number | null {
  return typeof mount === "object" && "Turret" in mount ? mount.Turret : null;
}

export interface AllowanceReport {
  allowance: Allowance;
  used: number;
  /** True when `used` exceeds the allowance. */
  overAllowance: boolean;
  /** One entry per input row; `null` where the row is legal or empty. */
  rowProblems: (string | null)[];
  /** Problems that belong to the armament as a whole, not to one row. */
  problems: string[];
}

/**
 * Score an armament against a hull's allowance.
 *
 * `weapons` is the row list straight out of the editor, so `null` entries
 * (mount "None") are expected and cost nothing.
 */
export function checkAllowance(
  weapons: (Weapon | null)[],
  displacement: number,
): AllowanceReport {
  const allowance = allowanceForDisplacement(displacement);
  const rowProblems: (string | null)[] = weapons.map(() => null);
  const problems: string[] = [];

  let used = 0;
  // Which rows pushed the running total past the allowance.  Charging the
  // overrun to the later rows means a legal ship stays clean and the referee
  // sees which additions are the ones that broke it.
  weapons.forEach((weapon, index) => {
    if (weapon === null) {
      return;
    }
    const cost = mountCost(weapon.mount, allowance.kind);
    used += cost;
    if (used > allowance.total) {
      rowProblems[index] = `Exceeds the ${allowance.total} ${allowance.kind} this hull allows`;
    }
  });

  if (allowance.kind === "firmpoints") {
    // A Firmpoint holds one weapon.  Exactly one may be upgraded to a turret,
    // and only to a single turret — not a double or a triple.
    let turretsSeen = 0;
    weapons.forEach((weapon, index) => {
      if (weapon === null || !isTurret(weapon.mount)) {
        return;
      }
      turretsSeen += 1;
      const size = turretSize(weapon.mount);
      if (size !== 1) {
        rowProblems[index] =
          rowProblems[index] ??
          "Small craft may only mount a single turret, not a double or triple";
      } else if (turretsSeen > 1) {
        rowProblems[index] =
          rowProblems[index] ?? "Only one Firmpoint may be upgraded to a turret";
      }
    });
    if (turretsSeen > 1) {
      problems.push(`${turretsSeen} turrets, but only one Firmpoint may be a turret`);
    }
  }

  if (used > allowance.total) {
    problems.push(`Uses ${used} of ${allowance.total} available ${allowance.kind}`);
  }

  return {
    allowance,
    used,
    overAllowance: used > allowance.total,
    rowProblems,
    problems,
  };
}

/**
 * Number of editor rows to show for a design.
 *
 * Normally one row per point of allowance.  A design whose own armament
 * already exceeds its allowance gets enough rows to show all of it — otherwise
 * loading such a design would silently drop mounts.  `excelsior` is the one
 * design in the library that needs this: it is really a barbette plus a single
 * mixed triple turret (2 hardpoints), but `WeaponMount` cannot express a mixed
 * turret so it is stored as three mounts.
 */
export function rowCountForDesign(
  displacement: number,
  designWeaponCount: number,
): number {
  return Math.max(allowanceForDisplacement(displacement).total, designWeaponCount);
}

/** Pad (or keep) an armament out to `rows` entries, empty rows last. */
export function padWeaponRows(
  weapons: readonly Weapon[],
  rows: number,
): (Weapon | null)[] {
  const padded: (Weapon | null)[] = weapons.slice(0, Math.max(rows, weapons.length));
  while (padded.length < rows) {
    padded.push(null);
  }
  return padded;
}

/** Drop the empty rows, preserving order.  This is what goes on the wire. */
export function compactWeaponRows(weapons: readonly (Weapon | null)[]): Weapon[] {
  return weapons.filter((weapon): weapon is Weapon => weapon !== null);
}

// --- Dropdown option tables -------------------------------------------------

export interface MountOption {
  id: string;
  label: string;
  /** `null` is the "None" option: the row carries no weapon. */
  mount: WeaponMount | null;
}

const BAY_SIZES: BaySize[] = ["Small", "Medium", "Large"];

export const MOUNT_OPTIONS: MountOption[] = [
  { id: "none", label: "None", mount: null },
  { id: "fixed", label: "Fixed Mount", mount: "FixedMount" },
  { id: "turret-1", label: "Single Turret", mount: { Turret: 1 } },
  { id: "turret-2", label: "Double Turret", mount: { Turret: 2 } },
  { id: "turret-3", label: "Triple Turret", mount: { Turret: 3 } },
  { id: "barbette", label: "Barbette", mount: "Barbette" },
  ...BAY_SIZES.map((size) => ({
    id: `bay-${size.toLowerCase()}`,
    label: `${size} Bay`,
    mount: { Bay: size } as WeaponMount,
  })),
];

/**
 * The option id matching an existing mount, or `"none"` for an empty row.
 *
 * Returns `null` for a mount no option covers — a turret of an unexpected size,
 * say — so the caller can surface it rather than silently snapping the row to
 * some other mount.
 */
export function mountOptionId(mount: WeaponMount | null): string | null {
  if (mount === null) {
    return "none";
  }
  const match = MOUNT_OPTIONS.find((option) => {
    if (option.mount === null) {
      return false;
    }
    if (typeof option.mount === "string" || typeof mount === "string") {
      return option.mount === mount;
    }
    if ("Turret" in option.mount && "Turret" in mount) {
      return option.mount.Turret === mount.Turret;
    }
    if ("Bay" in option.mount && "Bay" in mount) {
      return option.mount.Bay === mount.Bay;
    }
    return false;
  });
  return match ? match.id : null;
}

export function mountForOptionId(id: string): WeaponMount | null {
  return MOUNT_OPTIONS.find((option) => option.id === id)?.mount ?? null;
}

/** Mirrors the Rust `WeaponType` enum. */
export const WEAPON_KINDS: string[] = [
  "Beam",
  "Pulse",
  "Missile",
  "Sand",
  "Particle",
];

export const DEFAULT_WEAPON_KIND = WEAPON_KINDS[0];

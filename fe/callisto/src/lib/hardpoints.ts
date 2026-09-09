import {
  BaySize,
  Gun,
  Weapon,
  WeaponMount,
  createWeapon,
  isUniformWeapon,
  weaponGuns,
  weaponKindLabel,
  weaponKinds,
} from "./weapon";
import WEAPON_MOUNTS from "./weaponMounts.json";

// Hardpoint and Firmpoint accounting, per High Guard pp. 26 and 31.
//
// This is a UI affordance only: the engine has no notion of a hardpoint and
// never validates armament against tonnage, power or cost.  The editor shows
// the referee when a ship's armament breaks the book rules; it does not stop
// them.
//
// The editor works in *groups* — "30 x Triple Beam Turret" — not one row per
// hardpoint.  That matches how the books write designs, and it matters at
// scale: the largest ship in the library mounts 34 weapons but only three
// distinct kinds, and no design anywhere has more than five.  Groups are
// flattened back to a dense weapon list on submit, because `weapon_id` on the
// wire is a plain index into that list.

export type AllowanceKind = "hardpoints" | "firmpoints";

export interface Allowance {
  kind: AllowanceKind;
  total: number;
}

/**
 * A run of identical weapons and the gunner skill serving them.
 *
 * `mount` is `null` for the trailing empty row.  `gunnery` is the effective
 * skill level — the referee folds DEX and anything else into one number, so
 * there is nothing here to decompose.
 */
export interface WeaponGroup {
  count: number;
  mount: WeaponMount | null;
  kind: string;
  gunnery: number;
  /**
   * Weapon Advantages and Disadvantages carried by every mount in this group.
   *
   * Part of the group's identity: the MK Mora fits long-range high-yield pulse
   * turrets alongside plain sandcaster turrets, and merging across modifiers
   * would spread them to weapons that never had them.
   */
  modifiers: string[];
  /**
   * The guns in each mount of this group, when it holds more than one type.
   *
   * `undefined` for a uniform mount, which `kind` and the turret size already
   * describe. A mixed mount cannot be reduced to a single kind, so the editor
   * carries its gun list verbatim and writes it back untouched -- without this
   * a mixed turret would be saved as a uniform one and lose its other weapons.
   */
  guns?: Gun[];
}

export const DEFAULT_GUNNERY = 0;

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
  /** One entry per input group; `null` where the group is legal or empty. */
  rowProblems: (string | null)[];
  /** Problems that belong to the armament as a whole, not to one group. */
  problems: string[];
}

/**
 * Score an armament against a hull's allowance.
 *
 * `groups` is the row list straight out of the editor, so empty rows (mount
 * `null`) are expected and cost nothing.
 */
export function checkAllowance(
  groups: readonly WeaponGroup[],
  displacement: number,
): AllowanceReport {
  const allowance = allowanceForDisplacement(displacement);
  const rowProblems: (string | null)[] = groups.map(() => null);
  const problems: string[] = [];

  let used = 0;
  // Which rows pushed the running total past the allowance.  Charging the
  // overrun to the later rows means a legal ship stays clean and the referee
  // sees which additions are the ones that broke it.
  groups.forEach((group, index) => {
    if (group.mount === null || group.count <= 0) {
      return;
    }
    used += mountCost(group.mount, allowance.kind) * group.count;
    if (used > allowance.total) {
      rowProblems[index] = `Exceeds the ${allowance.total} ${allowance.kind} this hull allows`;
    }
    // The rules do not sell every weapon in every mount — there is no torpedo
    // turret and no laser bay.  The dropdown will not offer these, but a design
    // file can still contain one, so flag it rather than quietly accepting it.
    if (!isLegalPairing(group.kind, group.mount)) {
      rowProblems[index] =
        rowProblems[index] ?? `A ${group.kind} cannot be mounted this way`;
    }
  });

  if (allowance.kind === "firmpoints") {
    // A Firmpoint holds one weapon.  Exactly one may be upgraded to a turret,
    // and only to a single turret — not a double or a triple.  A group of two
    // single turrets is already two turrets, so count carries here.
    let turretsSeen = 0;
    groups.forEach((group, index) => {
      if (group.mount === null || group.count <= 0 || !isTurret(group.mount)) {
        return;
      }
      turretsSeen += group.count;
      const size = turretSize(group.mount);
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

/** An empty trailing row, ready for the referee to fill in. */
export function emptyGroup(gunnery: number = DEFAULT_GUNNERY): WeaponGroup {
  return { count: 1, mount: null, kind: DEFAULT_WEAPON_KIND, gunnery, modifiers: [] };
}

function groupKey(weapon: Weapon, gunnery: number): string {
  // The whole gun list is part of the key.  `kind` and `modifiers` are
  // undefined on a mixed mount -- they live on the guns -- so keying on them
  // alone collapsed every mixed turret on a ship into one group regardless of
  // what was in it, and saving then rewrote them all as the first gun's type.
  const guns = JSON.stringify(weaponGuns(weapon));
  return `${JSON.stringify(weapon.mount)}|${gunnery}|${guns}`;
}

/**
 * Collapse a flat weapon list into editor rows.
 *
 * Weapons that share a mount, a kind *and* a gunner skill become one row;
 * gunnery is part of the key because merging across it would silently level
 * the ace on the missile bay down to everyone else.  Rows come out in
 * first-appearance order, matching `compressedWeapons`, which the design
 * summary alongside this editor already uses.
 *
 * `gunnery` is positional — index `i` is the skill for weapon `i` — and short
 * or missing entries read as {@link DEFAULT_GUNNERY}, exactly as the Rust
 * `Crew::get_gunnery` does for an out-of-range index.
 */
export function groupWeapons(
  weapons: readonly Weapon[],
  gunnery: readonly number[] = [],
): WeaponGroup[] {
  const byKey = new Map<string, WeaponGroup>();
  const groups: WeaponGroup[] = [];

  weapons.forEach((weapon, index) => {
    const skill = gunnery[index] ?? DEFAULT_GUNNERY;
    const key = groupKey(weapon, skill);
    const existing = byKey.get(key);
    if (existing) {
      existing.count += 1;
      return;
    }
    const uniform = isUniformWeapon(weapon);
    const group: WeaponGroup = {
      count: 1,
      mount: weapon.mount,
      kind: weapon.kind ?? weaponKinds(weapon)[0] ?? "",
      gunnery: skill,
      modifiers: weapon.modifiers ?? weaponGuns(weapon)[0]?.modifiers ?? [],
      // Only a mixed mount needs its guns kept; a uniform one is fully
      // described by its kind and the mount's size.
      guns: uniform ? undefined : weaponGuns(weapon),
    };
    byKey.set(key, group);
    groups.push(group);
  });

  return groups;
}

/**
 * Flatten editor rows back into the dense arrays that go over the wire.
 *
 * The two arrays are index-aligned by construction, which is what lets
 * `FireAction`/`BoostTarget` keep addressing a weapon and its gunner by the
 * same `weapon_id`.  Empty and non-positive rows contribute nothing.
 */
export function expandGroups(groups: readonly WeaponGroup[]): {
  weapons: Weapon[];
  gunnery: number[];
} {
  const weapons: Weapon[] = [];
  const gunnery: number[] = [];

  groups.forEach((group) => {
    if (group.mount === null) {
      return;
    }
    for (let n = 0; n < group.count; n++) {
      // A mixed mount is written back exactly as it came in.
      weapons.push(
        group.guns != null
          ? { mount: group.mount, guns: group.guns }
          : createWeapon(group.kind, group.mount, group.modifiers),
      );
      gunnery.push(group.gunnery);
    }
  });

  return { weapons, gunnery };
}

/** How many guns a mount of this kind holds. */
export function gunCapacity(mount: WeaponMount | null): number {
  if (mount != null && typeof mount === "object" && "Turret" in mount) {
    return mount.Turret;
  }
  return 1;
}

/**
 * A short description of a group's guns, for the editor's weapon cell.
 *
 * A uniform group is named by its kind; a mixed one has no single kind, so it
 * lists what is actually in the mount.
 */
export function describeGroupGuns(group: WeaponGroup): string {
  if (group.guns == null) {
    return weaponKindLabel(group.kind);
  }
  const counts = new Map<string, number>();
  group.guns.forEach((gun) => counts.set(gun.kind, (counts.get(gun.kind) ?? 0) + 1));
  return Array.from(counts.entries())
    .map(([kind, total]) =>
      total > 1 ? `${weaponKindLabel(kind)} x${total}` : weaponKindLabel(kind),
    )
    .join(", ");
}

/** Total mounts across all rows — the number of weapons the ship will have. */
export function totalMounts(groups: readonly WeaponGroup[]): number {
  return groups.reduce(
    (sum, group) => (group.mount === null ? sum : sum + Math.max(group.count, 0)),
    0,
  );
}

/**
 * The gunner skill shared by every mount, or `null` when they differ.
 *
 * Drives the bulk "Gunner skill" field: it reads as the crew's skill when the
 * whole ship agrees, and blanks out once any row is overridden.
 *
 * Armed rows decide the answer.  With none — a ship with no weapons yet, or one
 * whose only row is mid-edit at a count of zero — it falls back to the rows as
 * written, so the field still shows the skill new rows will inherit.  Without
 * that fallback the field renders empty and every keystroke computes straight
 * back to empty, which reads as a dead control.
 */
export function commonGunnery(groups: readonly WeaponGroup[]): number | null {
  const armed = groups.filter((group) => group.mount !== null);
  const pool = armed.length > 0 ? armed : groups;
  if (pool.length === 0) {
    return null;
  }
  const first = pool[0].gunnery;
  return pool.every((group) => group.gunnery === first) ? first : null;
}

/** Set every row's gunner skill, for the bulk field. */
export function setAllGunnery(
  groups: readonly WeaponGroup[],
  gunnery: number,
): WeaponGroup[] {
  return groups.map((group) => ({ ...group, gunnery }));
}

// --- Dropdown option tables -------------------------------------------------

export interface MountOption {
  id: string;
  label: string;
  /** `null` is the "None" option: the row carries no weapon. */
  mount: WeaponMount | null;
  /** Key into {@link WEAPON_MOUNTS}; `null` for the "None" option. */
  mountClass: MountClass | null;
}

/**
 * Mirrors the Rust `MountClass`: a mount with the turret size erased, which is
 * the granularity the rules describe weapons at.
 */
export type MountClass =
  | "Turret"
  | "Fixed"
  | "Barbette"
  | "SmallBay"
  | "MediumBay"
  | "LargeBay"
  | "Battery";

/** The mount class of a concrete mount, matching Rust's `From<&WeaponMount>`. */
export function mountClassOf(mount: WeaponMount): MountClass | null {
  if (mount === "FixedMount") {
    return "Fixed";
  }
  if (mount === "Barbette") {
    return "Barbette";
  }
  if (typeof mount === "object" && "Turret" in mount) {
    return "Turret";
  }
  if (typeof mount === "object" && "Bay" in mount) {
    return `${mount.Bay}Bay` as MountClass;
  }
  if (typeof mount === "object" && "Battery" in mount) {
    return "Battery";
  }
  return null;
}

/**
 * Whether the rules sell this weapon in this mount.
 *
 * The table is generated from the Rust `weapon_profile` table, so the editor
 * cannot drift from what the server will actually fire — see the
 * `frontend_mount_matrix_is_current` test in `rules_tables.rs`.
 */
export function isLegalPairing(kind: string, mount: WeaponMount | null): boolean {
  if (mount === null) {
    return true;
  }
  const mountClass = mountClassOf(mount);
  const legal = (WEAPON_MOUNTS as Record<string, string[]>)[kind];
  // An unknown weapon kind comes from a design this build does not know about.
  // Leave it alone rather than declaring the referee's data illegal.
  if (mountClass === null || legal === undefined) {
    return true;
  }
  return legal.includes(mountClass);
}

/** The weapons the rules allow in a given mount, in {@link WEAPON_KINDS} order. */
export function weaponKindsForMount(mount: WeaponMount | null): string[] {
  if (mount === null) {
    return WEAPON_KINDS;
  }
  return WEAPON_KINDS.filter((kind) => isLegalPairing(kind, mount));
}

const BAY_SIZES: BaySize[] = ["Small", "Medium", "Large"];

// Point-defence batteries come in three grades and no others (High Guard p. 40).
const BATTERY_TYPES = [
  { grade: 1, numeral: "I" },
  { grade: 2, numeral: "II" },
  { grade: 3, numeral: "III" },
];

export const MOUNT_OPTIONS: MountOption[] = [
  { id: "none", label: "None", mount: null, mountClass: null },
  { id: "fixed", label: "Fixed Mount", mount: "FixedMount", mountClass: "Fixed" },
  { id: "turret-1", label: "Single Turret", mount: { Turret: 1 }, mountClass: "Turret" },
  { id: "turret-2", label: "Double Turret", mount: { Turret: 2 }, mountClass: "Turret" },
  { id: "turret-3", label: "Triple Turret", mount: { Turret: 3 }, mountClass: "Turret" },
  { id: "barbette", label: "Barbette", mount: "Barbette", mountClass: "Barbette" },
  ...BAY_SIZES.map((size) => ({
    id: `bay-${size.toLowerCase()}`,
    label: `${size} Bay`,
    mount: { Bay: size } as WeaponMount,
    mountClass: `${size}Bay` as MountClass,
  })),
  ...BATTERY_TYPES.map(({ grade, numeral }) => ({
    id: `battery-${grade}`,
    label: `PD Battery (Type ${numeral})`,
    mount: { Battery: grade } as WeaponMount,
    mountClass: "Battery" as MountClass,
  })),
];

// Small craft mount one weapon per Firmpoint.  A single Firmpoint may be
// upgraded to a turret, but only a single turret; doubles and triples need a
// real Hardpoint.  Bays are ship-scale weapons and have no place on a hull too
// small to have Hardpoints at all.
const FIRMPOINT_OPTION_IDS = new Set(["none", "fixed", "turret-1", "barbette"]);

/**
 * The mounts the editor should offer for a hull of this allowance kind.
 *
 * Filtering here is belt-and-braces: {@link checkAllowance} still flags an
 * illegal mount that arrives from a design file, since existing data must stay
 * visible rather than be silently rewritten.  This only stops the referee
 * *choosing* a mount the hull could never carry.
 */
export function mountOptionsFor(kind: AllowanceKind): MountOption[] {
  return kind === "hardpoints"
    ? MOUNT_OPTIONS
    : MOUNT_OPTIONS.filter((option) => FIRMPOINT_OPTION_IDS.has(option.id));
}

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
    if ("Battery" in option.mount && "Battery" in mount) {
      return option.mount.Battery === mount.Battery;
    }
    return false;
  });
  return match ? match.id : null;
}

export function mountForOptionId(id: string): WeaponMount | null {
  return MOUNT_OPTIONS.find((option) => option.id === id)?.mount ?? null;
}

/**
 * Mirrors the Rust `WeaponType` enum, in declaration order.
 *
 * Taken from the generated mount matrix so a weapon added on the server shows up
 * here without a second edit.
 */
export const WEAPON_KINDS: string[] = Object.keys(WEAPON_MOUNTS);

export const DEFAULT_WEAPON_KIND = WEAPON_KINDS[0];

export { weaponKindLabel };

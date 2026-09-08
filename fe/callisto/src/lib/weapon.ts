export type BaySize = "Small" | "Medium" | "Large";

// Mirrors the Rust `WeaponMount` enum: unit variants (`Barbette`) serialize as a
// bare string, tuple variants as a single-key object.
export type WeaponMount =
  | string
  | {Turret: number}
  | {Bay: BaySize}
  | {Battery: number};

/**
 * Human-readable names for the weapon kinds.
 *
 * Weapon kinds travel the wire as Rust enum variant names, so without this a
 * referee sees "PointDefense" and "MassDriver" in the design summary and the
 * Add Ship editor. Only kinds whose identifier differs from their name need an
 * entry; the rest are already words.
 */
const WEAPON_LABELS: {[kind: string]: string} = {
  PointDefense: "Point Defence",
  MassDriver: "Mass Driver",
};

/** The name to show a referee for a weapon kind. */
export const weaponKindLabel = (kind: string): string =>
  WEAPON_LABELS[kind] ?? kind;

/** Roman numerals for point-defence battery grades, which only run I to III. */
const BATTERY_TYPES: {[grade: number]: string} = {1: "I", 2: "II", 3: "III"};

export interface Weapon {
  kind: string;
  mount: WeaponMount;
  /**
   * High Guard weapon Advantages and Disadvantages. These ride on the weapon,
   * not the mount: a triple turret can hold two long-range high-yield pulse
   * lasers and an unmodified sandcaster. Omitted from the wire when empty.
   */
  modifiers?: string[];
}

/** Readable names for modifiers, which travel the wire as Rust variant names. */
const MODIFIER_LABELS: {[kind: string]: string} = {
  Accurate: "accurate",
  Inaccurate: "inaccurate",
  HighYield: "high yield",
  VeryHighYield: "very high yield",
  IntenseFocus: "intense focus",
  LongRange: "long range",
  Resilient: "resilient",
  EnergyEfficient: "energy efficient",
  EnergyInefficient: "energy inefficient",
  SizeReduction: "size reduction",
  IncreasedSize: "increased size",
  EasyToRepair: "easy to repair",
};

/**
 * Modifiers as a readable list, collapsing repeats the way the book writes them
 * ("energy efficient x3").
 */
export const describeModifiers = (modifiers: string[] | undefined): string => {
  if (modifiers == null || modifiers.length === 0) {
    return "";
  }
  const counts = new Map<string, number>();
  modifiers.forEach((m) => counts.set(m, (counts.get(m) ?? 0) + 1));
  return Array.from(counts.entries())
    .map(([kind, total]) => {
      const name = MODIFIER_LABELS[kind] ?? kind;
      return total > 1 ? `${name} x${total}` : name;
    })
    .join(", ");
};

export const createWeapon = (
  kind: string,
  mount: WeaponMount,
  modifiers?: string[],
): Weapon =>
  modifiers != null && modifiers.length > 0
    ? {kind, mount, modifiers}
    : {kind, mount};

export const weaponToString = (weapon: Weapon): string => {
    const kind = weaponKindLabel(weapon.kind);
    const mods = describeModifiers(weapon.modifiers);
    const suffix = mods === "" ? "" : ` (${mods})`;
    if (weapon.mount === "FixedMount") {
      return `${kind} Fixed Mount${suffix}`;
    } else if (typeof weapon.mount === "string") {
      return `${kind} Barbette${suffix}`;
    } else if ("Turret" in weapon.mount) {
      if (weapon.mount.Turret === 1) {
        return `Single ${kind} Turret${suffix}`;
      } else if (weapon.mount.Turret === 2) {
        return `Double ${kind} Turret${suffix}`;
      } else if (weapon.mount.Turret === 3) {
        return `Triple ${kind} Turret${suffix}`;
      }
    } else if ("Bay" in weapon.mount) {
      return `${weapon.mount.Bay} ${kind} Bay${suffix}`;
    } else if ("Battery" in weapon.mount) {
      // The grade is the whole identity of a battery, so it is named instead of
      // the weapon kind -- "Point Defence Battery (Type III)", not
      // "PointDefense Battery".
      const grade = BATTERY_TYPES[weapon.mount.Battery] ?? weapon.mount.Battery;
      return `Point Defence Battery (Type ${grade})${suffix}`;
    }
    console.error("Unknown weapon mount type: " + weapon.mount);
    return "ERROR in weaponToString()";
}

/**
 * Weapon kinds the crew never orders directly.
 *
 * Sandcasters are deployed by the combat engine rather than fired;
 * point-defence batteries intercept automatically, "needing only a command from
 * the bridge" (High Guard p. 40); and repulsors deflect incoming missiles rather
 * than attacking, so there is no target to pick for any of them.
 */
const PASSIVE_WEAPON_KINDS = new Set(["Sand", "PointDefense", "Repulsor"]);

/**
 * Whether this weapon is something the crew can be ordered to use.
 *
 * Tested on the weapon rather than its display name, so a design whose weapon
 * kind merely contains the word "Sand" keeps its buttons.
 */
export const isActionableWeapon = (weapon: Weapon): boolean => {
  if (PASSIVE_WEAPON_KINDS.has(weapon.kind)) {
    return false;
  }
  return !(typeof weapon.mount === "object" && "Battery" in weapon.mount);
};

/** The complement of {@link isActionableWeapon}: defences that run themselves. */
export const isPassiveWeapon = (weapon: Weapon): boolean =>
  !isActionableWeapon(weapon);

export interface CompressedWeapon {
  [weapon: string]: {
    kind: string;
    mount: WeaponMount;
    total: number;
  };
};

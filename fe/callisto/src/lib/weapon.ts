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

/** The mount on its own, with no weapon named. */
export const mountToString = (mount: WeaponMount): string => {
  if (mount === "FixedMount") {
    return "Fixed Mount";
  }
  if (typeof mount === "string") {
    return "Barbette";
  }
  if ("Turret" in mount) {
    return (
      {1: "Single Turret", 2: "Double Turret", 3: "Triple Turret"}[
        mount.Turret
      ] ?? `Turret of ${mount.Turret}`
    );
  }
  if ("Bay" in mount) {
    return `${mount.Bay} Bay`;
  }
  if ("Battery" in mount) {
    return `Point Defence Battery (Type ${mount.Battery})`;
  }
  return "Unknown Mount";
};

/** The name to show a referee for a weapon kind. */
export const weaponKindLabel = (kind: string): string =>
  WEAPON_LABELS[kind] ?? kind;

/** Roman numerals for point-defence battery grades, which only run I to III. */
const BATTERY_TYPES: {[grade: number]: string} = {1: "I", 2: "II", 3: "III"};

/** One gun inside a mount. */
export interface Gun {
  kind: string;
  modifiers?: string[];
}

/**
 * One weapon mount and its contents.
 *
 * The server writes the older single-kind shape whenever every gun in a mount
 * matches, which is almost always, so `kind` is present on all but genuinely
 * mixed turrets. Use {@link weaponGuns} rather than reading `kind` directly.
 */
export interface Weapon {
  kind?: string;
  guns?: Gun[];
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

/**
 * The guns in a mount, whichever shape it arrived in.
 *
 * A uniform mount sends one `kind` and a turret size; a mixed one sends `guns`.
 */
export const weaponGuns = (weapon: Weapon): Gun[] => {
  if (weapon.guns != null) {
    return weapon.guns;
  }
  const size =
    typeof weapon.mount === "object" && "Turret" in weapon.mount
      ? weapon.mount.Turret
      : 1;
  return Array.from({length: size}, () => ({
    kind: weapon.kind ?? "",
    modifiers: weapon.modifiers,
  }));
};

/** The distinct weapon types in a mount, in first-appearance order. */
export const weaponKinds = (weapon: Weapon): string[] => {
  const seen: string[] = [];
  weaponGuns(weapon).forEach((gun) => {
    if (!seen.includes(gun.kind)) {
      seen.push(gun.kind);
    }
  });
  return seen;
};

/** True when every gun in the mount is the same type. */
export const isUniformWeapon = (weapon: Weapon): boolean =>
  weaponKinds(weapon).length <= 1;

/** How many guns of `kind` the mount holds. */
export const countOfKind = (weapon: Weapon, kind: string): number =>
  weaponGuns(weapon).filter((gun) => gun.kind === kind).length;

export const createWeapon = (
  kind: string,
  mount: WeaponMount,
  modifiers?: string[],
): Weapon =>
  modifiers != null && modifiers.length > 0
    ? {kind, mount, modifiers}
    : {kind, mount};

export const weaponToString = (weapon: Weapon): string => {
    const kinds = weaponKinds(weapon);
    const mods = describeModifiers(
      weapon.modifiers ?? weaponGuns(weapon)[0]?.modifiers,
    );

    // A mixed mount cannot be named "Triple <kind> Turret", because it has no
    // single kind. Name the mount and list what is in it.
    if (kinds.length > 1) {
      const contents = kinds
        .map((k) => {
          const count = countOfKind(weapon, k);
          const label = weaponKindLabel(k);
          return count > 1 ? `${label} x${count}` : label;
        })
        .join(", ");
      return `${mountToString(weapon.mount)} (${contents})`;
    }

    const kind = weaponKindLabel(kinds[0] ?? weapon.kind ?? "");
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
  // A mount is actionable if any gun in it is something the crew can order.
  // A mixed turret of lasers and sand still gets a button for the lasers.
  if (weaponKinds(weapon).every((kind) => PASSIVE_WEAPON_KINDS.has(kind))) {
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
    /** Present when the mount holds more than one weapon type. */
    guns?: Gun[];
  };
};

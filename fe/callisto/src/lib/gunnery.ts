/**
 * What a gunner needs to know before pressing a button: how far a target is,
 * whether a given mount reaches it, and what the shot would roll at.
 *
 * Mirrors the server's `rules_tables.rs` (High Guard pp. 28-33 and the Core
 * Rulebook's range table). The two have to agree: the server decides whether
 * the shot lands, and a board that disagreed would be worse than none.
 */
import {Weapon, WeaponMount, weaponGuns} from "lib/weapon";

/** The bands, outermost last, as the server orders them. */
export const BANDS = ["Short", "Medium", "Long", "Very Long", "Distant"] as const;
export type Band = (typeof BANDS)[number];

/** DM to an attack at each band (Core Rulebook p. 169). */
export const RANGE_MOD: Record<Band, number> = {
  Short: 1,
  Medium: 0,
  Long: -2,
  "Very Long": -4,
  Distant: -6,
};

/**
 * The furthest band each weapon reaches, by mount class. `null` means the
 * weapon has no range limit of its own -- a launched salvo flies to its
 * target whatever the range.
 */
const MAX_RANGE: Record<string, Partial<Record<string, Band | null>>> = {
  Beam: {Turret: "Medium", FixedMount: "Medium", Barbette: "Medium"},
  Pulse: {Turret: "Long", FixedMount: "Long", Barbette: "Long"},
  Sand: {Turret: "Short", FixedMount: "Short"},
  Missile: {Turret: null, FixedMount: null, Barbette: null, Small: null, Medium: null, Large: null},
  Torpedo: {Barbette: null, Small: null, Medium: null, Large: null},
  Particle: {
    Turret: "Very Long",
    FixedMount: "Very Long",
    Barbette: "Very Long",
    Small: "Very Long",
    Medium: "Very Long",
    Large: "Distant",
  },
  Fusion: {
    Turret: "Medium",
    FixedMount: "Medium",
    Barbette: "Medium",
    Small: "Medium",
    Medium: "Medium",
    Large: "Long",
  },
  Plasma: {Turret: "Medium", FixedMount: "Medium", Barbette: "Medium"},
  Railgun: {
    Turret: "Short",
    FixedMount: "Short",
    Barbette: "Medium",
    Small: "Short",
    Medium: "Short",
    Large: "Medium",
  },
  Meson: {Small: "Long", Medium: "Long", Large: "Long"},
  MassDriver: {Small: "Short", Medium: "Short", Large: "Medium"},
  Repulsor: {Small: "Short", Medium: "Short", Large: "Short"},
  Ion: {Barbette: "Medium", Small: "Medium", Medium: "Medium", Large: "Medium"},
  PointDefense: {Battery: "Short"},
};

/** The mount class the tables are keyed by. */
export const mountClass = (mount: WeaponMount): string => {
  if (typeof mount === "string") {
    return mount;
  }
  if ("Turret" in mount) {
    return "Turret";
  }
  if ("Bay" in mount) {
    return mount.Bay;
  }
  return "Battery";
};

/** Whether this mount can shoot something at `band`. */
export const reaches = (weapon: Weapon, band: Band): boolean => {
  const kinds = weaponGuns(weapon).map((gun) => gun.kind);
  const cls = mountClass(weapon.mount);
  return kinds.some((kind) => {
    const entry = MAX_RANGE[kind];
    if (entry == null || !(cls in entry)) {
      return false;
    }
    const max = entry[cls];
    // A launcher has no reach of its own: the salvo flies to the target.
    if (max == null) {
      return true;
    }
    return BANDS.indexOf(band) <= BANDS.indexOf(max);
  });
};

/** The furthest band this mount reaches, for a summary line. */
export const maxBand = (weapon: Weapon): Band | null => {
  const kinds = weaponGuns(weapon).map((gun) => gun.kind);
  const cls = mountClass(weapon.mount);
  let furthest: Band | null = null;
  for (const kind of kinds) {
    const entry = MAX_RANGE[kind];
    if (entry == null || !(cls in entry)) {
      continue;
    }
    const max = entry[cls];
    if (max == null) {
      // Unlimited beats anything else in the mount.
      return null;
    }
    if (furthest == null || BANDS.indexOf(max) > BANDS.indexOf(furthest)) {
      furthest = max;
    }
  }
  return furthest;
};

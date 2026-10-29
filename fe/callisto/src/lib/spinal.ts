/**
 * Spinal mounts (High Guard pp. 34-36), as the server rules them.
 *
 * Mirrors `rules_tables.rs`: the Spinal Mount Weapons and Improvement tables,
 * the size DMs and the 2,000-ton limit, and the close-range DMs. The server
 * decides every shot; this is what the gunner's card and the design summary
 * show beforehand, so the two have to agree.
 */
import {Spinal, weaponKindLabel} from "lib/weapon";

export {spinalOf} from "lib/weapon";
export type {Spinal} from "lib/weapon";

/** One row of the Spinal Mount Weapons table (High Guard p. 35). */
export interface SpinalSpec {
  tl: number;
  /** Tons a multiple at that TL. */
  baseTons: number;
  /** Power per multiple. */
  power: number;
  /** Damage dice per multiple. */
  dice: number;
  maxTons: number;
}

export const SPINAL_SPECS: Record<string, SpinalSpec> = {
  MassDriver: {tl: 10, baseTons: 5_000, power: 250, dice: 4, maxTons: 100_000},
  Meson: {tl: 12, baseTons: 7_500, power: 1_000, dice: 6, maxTons: 75_000},
  Particle: {tl: 11, baseTons: 3_500, power: 1_000, dice: 8, maxTons: 28_000},
  Railgun: {tl: 10, baseTons: 3_500, power: 500, dice: 4, maxTons: 21_000},
};

/** Spinal Mounts Improvement: the share of its tons a spinal keeps at 0-3 TL above its own. */
const IMPROVEMENT_TONS = [100, 90, 85, 80];

/** The weapon's TL: its own when stated, else the design's. */
export const spinalTl = (spinal: Spinal, designTl: number): number => spinal.tl ?? designTl;

/** A spinal's tonnage, or null for a weapon with no spinal or a TL too low. */
export const spinalTons = (kind: string, multiple: number, tl: number): number | null => {
  const spec = SPINAL_SPECS[kind];
  if (spec == null || tl < spec.tl) {
    return null;
  }
  const keeps = IMPROVEMENT_TONS[Math.min(3, tl - spec.tl)];
  return Math.floor((spec.baseTons * multiple * keeps) / 100);
};

/** One Hardpoint per 100 tons, rounding up (High Guard p. 34). */
export const spinalHardpoints = (tons: number): number => Math.ceil(tons / 100);

/** What is wrong with a spinal on this hull, as the server would refuse it, or null. */
export const spinalProblem = (
  kind: string,
  spinal: Spinal,
  displacement: number,
  designTl: number,
): string | null => {
  const spec = SPINAL_SPECS[kind];
  const name = `${weaponKindLabel(kind).toLowerCase()} spinal mount`;
  if (spec == null) {
    return `A ${weaponKindLabel(kind)} cannot be a spinal mount`;
  }
  const tl = spinalTl(spinal, designTl);
  const tons = spinalTons(kind, spinal.multiple, tl);
  if (tons == null) {
    return `A ${name} needs TL${spec.tl}; this one is TL${tl}`;
  }
  if (tons > spec.maxTons) {
    return `A ${spinal.multiple}× ${name} at TL${tl} is ${tons.toLocaleString()} tons, over the largest of ${spec.maxTons.toLocaleString()}`;
  }
  if (tons * 2 > displacement) {
    return `A ${spinal.multiple}× ${name} at TL${tl} is ${tons.toLocaleString()} tons, more than half the hull`;
  }
  return null;
};

/**
 * The spinal's DM against a target of `tons`: -4 at 10,000 or less, -8 at
 * 5,000 or less, and null under 2,000, which it cannot attack unless she held
 * still last round (and then at -8).
 */
export const spinalSizeDm = (tons: number): number | null =>
  tons < 2_000 ? null : tons <= 5_000 ? -8 : tons <= 10_000 ? -4 : 0;

export type ShortBand = "Adjacent" | "Close" | "Short";

/** Where in the Short band a distance in metres falls: under 1 km, to 10 km, beyond. */
export const shortBandOf = (metres: number): ShortBand =>
  metres < 1_000 ? "Adjacent" : metres <= 10_000 ? "Close" : "Short";

const RANGE_DMS: Record<ShortBand, number[]> = {
  Adjacent: [-4, -6, -8, -10],
  Close: [-2, -4, -6, -8],
  Short: [-1, -2, -4, -6],
};

/** Spinal Weapon Range Adjustments (High Guard p. 35), by the attacker's size. */
export const spinalRangeDm = (attackerTons: number, band: ShortBand): number => {
  const column =
    attackerTons < 10_000 ? 0 : attackerTons <= 50_000 ? 1 : attackerTons <= 250_000 ? 2 : 3;
  return RANGE_DMS[band][column];
};

/** "Spinal – Meson (TL15) – 3×", as a record sheet writes it (High Guard p. 35). */
export const spinalLabel = (kind: string, spinal: Spinal, designTl: number): string =>
  `Spinal – ${weaponKindLabel(kind)} (TL${spinalTl(spinal, designTl)}) – ${spinal.multiple}×`;

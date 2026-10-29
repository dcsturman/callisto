/**
 * What a ship's crew carries into every check, and the size rules that go
 * with it, mirrored from the server (`rules_tables.rs`) so the cards can
 * say what a roll will take before it is made.
 */
import {Ship} from "lib/entities";
import {ShipDesignTemplate} from "lib/shipDesignTemplates";

/** The crew as a percentage of the complement, 100 when whole. */
export const crewPercent = (current: number, complement: number): number =>
  complement > 0 ? Math.floor((current * 100) / complement) : 100;

/**
 * The DM a large ship's crew losses put on every check (High Guard p. 27):
 * DM-1 unless the crew is above 90% of the complement, DM-2 unless above
 * 70%, DM-4 unless above 25%. The worst that applies.
 */
export const crewLossDm = (current: number, complement: number): number => {
  if (complement <= 0) {
    return 0;
  }
  const hundredths = current * 100;
  if (hundredths <= 25 * complement) {
    return -4;
  }
  if (hundredths <= 70 * complement) {
    return -2;
  }
  if (hundredths <= 90 * complement) {
    return -1;
  }
  return 0;
};

/**
 * The DM radiation puts on everything a ship does (High Guard p. 31): DM-1
 * per 10% of hull lost to Radiation weapons, per 40% with shielding, which
 * mesons ignore.
 */
export const radiationDm = (maxHull: number, other: number, meson: number, shielded: boolean): number => {
  if (maxHull <= 0) {
    return 0;
  }
  const steps = shielded
    ? Math.floor((other * 10) / (4 * maxHull)) + Math.floor((meson * 10) / maxHull)
    : Math.floor(((other + meson) * 10) / maxHull);
  return 0 - steps;
};

export interface CrewDm {
  /** From crew lost to criticals. */
  losses: number;
  /** From radiation. */
  radiation: number;
  /** The two together: what every check this crew makes carries. */
  total: number;
}

/** The DM this ship's crew carries into every check, by cause. */
export const crewDm = (ship: Ship, design: ShipDesignTemplate | undefined): CrewDm => {
  const losses = crewLossDm(ship.current_crew, design?.crew ?? 0);
  const radiation = radiationDm(
    design?.hull ?? 0,
    ship.radiation_hull_lost ?? 0,
    ship.meson_hull_lost ?? 0,
    design?.radiation_shielding ?? false,
  );
  return {losses, radiation, total: losses + radiation};
};

/**
 * Sensor stations on duty for Electronic Warfare: "one on-duty sensor
 * operator for every full 7,500 tons" (High Guard p. 34), never fewer than
 * one. Each can jam a different salvo.
 */
export const jamStations = (displacement: number): number => Math.max(1, Math.floor(displacement / 7_500));

/**
 * The mounts of weapon `index` still up: its count less any a Weapons
 * critical took, and none while the weapon is switched off.
 */
export const liveMounts = (ship: Ship, index: number, mounts: number): number =>
  ship.active_weapons[index] === false ? 0 : Math.max(0, mounts - (ship.mounts_down?.[index] ?? 0));

/**
 * The Hull figures High Guard p. 10 gives a hull of `tons`: one per 2.5
 * tons, per 2 from 25,000, per 1.5 from 100,000, under each legal mix of
 * configuration (dispersed -10%, buffered planetoid +50%) and reinforced or
 * light (+10%, -10%), rounded down -- as the server checks a design.
 */
export const hullPointsOptions = (tons: number): number[] => {
  const base = tons < 25_000 ? Math.floor((tons * 2) / 5) : tons < 100_000 ? Math.floor(tons / 2) : Math.floor((tons * 2) / 3);
  return [100, 110, 90, 99, 81, 150, 165, 135].map((percent) => Math.floor((base * percent) / 100));
};

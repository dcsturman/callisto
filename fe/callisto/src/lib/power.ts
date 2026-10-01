/**
 * Power arithmetic, kept apart from `entities.ts` because that module reaches
 * into the component tree and cannot be imported from a plain unit test.
 */
import {Weapon, WeaponMount, weaponGuns, weaponToString} from "lib/weapon";

/** The parts of a ship this module needs. */
export interface PoweredShip {
  current_power: number;
  /** Power an ion hit is currently suppressing; absent when none is. */
  ion_power_loss?: number;
}

/**
 * Power the ship can actually use, after any ion suppression.
 *
 * The server tracks ion damage apart from `current_power` so that a repair
 * cannot undo an ion hit -- which means anything asking what a ship can do now
 * has to subtract it. Reading `current_power` directly shows a ship at full
 * power in the same round its power was drained.
 */
export const availablePower = (ship: PoweredShip): number =>
  Math.max(0, ship.current_power - (ship.ion_power_loss ?? 0));

// --- The power budget -----------------------------------------------------
//
// Mirrors the server's `Ship::power_lines` (ship.rs) and the weapon tables in
// `rules_tables.rs`, which in turn are High Guard pp. 16-33. The engineer's
// board reads from here, and the two have to agree: the server decides what a
// ship can actually do, and a board that disagreed with it would be worse than
// no board.

/** One call on the plant, as the server names them. */
export type PowerSystem = "Basic" | "Sensors" | "Maneuver" | "Jump" | {Weapon: number};

/** One line of the budget. */
export interface PowerLine {
  system: PowerSystem;
  label: string;
  /** What it draws while running. */
  draw: number;
  online: boolean;
  /** True for the jump drive, which draws only as the ship jumps. */
  onDemand: boolean;
  /** False for basic systems, which can be turned down but not off. */
  switchable: boolean;
}

/** Whether two system references are the same line. */
export const samePowerSystem = (a: PowerSystem, b: PowerSystem): boolean =>
  typeof a === "string" || typeof b === "string"
    ? a === b
    : a.Weapon === b.Weapon;

/** What a sensor suite draws (High Guard p. 23). */
const SENSOR_POWER: {[grade: string]: number} = {
  Basic: 0,
  Civilian: 1,
  Military: 2,
  Improved: 4,
  Advanced: 6,
};

/** Per-gun Power by weapon and mount class (High Guard pp. 28-33). */
const WEAPON_POWER: {[kind: string]: {[mount: string]: number}} = {
  Missile: {Turret: 0, FixedMount: 0, Barbette: 0, Small: 5, Medium: 10, Large: 20},
  Torpedo: {Barbette: 2, Small: 2, Medium: 5, Large: 10},
  Sand: {Turret: 0, FixedMount: 0},
  Beam: {Turret: 4, FixedMount: 4, Barbette: 12},
  Pulse: {Turret: 4, FixedMount: 4, Barbette: 12},
  Railgun: {Turret: 2, FixedMount: 2, Barbette: 5, Small: 10, Medium: 15, Large: 25},
  Fusion: {Turret: 12, FixedMount: 12, Barbette: 20, Small: 50, Medium: 80, Large: 100},
  Plasma: {Turret: 6, FixedMount: 6, Barbette: 12},
  Particle: {Turret: 8, FixedMount: 8, Barbette: 15, Small: 30, Medium: 50, Large: 80},
  Ion: {Barbette: 10, Small: 20, Medium: 30, Large: 40},
  Meson: {Small: 20, Medium: 30, Large: 120},
  MassDriver: {Small: 15, Medium: 25, Large: 35},
  Repulsor: {Small: 50, Medium: 100, Large: 200},
  PointDefense: {},
};

/** The mount's own draw: a turret of any size takes one (High Guard p. 28). */
const mountPower = (mount: WeaponMount): number =>
  typeof mount === "object" && ("Turret" in mount || "Battery" in mount) ? 1 : 0;

/** The key `WEAPON_POWER` is indexed by for this mount. */
const mountKey = (mount: WeaponMount): string => {
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

/** What one mount draws with everything in it running. */
export const weaponMountPower = (weapon: Weapon): number => {
  const key = mountKey(weapon.mount);
  const guns = weaponGuns(weapon).reduce(
    (total, gun) => total + (WEAPON_POWER[gun.kind]?.[key] ?? 0),
    0
  );
  return guns + mountPower(weapon.mount);
};

/** The parts of a ship and its design the budget needs. */
export interface PowerBudgetShip extends PoweredShip {
  current_sensors: string;
  active_weapons: boolean[];
  offline?: PowerSystem[];
  basic_power_halved?: boolean;
}

/**
 * Every call on the plant, in the order the board shows them.
 *
 * `weapons` is the ship's own armament, which is what the server indexes by.
 */
export const powerLines = (
  ship: PowerBudgetShip,
  design: {displacement: number; maneuver: number; jump: number},
  weapons: Weapon[]
): PowerLine[] => {
  const offline = ship.offline ?? [];
  const isOnline = (system: PowerSystem) =>
    !offline.some((off) => samePowerSystem(off, system));
  const hull = design.displacement;

  const lines: PowerLine[] = [
    {
      system: "Basic",
      label: ship.basic_power_halved ? "Basic systems (half)" : "Basic systems",
      draw: ship.basic_power_halved ? Math.floor(hull / 10) : Math.floor(hull / 5),
      online: true,
      onDemand: false,
      switchable: false,
    },
    {
      system: "Sensors",
      label: `Sensors (${ship.current_sensors})`,
      draw: SENSOR_POWER[ship.current_sensors] ?? 0,
      online: isOnline("Sensors"),
      onDemand: false,
      switchable: true,
    },
    {
      system: "Maneuver",
      label: `M-drive (thrust ${design.maneuver})`,
      draw: Math.floor(hull / 10) * design.maneuver,
      online: isOnline("Maneuver"),
      onDemand: false,
      switchable: true,
    },
  ];

  if (design.jump > 0) {
    lines.push({
      system: "Jump",
      label: `J-drive (jump ${design.jump})`,
      draw: Math.floor(hull / 10) * design.jump,
      online: isOnline("Jump"),
      onDemand: true,
      switchable: true,
    });
  }

  weapons.forEach((weapon, index) => {
    const draw = weaponMountPower(weapon);
    if (draw === 0) {
      return;
    }
    lines.push({
      system: {Weapon: index},
      label: weaponToString(weapon),
      draw,
      online: isOnline({Weapon: index}) && (ship.active_weapons[index] ?? true),
      onDemand: false,
      switchable: true,
    });
  });

  return lines;
};

/** What everything running is drawing, leaving out the jump drive. */
export const powerDemand = (lines: PowerLine[]): number =>
  lines.filter((line) => line.online && !line.onDemand).reduce((total, line) => total + line.draw, 0);

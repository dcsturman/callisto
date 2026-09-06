import {Weapon, CompressedWeapon, weaponToString} from "./weapon";
// Type-only: `lib/entities` sits in an import cycle with the Redux slices, and
// a value import from here would drag this module into it.
import type {Ship} from "./entities";

export interface ShipDesignTemplate {
  name: string;
  displacement: number;
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
  weapons: Weapon[];
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
        kind: weapon.kind,
        mount: weapon.mount,
        total: 1,
      };
    }
    return accumulator;
  }, initial_acc);
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

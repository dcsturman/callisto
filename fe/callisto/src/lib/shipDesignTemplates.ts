import {Weapon, CompressedWeapon, weaponToString} from "./weapon";

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

export const compressedWeaponsFromTemplate = (design: ShipDesignTemplate | null) => {
  const initial_acc: CompressedWeapon = {};

  if (design === null) {
    return initial_acc;
  }

  return design.weapons.reduce((accumulator, weapon) => {
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
export const findNthWeapon = (design: ShipDesignTemplate, weapon_name: string, n: number) => {
  for (let count = 0; count < design.weapons.length; count++) {
    if (weaponToString(design.weapons[count]) === weapon_name) {
      n -= 1;
      if (n === 0) {
        return count;
      }
    }
  }
  return -1;
};

export const getWeaponName = (design: ShipDesignTemplate, weapon_id: number) => {
  return weaponToString(design.weapons[weapon_id]);
};

export type ShipDesignTemplates = {[key: string]: ShipDesignTemplate};

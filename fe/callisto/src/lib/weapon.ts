export type BaySize = "Small" | "Medium" | "Large";

// Mirrors the Rust `WeaponMount` enum: unit variants (`Barbette`) serialize as a
// bare string, tuple variants as a single-key object.
export type WeaponMount =
  | string
  | {Turret: number}
  | {Bay: BaySize}
  | {Battery: number};

/** Roman numerals for point-defence battery grades, which only run I to III. */
const BATTERY_TYPES: {[grade: number]: string} = {1: "I", 2: "II", 3: "III"};

export interface Weapon {
  kind: string;
  mount: WeaponMount;
}


export const createWeapon = (kind: string, mount: WeaponMount): Weapon => {
  return {kind, mount};
};

export const weaponToString = (weapon: Weapon): string => {
    if (weapon.mount === "FixedMount") {
      return `${weapon.kind} Fixed Mount`;
    } else if (typeof weapon.mount === "string") {
      return `${weapon.kind} Barbette`;
    } else if ("Turret" in weapon.mount) {
      if (weapon.mount.Turret === 1) {
        return `Single ${weapon.kind} Turret`;
      } else if (weapon.mount.Turret === 2) {
        return `Double ${weapon.kind} Turret`;
      } else if (weapon.mount.Turret === 3) {
        return `Triple ${weapon.kind} Turret`;
      }
    } else if ("Bay" in weapon.mount) {
      return `${weapon.mount.Bay} ${weapon.kind} Bay`;
    } else if ("Battery" in weapon.mount) {
      // The grade is the whole identity of a battery, so it is named instead of
      // the weapon kind -- "Point Defence Battery (Type III)", not
      // "PointDefense Battery".
      const grade = BATTERY_TYPES[weapon.mount.Battery] ?? weapon.mount.Battery;
      return `Point Defence Battery (Type ${grade})`;
    }
    console.error("Unknown weapon mount type: " + weapon.mount);
    return "ERROR in weaponToString()";
}

/**
 * Whether this weapon is something the crew can be ordered to use.
 *
 * Sandcasters are deployed by the combat engine rather than fired, and
 * point-defence batteries intercept automatically -- "needing only a command
 * from the bridge", High Guard p. 40 -- so neither gets an action button.
 *
 * Tested on the weapon rather than its display name, so a design whose weapon
 * kind merely contains the word "Sand" keeps its buttons.
 */
export const isActionableWeapon = (weapon: Weapon): boolean => {
  if (weapon.kind === "Sand" || weapon.kind === "PointDefense") {
    return false;
  }
  return !(typeof weapon.mount === "object" && "Battery" in weapon.mount);
};

export interface CompressedWeapon {
  [weapon: string]: {
    kind: string;
    mount: WeaponMount;
    total: number;
  };
};

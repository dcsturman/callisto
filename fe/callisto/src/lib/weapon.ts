export type BaySize = "Small" | "Medium" | "Large";

// Mirrors the Rust `WeaponMount` enum: unit variants (`Barbette`) serialize as a
// bare string, tuple variants as a single-key object.
export type WeaponMount = string | {Turret: number} | {Bay: BaySize};

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
    }
    console.error("Unknown weapon mount type: " + weapon.mount);
    return "ERROR in weaponToString()";
}

export interface CompressedWeapon {
  [weapon: string]: {
    kind: string;
    mount: WeaponMount;
    total: number;
  };
};

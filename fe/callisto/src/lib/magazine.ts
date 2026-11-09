import {Weapon, countOfKind, weaponGuns, weaponMounts} from "lib/weapon";

/**
 * Missiles, torpedoes and sandcaster canisters: in a mount, or in stores. And
 * rounds for a railgun or mass driver spinal, which feed it from stores and
 * are written only when there are some.
 */
export type Magazine = {missiles: number; torpedoes: number; sand: number; rounds?: number};

export const EMPTY_MAGAZINE: Magazine = {missiles: 0, torpedoes: 0, sand: 0};

/** The munitions a mount can hold, in the order they are listed. */
export const MUNITIONS: {key: "missiles" | "torpedoes" | "sand"; kind: string; label: string}[] = [
  {key: "missiles", kind: "Missile", label: "missiles"},
  {key: "torpedoes", kind: "Torpedo", label: "torpedoes"},
  {key: "sand", kind: "Sand", label: "canisters"},
];

const BAY_SALVO: {[kind: string]: {[size: string]: number}} = {
  Missile: {Small: 12, Medium: 24, Large: 120},
  Torpedo: {Small: 3, Medium: 6, Large: 30},
};

/**
 * What a mount holds ready to fire, for every mount in the entry. Mirrors the
 * server's `mount_capacity`.
 *
 * A turret with racks in it holds 12 missiles however many racks share it,
 * and one with sandcasters 20 canisters; four of either on a Firmpoint, which
 * is any hull under 100 tons (High Guard pp. 28-29, Core Rulebook p. 168). A
 * barbette holds 25 missiles or 3 torpedoes, a bay twelve salvos. A fixed
 * mount loads as a turret.
 */
export const mountCapacity = (weapon: Weapon, displacement: number): Magazine => {
  const has = (kind: string) => weaponGuns(weapon).some((gun) => gun.kind === kind);
  const firmpoint = displacement < 100;
  const mount = weapon.mount;
  let one = EMPTY_MAGAZINE;
  if (mount === "FixedMount" || (typeof mount === "object" && "Turret" in mount)) {
    one = {
      missiles: has("Missile") ? (firmpoint ? 4 : 12) : 0,
      torpedoes: 3 * countOfKind(weapon, "Torpedo"),
      sand: has("Sand") ? (firmpoint ? 4 : 20) : 0,
    };
  } else if (mount === "Barbette") {
    one = {missiles: has("Missile") ? 25 : 0, torpedoes: has("Torpedo") ? 3 : 0, sand: 0};
  } else if (typeof mount === "object" && "Bay" in mount) {
    one = {
      missiles: has("Missile") ? 12 * BAY_SALVO.Missile[mount.Bay] : 0,
      torpedoes: has("Torpedo") ? 12 * BAY_SALVO.Torpedo[mount.Bay] : 0,
      sand: 0,
    };
  }
  const mounts = weaponMounts(weapon);
  return {missiles: one.missiles * mounts, torpedoes: one.torpedoes * mounts, sand: one.sand * mounts};
};

const total = (magazine: Magazine) => magazine.missiles + magazine.torpedoes + magazine.sand;

/** Whether a mount holds any munitions at all: a launcher or a sandcaster. */
export const holdsMunitions = (weapon: Weapon, displacement: number): boolean =>
  total(mountCapacity(weapon, displacement)) > 0;

/**
 * Whether a Reload would put anything in this mount: it is below its load,
 * and stores hold some of what it is missing.
 */
export const canReload = (capacity: Magazine, loaded: Magazine, stores: Magazine): boolean =>
  MUNITIONS.some(({key}) => capacity[key] > loaded[key] && stores[key] > 0);

/**
 * Gunners beyond the mounts. The crew's gunnery list is index-aligned with
 * the mounts, so its entries past the last are gunners with no mount of
 * their own, free to reload one.
 */
export const spareGunners = (gunnery: number[] | undefined, mounts: number): number =>
  Math.max(0, (gunnery?.length ?? 0) - mounts);

/**
 * Which Reloads are done by a spare gunner: the first `spares` in the order
 * chosen. The rest are done by the mount's own gunner, who then does not
 * fire or defend that round.
 */
export const bySpare = (reloads: number[], spares: number, weaponId: number): boolean => {
  const place = reloads.indexOf(weaponId);
  return place >= 0 && place < spares;
};

/** "6 of 12 missiles", for each munition the mount holds. */
export const describeLoad = (capacity: Magazine, loaded: Magazine): string =>
  MUNITIONS.filter(({key}) => capacity[key] > 0)
    .map(({key, label}) => `${loaded[key]} of ${capacity[key]} ${label}`)
    .join(", ");

/** "Reload (6 of 12 loaded; 30 in stores)". */
export const reloadLabel = (capacity: Magazine, loaded: Magazine, stores: Magazine): string => {
  const parts = MUNITIONS.filter(({key}) => capacity[key] > 0).map(
    ({key}) => `${loaded[key]} of ${capacity[key]} loaded; ${stores[key]} in stores`,
  );
  return `Reload (${parts.join(" / ")})`;
};

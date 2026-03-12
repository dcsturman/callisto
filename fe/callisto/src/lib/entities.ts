import { Crew, createCrew } from "components/controls/CrewBuilder";

export type Acceleration = [[number, number, number], number];

// Planet visual effects enum
export enum PlanetVisualEffect {
  PHONG_LIGHTING = "PhongLighting",
  NOISE_TEXTURE = "NoiseTexture",
  STRIPED_BANDS = "StripedBands",
  ATMOSPHERE_RING = "AtmosphereRing",
  PLANETARY_RING = "PlanetaryRing",
  LATITUDE_COLOR = "LatitudeColor",
  ANIMATED_CLOUDS = "AnimatedClouds",
}

// Convert effect to its bit flag value
export function effectToBit(effect: PlanetVisualEffect): number {
  const bitMap: Record<PlanetVisualEffect, number> = {
    [PlanetVisualEffect.PHONG_LIGHTING]: 1 << 0,
    [PlanetVisualEffect.NOISE_TEXTURE]: 1 << 1,
    [PlanetVisualEffect.STRIPED_BANDS]: 1 << 2,
    [PlanetVisualEffect.ATMOSPHERE_RING]: 1 << 3,
    [PlanetVisualEffect.PLANETARY_RING]: 1 << 4,
    [PlanetVisualEffect.LATITUDE_COLOR]: 1 << 5,
    [PlanetVisualEffect.ANIMATED_CLOUDS]: 1 << 6,
  };
  return bitMap[effect];
}

// Convert a set of effects to a bitmask
export function effectsToBitmask(effects: PlanetVisualEffect[]): number {
  return effects.reduce((acc, effect) => acc | effectToBit(effect), 0);
}

// Check if an effect is enabled in a bitmask
export function hasEffect(bitmask: number, effect: PlanetVisualEffect): boolean {
  return (bitmask & effectToBit(effect)) !== 0;
}

export interface Entity {
  name: string;
  position: [number, number, number];
  velocity: [number, number, number];
}

export interface Ship extends Entity {
  plan: [Acceleration, Acceleration | null];
  design: string;
  current_hull: number;
  current_armor: number;
  current_power: number;
  current_maneuver: number;
  current_jump: number;
  current_fuel: number;
  current_crew: number;
  current_sensors: string;
  active_weapons: boolean[];
  dodge_thrust: number;
  assist_gunners: boolean;
  can_jump: boolean;
  sensor_locks: string[];
  crew: Crew;
  crit_level?: number[]; // Array of 11 numbers indexed by ShipSystem
  repair_bonus?: number;
  last_repair_component?: string | null; // String representation of ShipSystem (e.g., "Sensors")
  temporary_maneuver?: number;
  temporary_power_multiplier?: number;
  engineer_action_taken?: boolean;
}

export enum ShipSystem {
  Sensors = 0,
  Powerplant = 1,
  Fuel = 2,
  Weapon = 3,
  Armor = 4,
  Hull = 5,
  Maneuver = 6,
  Cargo = 7,
  Jump = 8,
  Crew = 9,
  Bridge = 10,
}

// Convert ShipSystem enum value to its string representation (for serialization)
export function shipSystemToString(system: ShipSystem): string {
  const names: Record<ShipSystem, string> = {
    [ShipSystem.Sensors]: "Sensors",
    [ShipSystem.Powerplant]: "Powerplant",
    [ShipSystem.Fuel]: "Fuel",
    [ShipSystem.Weapon]: "Weapon",
    [ShipSystem.Armor]: "Armor",
    [ShipSystem.Hull]: "Hull",
    [ShipSystem.Maneuver]: "Maneuver",
    [ShipSystem.Cargo]: "Cargo",
    [ShipSystem.Jump]: "Jump",
    [ShipSystem.Crew]: "Crew",
    [ShipSystem.Bridge]: "Bridge",
  };
  return names[system];
}

// Convert string representation back to ShipSystem enum value
export function stringToShipSystem(name: string): ShipSystem | null {
  const mapping: Record<string, ShipSystem> = {
    Sensors: ShipSystem.Sensors,
    Powerplant: ShipSystem.Powerplant,
    Fuel: ShipSystem.Fuel,
    Weapon: ShipSystem.Weapon,
    Armor: ShipSystem.Armor,
    Hull: ShipSystem.Hull,
    Maneuver: ShipSystem.Maneuver,
    Cargo: ShipSystem.Cargo,
    Jump: ShipSystem.Jump,
    Crew: ShipSystem.Crew,
    Bridge: ShipSystem.Bridge,
  };
  return mapping[name] ?? null;
}

// EngineerAction enum matches the Rust enum (unit variants and struct variant)
// Note: ShipSystem serializes as a string (e.g., "Sensors", "Powerplant", etc.)
export type EngineerActionType =
  | "OverloadDrive"
  | "OverloadPlant"
  | { Repair: { system: string } };

// EngineerActionMsg matches the Rust struct
export interface EngineerActionMsg {
  ship_name: string;
  action: EngineerActionType;
}

export interface EngineerActionResult {
  ship_name: string;
  action: EngineerActionType;
  success: boolean;
  check: number;
  target: number;
  message: string;
  critical_failure: boolean;
}

const createShip = (
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  plan: [Acceleration, Acceleration | null],
  design: string,
  current_hull: number,
  current_armor: number,
  current_power: number,
  current_maneuver: number,
  current_jump: number,
  current_fuel: number,
  current_crew: number,
  current_sensors: string,
  active_weapons: boolean[],
  dodge_thrust: number,
  assist_gunners: boolean,
  can_jump: boolean,
  sensor_locks: string[],
) => {
  return {
    name,
    position,
    velocity,
    plan,
    design,
    current_hull,
    current_armor,
    current_power,
    current_maneuver,
    current_jump,
    current_fuel,
    current_crew,
    current_sensors,
    active_weapons,
    dodge_thrust,
    assist_gunners,
    can_jump,
    sensor_locks,
    crew: createCrew(active_weapons.length),
  };
};

export const defaultShip = () => {
  return createShip(
    "New Ship",
    [0, 0, 0],
    [0, 0, 0],
    [[[0, 0, 0], 0], null],
    "Buccaneer",
    0,
    0,
    0,
    0,
    0,
    0,
    0,
    "",
    [],
    0,
    false,
    false,
    [],
  );
};

export interface Missile extends Entity {
  acceleration: [number, number, number];
  target: string;
  target_locked: boolean;
  target_sensor_lock: boolean;
  target_jump: boolean;
  target_destroyed: boolean;
  target_out_of_range: boolean;
  fuse: number;
}

export const defaultMissile = () => {
  return {
    name: "New Missile",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    target: "",
    target_locked: false,
    target_sensor_lock: false,
    target_jump: false,
    target_destroyed: false,
    target_out_of_range: false,
    fuse: 0,
  };
};

export const createMissile = (
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  acceleration: [number, number, number],
  target: string,
) => {
  return {
    name,
    position,
    velocity,
    acceleration,
    target,
    target_locked: false,
    target_sensor_lock: false,
    target_jump: false,
    target_destroyed: false,
    target_out_of_range: false,
    fuse: 0,
  };
};

export interface Planet extends Entity {
  color: string;
  primary: string | null;
  radius: number;
  mass: number;
  gravity_radius_2: number;
  gravity_radius_1: number;
  gravity_radius_05: number;
  gravity_radius_025: number;
  visual_effects: PlanetVisualEffect[]; // Array of visual effects to apply
}

export const defaultPlanet = () => {
  return {
    name: "New Planet",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    color: "yellow",
    primary: null,
    radius: 6.371e6,
    mass: 100,
    gravity_radius_2: 0,
    gravity_radius_1: 0,
    gravity_radius_05: 0,
    gravity_radius_025: 0,
    visual_effects: [],
  };
};

export const createPlanet = (
  name: string,
  position: [number, number, number],
  velocity: [number, number, number],
  color: string,
  primary: string | null,
  radius: number,
  mass: number,
  gravity_radius_2: number,
  gravity_radius_1: number,
  gravity_radius_05: number,
  gravity_radius_025: number,
  visual_effects: PlanetVisualEffect[] = [],
) => {
  return {
    name,
    position,
    velocity,
    color,
    primary,
    radius,
    mass,
    gravity_radius_2,
    gravity_radius_1,
    gravity_radius_05,
    gravity_radius_025,
    visual_effects,
  };
};

export interface MetaData {
  name: string;
  description: string;
}

export const createMetaData = (name: string, description: string) => {
  return {
    name,
    description,
  };
};

export interface EntityList {
  ships: Ship[];
  planets: Planet[];
  missiles: Missile[];
  metadata: MetaData;
}

export const defaultEntityList = () => {
  return {
    ships: [],
    planets: [],
    missiles: [],
    metadata: createMetaData("", ""),
  };
};

export const findShip = (entities: EntityList, name: string | null) => {
  if (name == null) {
    return null;
  }
  return entities.ships.find((ship) => ship.name === name) || null;
};

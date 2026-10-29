import { Crew, createCrew } from "components/controls/CrewBuilder";
import { Weapon } from "lib/weapon";
import { Team } from "lib/teams";
export { availablePower } from "lib/power";
import { PowerSystem } from "lib/power";
import {Software} from "lib/software";

export type Acceleration = [[number, number, number], number];

// Planet visual effects enum
export enum PlanetVisualEffect {
  PHONG_LIGHTING = "PhongLighting",
  NOISE_TEXTURE = "NoiseTexture",
  CONTINENTS = "Continents",
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
    [PlanetVisualEffect.CONTINENTS]: 1 << 2,
    [PlanetVisualEffect.STRIPED_BANDS]: 1 << 3,
    [PlanetVisualEffect.ATMOSPHERE_RING]: 1 << 4,
    [PlanetVisualEffect.PLANETARY_RING]: 1 << 5,
    [PlanetVisualEffect.LATITUDE_COLOR]: 1 << 6,
    [PlanetVisualEffect.ANIMATED_CLOUDS]: 1 << 7,
  };
  return bitMap[effect];
}

// Convert a set of effects to a bitmask
export function effectsToBitmask(effects: PlanetVisualEffect[]): number {
  return effects.reduce((acc, effect) => acc | effectToBit(effect), 0);
}

// Check if an effect is enabled in a bitmask
export function hasEffect(
  bitmask: number,
  effect: PlanetVisualEffect,
): boolean {
  return (bitmask & effectToBit(effect)) !== 0;
}

export interface Entity {
  name: string;
  position: [number, number, number];
  velocity: [number, number, number];
}

/** Who gives a ship its orders: people, or the AI service. */
export type Controller = "Human" | "AI";

/** Where the AI crewing a ship is in its round. Matches the Rust `AiStatus`. */
export type AiStatus = "Deciding" | "Ready" | "NoAnswer";

/** The AI's account of a ship's round, as its captain would give it. */
export interface AiReport {
  round: number;
  status: AiStatus;
  /** The side's standing plan. */
  plan?: string;
  /** Why this ship has the orders she has this round. */
  rationale?: string;
}

export interface Ship extends Entity {
  plan: [Acceleration, Acceleration | null];
  design: string;
  // Per-ship armament.  Absent means the ship uses its design's weapons, which
  // is what every ship built before this field existed does.  Read it through
  // `shipWeapons`, never directly.
  weapons?: Weapon[];
  current_hull: number;
  current_armor: number;
  current_power: number;
  /**
   * Power an ion hit is currently suppressing. Absent when none is.
   *
   * Tracked apart from `current_power` on the server so a repair cannot undo an
   * ion hit -- which means anything asking what the ship can actually do has to
   * subtract it. Use {@link availablePower} rather than `current_power`.
   */
  ion_power_loss?: number;
  /**
   * Bandwidth an ion hit is currently suppressing. Absent when none is.
   *
   * House rule: an ion hit spills a tenth of its damage into the computer.
   * Read the computer's capacity through {@link availableProcessing}.
   */
  ion_bandwidth_loss?: number;
  /** Rounds of ion suppression still to run. Absent when none is. */
  ion_rounds?: number;
  current_maneuver: number;
  current_jump: number;
  current_fuel: number;
  current_crew: number;
  /** The computer's Processing score, which Bandwidth is measured against. */
  current_computer: number;
  current_sensors: string;
  active_weapons: boolean[];
  dodge_thrust: number;
  assist_gunners: boolean;
  can_jump: boolean;
  sensor_locks: string[];
  /**
   * Ships this one currently detects.
   *
   * Directional: a contact here says nothing about whether that ship sees this
   * one. Absent means none, and nothing can be done to a ship that is not on
   * this list.
   */
  contacts?: string[];
  /**
   * Whether the ship runs active radar/lidar. Absent means yes.
   *
   * Omitted from the wire when true, so a ship running normally looks exactly
   * as it did before emissions existed.
   */
  active_sensors?: boolean;
  /**
   * Whether the ship is radiating on RF -- transponder, radio comms or both.
   * Absent means **no**: this defaults off, unlike active sensors.
   *
   * High Guard prints transponder and comms as one row at +6, the largest
   * modifier on the detection table. A scenario opts into that noise
   * deliberately rather than inheriting it.
   */
  transmitting?: boolean;
  /**
   * Whether the ship shares its sensor picture with its team. Absent means no.
   *
   * Sharing means broadcasting, so while this is on the server holds
   * {@link Ship.transmitting} on and the control for it is locked.
   */
  handoff_sensors?: boolean;
  /** Which side the ship is on. Absent means unaligned. */
  team?: Team;
  /** The squadron she flies in, within her team. */
  squadron?: string;
  /** Who gives the ship its orders. Absent means the people at its consoles. */
  controller?: Controller;
  /** Standing orders, in plain English. Absent means none. */
  orders?: string;
  /** Her team's fleet orders, as every ship of the team carries them. */
  fleet_orders?: string;
  /** Her squadron's orders, as every ship of it carries them. */
  squadron_orders?: string;
  /** What the ship's side is trying to do: the goal a trained commander is scored on. */
  goal?: "destroy" | "hold" | "raid" | "escape";
  /** Where a Hold ship holds: a body by name, or a point in metres. Absent:
   *  where its side started. */
  post?: string | [number, number, number];
  /** How close to the post, in km, counts as holding it. Absent: 25,000. */
  post_radius?: number;
  /** The AI's last report on this ship. Only an AI-crewed ship has one. */
  ai_report?: AiReport;
  crew: Crew;
  /**
   * Which sensor operator and which engineer are working this round, as
   * indices into the crew's lists. Absent means the first of them, which is
   * what a ship with one of each always uses.
   */
  sensor_operator?: number;
  engineer_on_duty?: number;
  /** Systems the engineer has powered down. Absent when everything is live. */
  offline?: PowerSystem[];
  /** Software aboard, which may differ from the design's. */
  software?: Software[];
  /** Which of it the computer is running. Bandwidth limits this. */
  software_running?: Software[];
  /**
   * Missiles, torpedoes and sandcaster barrels still aboard.
   *
   * Spent as they are fired and thrown; a reset restocks the ship.
   */
  magazine?: {missiles: number; torpedoes: number; sand: number};
  /**
   * Switchable features to start running, by index into the design's list.
   *
   * Only ever set by the Add Ship dialog on its way to the server; a ship
   * coming back from the server reports its switches in `offline` instead.
   */
  features_on?: number[];
  /** Basic ship systems running at half, which the rules allow in a pinch. */
  basic_power_halved?: boolean;
  /** How many times each overload has been tried; each one past the first costs DM-2. */
  overload_drive_attempts?: number;
  overload_plant_attempts?: number;
  crit_level?: number[]; // Array of 11 numbers indexed by ShipSystem
  /**
   * Each bridge station's state, in the order of {@link BRIDGE_STATIONS}.
   * Absent while every station works. A disabled station carries the
   * end-of-round ticks it has left.
   */
  bridge_stations?: StationStatus[];
  repair_bonus?: number;
  last_repair_component?: string | null; // String representation of ShipSystem (e.g., "Sensors")
  temporary_maneuver?: number;
  temporary_power_multiplier?: number;
  engineer_action_taken?: boolean;
  leadership_points?: number;
  leadership_rolled?: boolean;
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

// EngineerActionType is the queued engineer-slot's value type, mirroring the
// Rust ShipAction variants for OverloadDrive / OverloadPlant / Repair.
// Sent as part of ModifyActions; received as the `action` field on
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
  current_computer: number,
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
    current_computer,
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
  /** The ship that launched it. */
  source: string;
  target: string;
  target_locked: boolean;
  target_sensor_lock: boolean;
  target_jump: boolean;
  target_destroyed: boolean;
  target_out_of_range: boolean;
  fuse: number;
  /** Rounds of thrust it has left; absent from an older server. */
  burns?: number;
  /** Missiles in this salvo. A launch from one mount flies as one object;
   *  absent when it is a lone missile. */
  count?: number;
}

export const defaultMissile = () => {
  return {
    name: "New Missile",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    source: "",
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
  /** A template's sides and ships, sent with the scenario list for the
   *  new-game screen; absent elsewhere. */
  sides?: ScenarioSide[];
  /** How many rounds the scenario lasts, if it has a limit: a Hold side
   *  still at its post when they are up has won. */
  rounds?: number;
  /** How many of an Escape side's ships must jump out safely for it to have
   *  escaped; absent, more than two-thirds of them. */
  escape_needed?: number;
}

/** A ship that has jumped out of the scenario: got away, or misjumped. */
export interface Departure {
  name: string;
  team?: string;
  /** A successful jump. A misjump also leaves, but is no escape. */
  safe: boolean;
}

/** One side of a scenario template: its team (`Unaligned` for ships on
 *  none) and its ships, each with whether the file has the AI crew her. */
export interface ScenarioSide {
  team: string;
  ships: { name: string; ai: boolean }[];
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
  // Basename of the scenario file this list was loaded from (e.g.
  // "planetfun.json"). Empty for from-scratch builder sessions.
  filename: string;
  /** The round now being played, counting from 1. Absent from an older server. */
  round?: number;
  /** The ships that have jumped out, safely or not. Absent when none has. */
  departed?: Departure[];
}

export const defaultEntityList = () => {
  return {
    ships: [],
    planets: [],
    missiles: [],
    metadata: createMetaData("", ""),
    filename: "",
    round: 1,
  };
};

export const findShip = (entities: EntityList, name: string | null) => {
  if (name == null) {
    return null;
  }
  return entities.ships.find((ship) => ship.name === name) || null;
};

export const findPlanet = (entities: EntityList, name: string | null) => {
  if (name == null) {
    return null;
  }
  return entities.planets.find((planet) => planet.name === name) || null;
};

/** Bridge stations, in the server's order. */
export const BRIDGE_STATIONS = [
  "Comms",
  "Sensors",
  "Computer",
  "Astrogation",
  "Fire control",
  "Pilot",
] as const;

export type StationStatus = "Working" | "Destroyed" | { Disabled: number };

/**
 * The bridge stations that are out, as "Pilot disabled" or "Computer
 * destroyed". Empty when all are working.
 */
export const stationsDown = (ship: Ship): string[] =>
  (ship.bridge_stations ?? []).flatMap((status, index) => {
    if (status === "Working") return [];
    const state = status === "Destroyed" ? "destroyed" : "disabled";
    return [`${BRIDGE_STATIONS[index]} ${state}`];
  });

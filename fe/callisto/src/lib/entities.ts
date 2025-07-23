import {Crew, createCrew} from "components/controls/CrewBuilder";

export type Acceleration = [[number, number, number], number];

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
  sensor_locks: string[]
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
    []
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
  target: string
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
  gravity_radius_025: number
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
}

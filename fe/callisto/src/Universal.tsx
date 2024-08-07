import { createContext } from "react";

export type Acceleration = [[number, number, number], number];

export class Entity {
  name: string = "New Entity";
  position: [number, number, number] = [0, 0, 0];
  velocity: [number, number, number] = [0, 0, 0];

  constructor(name: string, position: [number, number, number], velocity: [number, number, number]) {
    this.name = name;
    this.position = position;
    this.velocity = velocity;
  }
}

export class Ship extends Entity {
  plan: [Acceleration, Acceleration | null] = [[[0, 0, 0], 0], null];
  usp: string = "0000000-00000-0";

  constructor(name: string, position: [number, number, number], velocity: [number, number, number], plan: [Acceleration, Acceleration | null], usp: string) {
    super(name, position, velocity);
    this.plan = plan;
    this.usp = usp;
  }
}

export class Planet extends Entity {
  color: string = "yellow";
  primary: [number, number, number] = [0, 0, 0];
  radius: number = 6.371e6;
  mass: number = 100;
  gravity_radius_2: number = 0;
  gravity_radius_1: number = 0;
  gravity_radius_05: number = 0;
  gravity_radius_025: number = 0;

  constructor(name: string, position: [number, number, number], velocity: [number, number, number], color: string, primary: [number, number, number], radius: number, mass: number, gravity_radius_2: number, gravity_radius_1: number, gravity_radius_05: number, gravity_radius_025: number) {
    super(name, position, velocity);
    this.color = color;
    this.primary = primary;
    this.radius = radius;
    this.mass = mass;
    this.gravity_radius_2 = gravity_radius_2;
    this.gravity_radius_1 = gravity_radius_1;
    this.gravity_radius_05 = gravity_radius_05;
    this.gravity_radius_025 = gravity_radius_025;
  }
}

export class Missile extends Entity {
  target: string = "";
  burns: number = 1;
  acceleration: [number, number, number] = [0, 0, 0];

  constructor(name: string, position: [number, number, number], velocity: [number, number, number], acceleration: [number, number, number]) {
    super(name, position, velocity);
    this.acceleration = acceleration;
  }
}

export type EntityList = {
  ships: Ship[];
  planets: Planet[];
  missiles: Missile[];
};
export type EntityRefreshCallback = (entities: EntityList) => void;

export const EntitiesServerContext = createContext<{
  entities: EntityList;
  handler: EntityRefreshCallback;
}>({ entities: { ships: [], planets: [], missiles: [] }, handler: (e) => {} });

export const EntitiesServerProvider = EntitiesServerContext.Provider;

export const EntityToShowContext = createContext<{
  entityToShow: Entity | null;
  setEntityToShow: (ship: Entity | null) => void;
}>({entityToShow: null, setEntityToShow: (e) => {}});

export const EntityToShowProvider = EntityToShowContext.Provider;

export type FlightPathResult = {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  plan: [Acceleration, Acceleration | null]
};

export type ViewControlParams = {gravityWells: boolean};

export const SCALE = 1e-6; // 1 unit = 100km or 1e6m
export const TURN_IN_SECONDS = 1e3;
export const G = 9.81;
export const DEFAULT_ACCEL_DURATION = 10000;

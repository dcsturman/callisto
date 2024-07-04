import { createContext } from "react";

export class Ship {}

export class Planet {
  color: string = "yellow";
  primary: [number, number, number] = [0, 0, 0];
  radius: number = 6.371e6;
  mass: number = 100;
}

export class Missile {
  target: string = "";
  burns: number = 1;
}

export type EntityKind = Ship | Planet | Missile;

export type Entity = {
  name: string;
  position: [number, number, number];
  velocity: [number, number, number];
  acceleration: [number, number, number];
  kind: EntityKind;
};

export const initEntity = {
  name: "New Entity",
  position: [0, 0, 0],
  velocity: [0, 0, 0],
  acceleration: [0, 0, 0],
};

export type EntityList = {
  ships: Entity[];
  planets: Entity[];
  missiles: Entity[];
};
export type EntityRefreshCallback = (entities: EntityList) => void;

export const EntitiesServerContext = createContext<{
  entities: EntityList;
  handler: EntityRefreshCallback;
}>({ entities: { ships: [], planets: [], missiles: [] }, handler: (e) => {} });

export const EntitiesServerProvider = EntitiesServerContext.Provider;

export type FlightPlan = {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  accelerations: [[number, number, number], number][];
};

export const SCALE = 1e-6; // 1 unit = 100km or 1e6m
export const TURN_IN_SECONDS = 1e3;
export const G = 9.81;

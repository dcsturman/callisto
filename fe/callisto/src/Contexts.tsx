import { create } from 'domain';
import { createContext, Dispatch, SetStateAction } from 'react';

export type Entity = {name: string, position: [number, number, number], velocity: [number, number, number], acceleration: [number, number, number]};

export const initEntity = {name: "New Entity", position: [0, 0, 0], velocity: [0, 0, 0], acceleration: [0, 0, 0]};

export const EntitiesServerContext = createContext<Entity[]>([]);
export const EntitiesServerUpdateContext = createContext<Dispatch<SetStateAction<Entity[]>>>(() => {}); // empty function as default value

export const EntitiesServerProvider = EntitiesServerContext.Provider;
export const EntitiesServerUpdateProvider = EntitiesServerUpdateContext.Provider;

export type EntityRefreshCallback = (entities: Entity[]) => void;

export type FlightPlan = {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  accelerations: [[number, number, number], number][];
};

export const scale = 1e-6; // 1 unit = 100km or 1e6m
export const timeUnit = 1e3;
export const G = 9.81;
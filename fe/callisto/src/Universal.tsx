import { createContext } from "react";
import { Crew } from "./CrewBuilder";

export type Acceleration = [[number, number, number], number];

export class Entity {
  name: string = "New Entity";
  position: [number, number, number] = [0, 0, 0];
  velocity: [number, number, number] = [0, 0, 0];

  constructor(
    name: string,
    position: [number, number, number],
    velocity: [number, number, number]
  ) {
    this.name = name;
    this.position = position;
    this.velocity = velocity;
  }
}

export class Ship extends Entity {
  plan: [Acceleration, Acceleration | null] = [[[0, 0, 0], 0], null];
  design: string = "Buccaneer";

  current_hull: number = 0;
  current_armor: number = 0;
  current_power: number = 0;
  current_maneuver: number = 0;
  current_jump: number = 0;
  current_fuel: number = 0;
  current_crew: number = 0;
  current_sensors: string = "";
  active_weapons: boolean[] = [];
  dodge_thrust: number = 0;
  assist_gunners: boolean = false;

  crew: Crew;

  constructor(
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
    assist_gunners: boolean
  ) {
    super(name, position, velocity);
    this.plan = plan;
    this.design = design;
    this.current_hull = current_hull;
    this.current_armor = current_armor;
    this.current_power = current_power;
    this.current_maneuver = current_maneuver;
    this.current_jump = current_jump;
    this.current_fuel = current_fuel;
    this.current_crew = current_crew;
    this.current_sensors = current_sensors;
    this.active_weapons = active_weapons;
    this.dodge_thrust = dodge_thrust;
    this.assist_gunners = assist_gunners;
    this.crew = new Crew(active_weapons.length);
  }

  static default(): Ship {
    return new Ship(
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
      [true, true],
      0,
      false
    );
  }
  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  static parse(json: any): Ship {
    const ship = new Ship(
      json.name,
      json.position,
      json.velocity,
      json.plan,
      json.design,
      json.current_hull,
      json.current_armor,
      json.current_power,
      json.current_maneuver,
      json.current_jump,
      json.current_fuel,
      json.current_crew,
      json.current_sensors,
      json.active_weapons,
      json.dodge_thrust,
      json.assist_gunners
    );
    ship.crew.parse(json.crew);
    return ship;
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

  constructor(
    name: string,
    position: [number, number, number],
    velocity: [number, number, number],
    color: string,
    primary: [number, number, number],
    radius: number,
    mass: number,
    gravity_radius_2: number,
    gravity_radius_1: number,
    gravity_radius_05: number,
    gravity_radius_025: number
  ) {
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

  static parse(json: { 
    name: string;
    position: [number, number, number];
    velocity: [number, number, number];
    color: string;
    primary: [number, number, number];
    radius: number;
    mass: number;
    gravity_radius_2: number;
    gravity_radius_1: number;
    gravity_radius_05: number;
    gravity_radius_025: number;
  }): Planet {
    return new Planet(
      json.name,
      json.position,
      json.velocity,
      json.color,
      json.primary,
      json.radius,
      json.mass,
      json.gravity_radius_2,
      json.gravity_radius_1,
      json.gravity_radius_05,
      json.gravity_radius_025
    );
  }
}

export class Missile extends Entity {
  target: string = "";
  burns: number = 1;
  acceleration: [number, number, number] = [0, 0, 0];

  constructor(
    name: string,
    position: [number, number, number],
    velocity: [number, number, number],
    acceleration: [number, number, number]
  ) {
    super(name, position, velocity);
    this.acceleration = acceleration;
  }

  static parse(json: {
    name: string;
    position: [number, number, number];
    velocity: [number, number, number];
    acceleration: [number, number, number];
  }): Missile {
    return new Missile(
      json.name,
      json.position,
      json.velocity,
      json.acceleration
    );
  }
}

export class EntityList {
  ships: Ship[];
  planets: Planet[];
  missiles: Missile[];

  constructor() {
    this.ships = [];
    this.planets = [];
    this.missiles = [];
  }

  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  static parse(json: any): EntityList {
    const entities = new EntityList();
    /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
    entities.ships = json.ships.map((ship: any) => Ship.parse(ship));
    /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
    entities.planets = json.planets.map((planet: any) => Planet.parse(planet));
    /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
    entities.missiles = json.missiles.map((missile: any) => Missile.parse(missile));
    return entities;
  }
};
export type EntityRefreshCallback = (entities: EntityList) => void;

export const EntitiesServerContext = createContext<{
  entities: EntityList;
  handler: EntityRefreshCallback;
}>({ entities: { ships: [], planets: [], missiles: [] }, handler: () => {} });

export const DesignTemplatesContext = createContext<{
  templates: ShipDesignTemplates,
  handler: (templates: ShipDesignTemplates) => void;
}>({templates: {}, handler: () => {}});

export const EntitiesServerProvider = EntitiesServerContext.Provider;
export const DesignTemplatesProvider = DesignTemplatesContext.Provider;

export const EntityToShowContext = createContext<{
  entityToShow: Entity | null;
  setEntityToShow: (ship: Entity | null) => void;
}>({entityToShow: null, setEntityToShow: () => {}});

export const EntityToShowProvider = EntityToShowContext.Provider;

export class FlightPathResult {
  path: [number, number, number][];
  end_velocity: [number, number, number];
  plan: [Acceleration, Acceleration | null]

  constructor(
    path: [number, number, number][],
    end_velocity: [number, number, number],
    plan: [Acceleration, Acceleration | null]
  ) {
    this.path = path;
    this.end_velocity = end_velocity;
    this.plan = plan;
  }

  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  static parse(json: any): FlightPathResult {
    return new FlightPathResult(
      json.path,
      json.end_velocity,
      json.plan
    );
  }
};

export type WeaponMount = string | { Turret: number } | { BaySize: string };

export class Weapon {
  kind: string;
  mount: WeaponMount;

  static parse(json: {
    kind: string;
    mount: WeaponMount;
  }): Weapon {
    const w = new Weapon();
    w.kind = json.kind;
    w.mount = json.mount;
    return w;
  }

  toString(): string {
    if (typeof this.mount === "string") {
      return `${this.kind} Barbette`;
    } else if ("Turret" in this.mount) {
      if (this.mount.Turret === 1) {
        return `Single ${this.kind} Turret`;
      } else if (this.mount.Turret === 2) {
        return `Double ${this.kind} Turret`;
      } else if (this.mount.Turret === 3) {
        return `Triple ${this.kind} Turret`;
      }
    } else if ("Bay" in this.mount) {
      return `${this.mount.Bay} ${this.kind} Bay`;
    }
    console.error("Unknown weapon mount type: " + this.mount);
    return "ERROR in Weapon.toString()";
  }

  constructor(kind: string = "Beam", mount: WeaponMount = { Turret: 0 }) {
    this.kind = kind;
    this.mount = mount;
  }

}

export class ShipDesignTemplate {
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
  computer: number;
  weapons: Weapon[];
  tl: number;

  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  static parse(json: any): ShipDesignTemplate {
    const t = new ShipDesignTemplate();
    t.name = json.name;
    t.displacement = json.displacement;
    t.hull = json.hull;
    t.armor = json.armor;
    t.maneuver = json.maneuver;
    t.jump = json.jump;
    t.power = json.power;
    t.fuel = json.fuel;
    t.crew = json.crew;
    t.sensors = json.sensors;
    t.computer = json.computer;
    /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
    t.weapons = json.weapons.map((w: any) => Weapon.parse(w));
    t.tl = json.tl;
    return t;
  }

  constructor() {
    this.name = "";
    this.displacement = 0;
    this.hull = 0;
    this.armor = 0;
    this.maneuver = 0;
    this.jump = 0;
    this.power = 0;
    this.fuel = 0;
    this.crew = 0;
    this.sensors = "";
    this.computer = 0;
    this.weapons = [];
    this.tl = 0;
  }

  compressedWeapons() {
    const initial_acc: {
      [weapon: string]: {
        kind: string;
        mount: WeaponMount;
        used: number;
        total: number;
      };
    } = {};

    return this.weapons.reduce(
      (accumulator, weapon) => {
        if (accumulator[weapon.toString()]) {
          accumulator[weapon.toString()].total += 1;
        } else {
          accumulator[weapon.toString()] = {
            kind: weapon.kind,
            mount: weapon.mount,
            used: 0,
            total: 1,
          };
        }
        return accumulator;
      },
      initial_acc
    );
  }
}


export type ShipDesignTemplates = { [key: string] : ShipDesignTemplate };

export type ViewControlParams = {
  gravityWells: boolean;
  jumpDistance: boolean;
};

export const SCALE = 1e-6; // 1 unit = 100km or 1e6m
// Be sure TURN_IN_SECONDS and G match the constants in entity.rs
export const TURN_IN_SECONDS = 360;
export const G = 9.807;
export const DEFAULT_ACCEL_DURATION = 10000;
// Not to be confused with SCALE, POSITION_SCALE is the degree vector values for position should be scaled.
// i.e. rather than having users enter meters, they enter position in kilometers.  Thus a 1000.0 scale.
export const POSITION_SCALE = 1000.0;

// Range bands for Short, Medium, Long, Very Long
export const RANGE_BANDS = [
  1250000,
  10000000,
  25000000, 
  50000000
];

export const SHIP_SYSTEMS = [
  "Sensors",
  "Powerplant",
  "Fuel",
  "Weapon",
  "Armor",
  "Hull",
  "Maneuver",
  "Cargo",
  "Jump",
  "Crew",
  "Bridge",
];

export const SCALE = 1e-6; // 1 unit = 100km or 1e6m
// Be sure TURN_IN_SECONDS and G match the constants in entity.rs
export const TURN_IN_SECONDS = 360;
export const G = 9.807;
export const DEFAULT_ACCEL_DURATION = 50000;
// Not to be confused with SCALE, POSITION_SCALE is the degree vector values for position should be scaled.
// i.e. rather than having users enter meters, they enter position in kilometers.  Thus a 1000.0 scale.
export const POSITION_SCALE = 1000.0;

// Range bands for Short, Medium, Long, Very Long
export const RANGE_BANDS = [1250000, 10000000, 25000000, 50000000];

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

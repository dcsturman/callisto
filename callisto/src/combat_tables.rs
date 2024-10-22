// Index by Weapon: Beam, Pulse, Missile, Sand, Particle
pub const HIT_WEAPON_MOD: [i32;5] = [4, 2, 0, 0, 0];

// How my die damage by weapon.
pub const DAMAGE_WEAPON_DICE: [i32;5] = [1, 2, 4, 0, 4];

// Range bands for 0, Short, Medium, Long, Very Long
pub const RANGE_BANDS: [usize; 5] = [0, 1250000, 10000000, 25000000, 50000000];

// One more than the number of range bands to handle Distant
pub const RANGE_MOD: [i32; 5] = [1, 0, -2, -4, -6];

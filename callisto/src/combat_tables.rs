// Index by Weapon: Beam, Pulse, Missile, Sand, Particle
pub const HIT_WEAPON_MOD: [i32;5] = [4, 2, 0, 0, 0];

// How my die damage by weapon.
pub const DAMAGE_WEAPON_DICE: [i32;5] = [1, 2, 4, 0, 4];

// Range bands for Short, Medium, Long, Very Long
pub const RANGE_BANDS: [usize; 4] = [12500000, 100000000, 250000000, 500000000];

// One more than the number of range bands to handle Distant
pub const RANGE_MOD: [i32; 5] = [1, 0, -2, -4, -6];

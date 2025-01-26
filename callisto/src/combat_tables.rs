// Index by Weapon: Beam, Pulse, Missile, Sand, Particle
pub const HIT_WEAPON_MOD: [i32; 5] = [4, 2, 0, 0, 0];

// How many die damage by weapon.
pub const DAMAGE_WEAPON_DICE: [u8; 5] = [1, 2, 4, 0, 4];

// Range bands for Short, Medium, Long, Very Long
pub const RANGE_BANDS: [u32; 4] = [1_250_000, 10_000_000, 25_000_000, 50_000_000];

// One more than the number of range bands to handle Distant
pub const RANGE_MOD: [i32; 5] = [1, 0, -2, -4, -6];

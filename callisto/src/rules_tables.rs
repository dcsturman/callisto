use crate::ship::{CounterMeasures, Stealth};

// Index by Weapon: Beam, Pulse, Missile, Sand, Particle
pub const HIT_WEAPON_MOD: [i32; 5] = [4, 2, 0, 0, 0];

// How many die damage by weapon.
pub const DAMAGE_WEAPON_DICE: [u8; 5] = [1, 2, 4, 0, 4];

// Range bands for Short, Medium, Long, Very Long
pub const RANGE_BANDS: [u32; 4] = [1_250_000, 10_000_000, 25_000_000, 50_000_000];

// One more than the number of range bands to handle Distant
pub const RANGE_MOD: [i32; 5] = [1, 0, -2, -4, -6];

// DM to sensor checks based on sensor quality
pub const SENSOR_QUALITY_MOD: [i16; 5] = [-4, -2, 0, 1, 2];

// DM to sensor checks based on stealth
pub fn stealth_mod(stealth: Option<Stealth>) -> i16 {
  match stealth {
    None => 0,
    Some(stealth) => STEALTH_MOD[stealth as usize],
  }
}
// Use this locally only.
const STEALTH_MOD: [i16; 4] = [-2, -2, -4, -6];

pub fn countermeasures_mod(countermeasures: Option<CounterMeasures>) -> i16 {
  match countermeasures {
    None => 0,
    Some(CounterMeasures::Standard) => 2,
    Some(CounterMeasures::Military) => 4,
  }
}

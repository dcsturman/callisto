use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use cgmath::InnerSpace;
use rand::RngCore;

use crate::combat_tables::{DAMAGE_WEAPON_DICE, HIT_WEAPON_MOD, RANGE_BANDS, RANGE_MOD};
use crate::{ debug, error };
use crate::entity::Entity;
use crate::payloads::{EffectMsg, FireAction, LaunchMissileMsg};
use crate::ship::{BaySize, Sensors, Ship, ShipSystem, Weapon, WeaponMount, WeaponType};

const DIE_SIZE: u32 = 6;
const STANDARD_ROLL_THRESHOLD: i32 = 8;
const CRITICAL_THRESHOLD: i32 = 6 + STANDARD_ROLL_THRESHOLD;

pub fn roll(rng: &mut dyn RngCore) -> u32 {
    rng.next_u32() % DIE_SIZE + 1
}

pub fn roll_dice(dice: usize, rng: &mut dyn RngCore) -> u32 {
    (0..dice).map(|_| roll(rng)).sum()
}

pub fn attack(
    hit_mod: i32,
    damage_mod: i32,
    attacker: &Ship,
    defender: &mut Ship,
    weapon: &Weapon,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    let attacker_name = attacker.get_name();

    debug!("(Combat.attack) Calculating range with attacker {} at {:?}, defender {} at {:?}.  Distance is {}.  Range is {}. Range_mod is {}", attacker.get_name(), attacker.get_position(), defender.get_name(), defender.get_position(), (defender.get_position() - attacker.get_position()).magnitude(), find_range_band((defender.get_position() - attacker.get_position()).magnitude() as usize),RANGE_MOD[find_range_band(
            (defender.get_position() - attacker.get_position()).magnitude() as usize,
        )]);
    debug!(
        "(Combat.attack) Ship {} attacking with {:?} against {} with hit mod {} and range {}",
        attacker_name,
        weapon,
        defender.get_name(),
        hit_mod,
        RANGE_MOD[find_range_band(
            (defender.get_position() - attacker.get_position()).magnitude() as usize,
        )]
    );

    let hit_roll = roll_dice(2, rng) as i32
        + hit_mod
        + HIT_WEAPON_MOD[weapon.kind as usize]
        + RANGE_MOD[find_range_band(
            (defender.get_position() - attacker.get_position()).magnitude() as usize,
        )];

    if hit_roll < STANDARD_ROLL_THRESHOLD {
        debug!(
            "(Combat.attack) {}'s attack roll is {} and misses.",
            attacker_name, hit_roll
        );
        return vec![EffectMsg::message(format!(
            "{}'s {} attack misses {}.",
            attacker_name,
            String::from(&weapon.kind),
            defender.get_name()
        ))];
    }

    debug!(
        "(Combat.attack) {}'s attack roll is {} and hits {}.",
        attacker_name,
        hit_roll,
        defender.get_name()
    );

    // Damage is compute as the weapon dice for the given weapon
    // + the effect of the hit roll
    let mut damage = u32::try_from(
        roll_dice(DAMAGE_WEAPON_DICE[weapon.kind as usize] as usize, rng) as i32 + hit_roll
            - STANDARD_ROLL_THRESHOLD
            - defender.get_current_armor() as i32
            + damage_mod,
    )
    .unwrap_or(0);

    debug!(
        "(Combat.attack) {} does {} damage to {}: hit effect {}, defender armor {}",
        attacker_name,
        damage,
        defender.get_name(),
        (hit_roll - STANDARD_ROLL_THRESHOLD),
        defender.get_current_armor()
    );

    // If after the attack (w/ armor) the damage is 0, then we're done, except for a message.
    if damage == 0 {
        return vec![EffectMsg::message(format!(
            "{} hit by {}'s {} but damage absorbed by armor.",
            attacker.get_name(),
            defender.get_name(),
            String::from(weapon.kind)
        ))];
    }

    // Calculate additional damage multipliers (for non missiles) and effects for non-crits now.
    let mut effects = if weapon.kind == WeaponType::Missile {
        // Create two effects: a message stating the damage and a ship impact on the defender.
        vec![
            EffectMsg::Message {
                content: format!(
                    "{} hit by a missile for {} damage.",
                    defender.get_name(),
                    damage
                ),
            },
            EffectMsg::ShipImpact {
                position: defender.get_position(),
            },
        ]
    } else {
        // Weapon multiples are only for non-missiles.  Larger missile mounts just launch more missiles.
        match weapon.mount {
            WeaponMount::Turret(num) => {
                damage += (num as u32 - 1) * DAMAGE_WEAPON_DICE[weapon.kind as usize] as u32;
            }
            WeaponMount::Barbette => {
                damage = damage * 3;
            }
            WeaponMount::Bay(size) => match size {
                BaySize::Small => {
                    damage = damage * 10;
                }
                BaySize::Medium => {
                    damage = damage * 20;
                }
                BaySize::Large => {
                    damage = damage * 100;
                }
            },
        }
        vec![
            EffectMsg::Message {
                content: format!(
                    "{} hit by {} for {} damage.",
                    defender.get_name(),
                    String::from(&weapon.kind),
                    damage
                ),
            },
            EffectMsg::BeamHit {
                origin: attacker.get_position(),
                position: defender.get_position(),
            },
        ]
    };

    debug!(
        "(Combat.attack) After modifiers {} does {} damage to {}.",
        attacker_name,
        damage,
        defender.get_name()
    );

    // The primary crit (if any) is a single crit at a level determined by the success of the hit.
    let primary_crit = hit_roll - CRITICAL_THRESHOLD > 0;

    if primary_crit {
        debug!(
            "(Combat.attack) Primary crit level {} to {}.",
            hit_roll - CRITICAL_THRESHOLD,
            defender.get_name()
        );
        // Add a single crit at the effect level
        effects.append(&mut do_critical(
            u8::try_from(hit_roll - CRITICAL_THRESHOLD)
                .expect("(combat.attack) hit_role primary crit calc is out of range"),
            defender,
            rng,
        ));
    }
    let crit_threshold: u32 = (defender.get_max_hull_points() as f32 / 10.0).ceil() as u32;

    // The secondary crit occurs for each new 10% of the ship's hull points that this hit passes.
    let current_hull = defender.get_current_hull_points();
    let prev_crits = (defender.get_max_hull_points() - current_hull) / crit_threshold;
    let secondary_crit =
        (defender.get_max_hull_points() - current_hull + damage) / crit_threshold - prev_crits;

    debug!(
        "(Combat.attack) Secondary crits {} to {}.",
        secondary_crit,
        defender.get_name()
    );

    // Add a level 1 crit for each secondary crit.
    for _ in 0..secondary_crit {
        effects.append(&mut do_critical(1, defender, rng));
    }

    defender.set_hull_points(u32::saturating_sub(current_hull, damage));
    effects
}

fn do_critical(crit_level: u8, defender: &mut Ship, rng: &mut dyn RngCore) -> Vec<EffectMsg> {
    let location = ShipSystem::from_repr(
        usize::try_from(roll_dice(2, rng) - 2).expect("(combat.apply_crit) roll is out of range"),
    )
    .expect("(combat.apply_crit) Unable to convert a roll to ship system.");

    let effects = apply_crit(crit_level, location, defender, rng);

    debug!(
        "(Combat.do_critical) {} suffers crits: {:?}.",
        defender.get_name(),
        effects
    );

    effects
}

fn apply_crit(
    crit_level: u8,
    location: ShipSystem,
    defender: &mut Ship,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    let current_level = defender.crit_level[location as usize];
    let level = u8::max(current_level + 1, crit_level);

    if level > 6 {
        let damage = roll_dice(6, rng);
        debug!(
            "(Combat.apply_crit) {} suffers > level 6 crit to {:?} for {}.",
            defender.get_name(),
            location,
            damage
        );
        defender.set_hull_points(u32::saturating_sub(
            defender.get_current_hull_points(),
            damage,
        ));
        vec![EffectMsg::message(format!(
            "{}'s critical hit caused {} damage.",
            defender.get_name(),
            damage
        ))]
    } else {
        defender.crit_level[location as usize] = level;

        match (location, level) {
            // I take some liberties with interpreting Sensors impact to make it a bit structured
            (ShipSystem::Sensors, 1) => {
                defender.attack_dm -= 1;
                vec![EffectMsg::message(format!(
                    "{}'s sensors critical hit and attack DM reduced by 1.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Sensors, 6) => {
                defender.active_weapons = vec![false; defender.active_weapons.len()];
                vec![EffectMsg::message(format!(
                    "{}'s sensors critical hit and completely disabled.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Sensors, _) => {
                if defender.current_sensors == Sensors::Basic {
                    defender.active_weapons = vec![false; defender.active_weapons.len()];
                    vec![EffectMsg::message(format!(
                        "{}'s sensors critical hit and completely disabled.",
                        defender.get_name()
                    ))]
                } else {
                    defender.current_sensors = defender.current_sensors - 1;
                    vec![EffectMsg::message(format!(
                        "{}'s sensors critical hit and reduced to {}.",
                        defender.get_name(),
                        String::from(defender.current_sensors)
                    ))]
                }
            }
            (ShipSystem::Powerplant, 3) => {
                defender.current_power =
                    u32::saturating_sub(defender.current_power, defender.design.power / 2);
                vec![EffectMsg::message(format!(
                    "{}'s powerplant critical hit and reduced by 50%.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Powerplant, 4) => {
                defender.current_power = 0;
                vec![EffectMsg::message(format!(
                    "{}'s powerplant critical hit and offline.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Powerplant, level) if level < 3 => {
                defender.current_power =
                    u32::saturating_sub(defender.current_power, defender.design.power / 10);
                vec![EffectMsg::message(format!(
                    "{}'s powerplant critical hit and reduced by 10%.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Powerplant, level) => {
                defender.current_power = 0;
                let mut effects = vec![EffectMsg::message(format!(
                    "{}'s powerplant critical hit and offline.",
                    defender.get_name()
                ))];
                effects.append(&mut apply_crit(
                    if level == 5 { 1 } else { roll(rng) as u8 },
                    ShipSystem::Hull,
                    defender,
                    rng,
                ));
                effects
            }
            (ShipSystem::Fuel, level) if level < 4 => {
                let fuel_loss = match level {
                    1 => roll(rng),
                    2 => roll_dice(2, rng),
                    3 => roll(rng) * defender.design.fuel / 10,
                    _ => 0,
                };
                defender.current_fuel = u32::saturating_sub(defender.current_fuel, fuel_loss);
                vec![EffectMsg::message(format!(
                    "{}'s fuel critical hit and reduced by {}.",
                    defender.get_name(),
                    fuel_loss
                ))]
            }
            (ShipSystem::Fuel, level) => {
                defender.current_fuel = 0;
                let mut effects = vec![EffectMsg::message(format!(
                    "{}'s fuel critical hit and fuel take destroyed.",
                    defender.get_name()
                ))];
                effects.append(&mut apply_crit(
                    if level == 5 { 1 } else { roll(rng) as u8 },
                    ShipSystem::Hull,
                    defender,
                    rng,
                ));
                effects
            }
            (ShipSystem::Weapon, 1) => {
                defender.attack_dm -= 1;
                vec![EffectMsg::message(format!(
                    "{}'s weapon critical hit and attack DM reduced by 1.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Weapon, level) => {
                let possible = defender.active_weapons.iter().filter(|x| **x).count() as u32;
                let mut effects = if possible > 0 {
                    let pick = (rng.next_u32() % possible) as usize;

                    debug!(
                        "(Combat.apply_crit) Weapon pick {} from active weapons for {} of {:?}.",
                        pick,
                        defender.get_name(),
                        defender.active_weapons
                    );
                    let selected_index = defender
                        .active_weapons
                        .iter()
                        .enumerate()
                        .filter(|(_, &active)| active)
                        .nth(pick)
                        .map(|(index, _)| index)
                        .unwrap();

                    defender.active_weapons[selected_index] = false;
                    vec![EffectMsg::message(format!(
                        "{}'s weapon critical hit and {} disabled.",
                        defender.get_name(),
                        String::from(&defender.design.weapons[selected_index])
                    ))]
                } else {
                    vec![EffectMsg::message(format!(
                        "{}'s weapon critical hit but all weapons already disabled.",
                        defender.get_name()
                    ))]
                };
                effects.append(&mut match level {
                    5 => apply_crit(1, ShipSystem::Hull, defender, rng),
                    6 => apply_crit(roll(rng) as u8, ShipSystem::Hull, defender, rng),
                    _ => vec![],
                });
                effects
            }
            (ShipSystem::Armor, level) => {
                let damage = match level {
                    1 => 1,
                    2 => roll(rng) / 2,
                    x if x < 5 => roll(rng),
                    _ => roll_dice(2, rng),
                };

                defender.current_armor = u32::saturating_sub(defender.current_armor, damage);
                let mut effects = vec![EffectMsg::message(format!(
                    "{}'s armor critical hit and reduced by {}.",
                    defender.get_name(),
                    damage
                ))];
                if level >= 5 {
                    effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng));
                }
                effects
            }
            (ShipSystem::Hull, level) => {
                let damage = roll_dice(level as usize, rng);
                defender.current_hull = u32::saturating_sub(defender.current_hull, damage);
                vec![EffectMsg::message(format!(
                    "{}'s hull critical hit and reduced by {}.",
                    defender.get_name(),
                    damage
                ))]
            }
            (ShipSystem::Manuever, 5) => {
                defender.current_maneuver = 0;
                vec![EffectMsg::message(format!(
                    "{}'s maneuver critical hit and offline.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Manuever, 6) => {
                defender.current_maneuver = 0;
                let mut effects = vec![EffectMsg::message(format!(
                    "{}'s maneuver critical hit and offline.",
                    defender.get_name()
                ))];
                effects.append(&mut apply_crit(
                    roll(rng) as u8,
                    ShipSystem::Hull,
                    defender,
                    rng,
                ));
                effects
            }
            (ShipSystem::Manuever, _) => {
                defender.current_maneuver = u8::saturating_sub(defender.current_maneuver, 1);
                vec![EffectMsg::message(format!(
                    "{}'s maneuver critical hit and reduced by 1.",
                    defender.get_name()
                ))]
            }
            // For now cargo has no impact on play, so we'll just give a message to this effect.
            (ShipSystem::Cargo, level) => vec![EffectMsg::message(format!(
                "{}'s cargo critical hit, now at level {}. (no play impact)",
                defender.get_name(),
                level
            ))],
            (ShipSystem::Jump, 1) => {
                defender.current_jump = u8::saturating_sub(defender.current_jump, 1);
                vec![EffectMsg::message(format!(
                    "{}'s jump critical hit and reduced by 1.",
                    defender.get_name()
                ))]
            }
            (ShipSystem::Jump, level) => {
                defender.current_jump = 0;
                let mut effects = vec![EffectMsg::message(format!(
                    "{}'s jump critical hit and offline.",
                    defender.get_name()
                ))];
                if level >= 4 {
                    effects.append(&mut apply_crit(1, ShipSystem::Hull, defender, rng))
                }
                effects
            }
            // For now crew has no impact on play, so we'll just give a message to this effect.
            (ShipSystem::Crew, level) => vec![EffectMsg::message(format!(
                "{}'s crew critical hit, now at level {}. (no play impact)",
                defender.get_name(),
                level
            ))],
            // For now bridge/computer has no impact on play, so we'll just give a message to this effect.
            (ShipSystem::Bridge, level) => vec![EffectMsg::message(format!(
                "{}'s bridge critical hit, now at level {}. (no play impact)",
                defender.get_name(),
                level
            ))],
        }
    }
}

fn find_range_band(distance: usize) -> usize {
    RANGE_BANDS.iter().position(|&x| x >= distance).unwrap_or(RANGE_BANDS.len())
}

// Process all incoming fire actions and turn them into either missile launches or attacks.
pub fn do_fire_actions(
    attacker: &Ship,
    ships: &mut HashMap<String, Arc<RwLock<Ship>>>,
    sand_counts: &mut HashMap<String, u32>,
    actions: &[FireAction],
    rng: &mut dyn RngCore,
) -> (Vec<LaunchMissileMsg>, Vec<EffectMsg>) {
    let mut new_missiles = vec![];
    let effects = actions
        .iter()
        .flat_map(|action| {
            debug!("(Combat.do_fire_actions) Process fire action {:?}.", action);

            let target = ships.get(&action.target);
            if target.is_none() {
                debug!(
                    "(Combat.do_fire_actions) No such target {} for fire action.",
                    action.target
                );
                return vec![];
            }

            let mut target = target.unwrap().write().unwrap();

            let weapon = attacker.get_weapon(action.weapon_id);
            debug!("(Combat.do_fire_actions) {:?}.", weapon);
            debug!(
                "(Combat.do_fire_actions) active_weapons={:?}.",
                attacker.active_weapons
            );
            if !attacker.active_weapons[action.weapon_id as usize] {
                debug!(
                    "(Combat.do_fire_actions) Weapon {} is disabled.",
                    action.weapon_id
                );
                return vec![];
            }

            match weapon.kind {
                WeaponType::Missile => {
                    // Missiles don't actually attack when fired.  They'll come back and call the attack function on impact.
                    let num_missiles = match weapon.mount {
                        WeaponMount::Turret(num) => num,
                        WeaponMount::Barbette => 5,
                        WeaponMount::Bay(BaySize::Small) => 12,
                        WeaponMount::Bay(BaySize::Medium) => 24,
                        WeaponMount::Bay(BaySize::Large) => 120,
                    };
                    for _ in 0..num_missiles {
                        new_missiles.push(LaunchMissileMsg {
                            source: attacker.get_name().to_string(),
                            target: target.get_name().to_string(),
                        });
                    }

                    debug!(
                        "(Combat.do_fire_actions) {} launches {} missile at {}.",
                        attacker.get_name(),
                        num_missiles,
                        target.get_name()
                    );

                    vec![EffectMsg::message(format!(
                            "{} launches {} missile(s) at {}.",
                            attacker.get_name(),
                            num_missiles,
                            target.get_name()
                        ))]
                }
                WeaponType::Beam | WeaponType::Pulse => {
                    // Lasers are special as sand can be used against them.
                    debug!(
                        "(Combat.do_fire_actions) {} fires {} at {} with lasers.",
                        attacker.get_name(),
                        String::from(&weapon.kind),
                        target.get_name()
                    );
                    let (sand_mod, mut effects) = match sand_counts.get(target.get_name()) {
                        Some(sand_count) if *sand_count > 0 => {
                            sand_counts.insert(target.get_name().to_string(), *sand_count-1);
                            let effect = roll_dice(2, rng) as i32 - STANDARD_ROLL_THRESHOLD;
                            if effect >= 0 {
                                debug!(
                                    "(Combat.do_fire_actions) {}'s sand successfully deployed against {} with effect {}.",
                                    target.get_name(),
                                    attacker.get_name(),
                                    effect
                                );
                                let sand_mod = effect + roll(rng) as i32;
                                (sand_mod, vec![EffectMsg::message(format!(
                                    "{}'s sand successfully deployed against {} with mod {}.",
                                    target.get_name(),
                                    attacker.get_name(),
                                    sand_mod))])
            
                            } else {
                                debug!(
                                    "(Combat.do_fire_actions) {}'s sand failed to deploy against {} with effect {}.",
                                    target.get_name(),
                                    attacker.get_name(),
                                    effect);

                                (0, vec![EffectMsg::message(format!(
                                    "{}'s sand failed to deploy against {}.",
                                    target.get_name(),
                                    attacker.get_name()))])
                            }

                        }
                        _ => { debug!("(Combat.do_fire_actions) {} has no sand to deploy against {}.", target.get_name(), attacker.get_name()); (0, vec![])},
                    };
                   
                    effects.append(&mut attack(0, -sand_mod, attacker, &mut target, weapon, rng));
                    effects
                }
                _ => {
                    debug!(
                        "(Combat.do_fire_actions) {} fires {} at {}.",
                        attacker.get_name(),
                        String::from(&weapon.kind),
                        target.get_name()
                    );

                    attack(0, 0, attacker, &mut target, weapon, rng)
                }
            }
        })
        .collect();

    (new_missiles, effects)
}

pub fn create_sand_counts(ship_snapshot: &HashMap<String, Ship>) -> HashMap<String, u32> {
    ship_snapshot.into_iter().map(|(name, ship)| (name.clone(), ship.design.weapons.iter().enumerate().filter(
        |(index, weapon)| weapon.kind == WeaponType::Sand && ship.active_weapons[*index]).fold(0 as u32,|total, (_, weapon)| match weapon.mount {
            WeaponMount::Turret(n) => total + n as u32,
            WeaponMount::Barbette => { error!("Barbette sand mount not supported."); total },
            WeaponMount::Bay(_) => { error!("Bay sand mount not supported."); total },
        }))).collect()
    }


#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Vec3;
    use crate::payloads::FireAction;
    use crate::ship::{BaySize, FlightPlan, Weapon, WeaponMount, WeaponType};
    use crate::ship::{Ship, ShipDesignTemplate};
    use cgmath::Zero;

    use rand::rngs::StdRng;
    use rand::SeedableRng;
    use std::collections::HashMap;
    use std::sync::{Arc, RwLock};

    #[test_log::test]
    fn test_missile_fire_actions() {
        let mut rng = StdRng::seed_from_u64(38); // Use a seeded RNG for reproducibility

        // Create a mock ship design with various weapon types and mounts
        let attacker_design = ShipDesignTemplate {
            name: "TestShip".to_string(),
            weapons: vec![
                Weapon {
                    kind: WeaponType::Beam,
                    mount: WeaponMount::Turret(1),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Barbette,
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Small),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Medium),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Large),
                },
            ],
            ..ShipDesignTemplate::default()
        };

        let target_design = ShipDesignTemplate {
            name: "TestTarget".to_string(),
            armor: 0,
            ..ShipDesignTemplate::default()
        };

        let attacker = Ship::new(
            "Attacker".to_string(),
            Vec3::new(-1000.0, 1000.0, 0.0),
            Vec3::zero(),
            FlightPlan::default(),
            Arc::new(attacker_design),
        );
        let target = Ship::new(
            "Target".to_string(),
            Vec3::new(1000.0, 0.0, 0.0),
            Vec3::zero(),
            FlightPlan::default(),
            Arc::new(target_design),
        );

        let mut ships = HashMap::new();
        ships.insert("Target".to_string(), Arc::new(RwLock::new(target)));
        let mut sand_counts = create_sand_counts(&crate::entity::deep_clone(&ships));

        let actions = vec![
            FireAction {
                weapon_id: 0,
                target: "Target".to_string(),
            }, // Beam Turret
            FireAction {
                weapon_id: 1,
                target: "Target".to_string(),
            }, // Missile Turret
            FireAction {
                weapon_id: 2,
                target: "Target".to_string(),
            }, // Missile Barbette
            FireAction {
                weapon_id: 3,
                target: "Target".to_string(),
            }, // Missile Bay (Small)
            FireAction {
                weapon_id: 4,
                target: "Target".to_string(),
            }, // Missile Bay (Medium)
            FireAction {
                weapon_id: 5,
                target: "Target".to_string(),
            }, // Missile Bay (Large)
        ];

        let (missiles, effects) = do_fire_actions(&attacker, &mut ships, &mut sand_counts, &actions, &mut rng);

        // Check beam weapon effect
        assert!(effects
            .iter()
            .any(|e| matches!(e, EffectMsg::BeamHit { .. })));

        // Check missile counts
        assert_eq!(missiles.len(), 2 + 5 + 12 + 24 + 120); // 2 from turret, 5 from barbette, 12 from small bay, 24 from medium bay, 120 from large bay

        // Check that all missiles have correct source and target
        for missile in &missiles {
            assert_eq!(missile.source, "Attacker");
            assert_eq!(missile.target, "Target");
        }

        // Check that we have the expected number of effects
        // 1 for beam plus any potential damage messages
        assert!(effects.len() >= 1);

        // You might want to add more specific checks based on your exact implementation
        // For example, checking for specific damage amounts or other effect details
    }

    #[test_log::test]
    fn test_apply_crit() {
        let mut rng = StdRng::seed_from_u64(42); // Use a seeded RNG for reproducibility

        let mut ship = Ship::new(
            "TestShip".to_string(),
            Vec3::zero(),
            Vec3::zero(),
            FlightPlan::default(),
            Arc::new(ShipDesignTemplate::default()),
        );

        // Test Hull critical hits
        for level in 1..=6 {
            let effects = apply_crit(level, ShipSystem::Hull, &mut ship, &mut rng);
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            assert_eq!(ship.crit_level[ShipSystem::Hull as usize], level);
        }

        let design = ShipDesignTemplate {
            name: "TestShip".to_string(),
            power: 100, // Make math easier to check tests
            fuel: 100,  // Makes math easier to check tests
            // Ensure enough weapons in this design so we can do all weapon crits
            weapons: vec![
                Weapon {
                    kind: WeaponType::Beam,
                    mount: WeaponMount::Turret(1),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Barbette,
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Small),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Medium),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Large),
                },
            ],
            ..ShipDesignTemplate::default()
        };

        // Reset ship
        ship = Ship::new(
            "TestShip".to_string(),
            Vec3::zero(),
            Vec3::zero(),
            FlightPlan::default(),
            Arc::new(design),
        );

        // Test Armor critical hits
        for level in 1..=6 {
            let effects = apply_crit(level, ShipSystem::Armor, &mut ship, &mut rng);
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            assert_eq!(ship.crit_level[ShipSystem::Armor as usize], level);
        }

        // Test Sensor critical hits
        for level in 1..=6 {
            let orig_sensors = ship.current_sensors;
            let effects = apply_crit(level, ShipSystem::Sensors, &mut ship, &mut rng);

            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            match level {
                1 => assert_eq!(ship.attack_dm, -1),
                6 => assert_eq!(ship.active_weapons, vec![false; 6]),
                _ => assert_eq!(ship.current_sensors, orig_sensors - 1),
            }
            assert_eq!(ship.crit_level[ShipSystem::Sensors as usize], level);
        }

        // Test Powerplant critical hits
        for level in 1..=6 {
            ship.current_power = 100; // Reset power before each test
            ship.current_hull = 100; // Reset hull before each test

            let effects = apply_crit(level, ShipSystem::Powerplant, &mut ship, &mut rng);

            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));

            match level {
                1 | 2 => {
                    assert_eq!(
                        ship.current_power,
                        90,
                        "Power should be reduced by 10% for level 1-2 {}",
                        level.to_string().as_str()
                    );
                }
                3 => {
                    assert_eq!(
                        ship.current_power, 50,
                        "Power should be reduced by 50% for level 3"
                    );
                }
                4 | 5 | 6 => {
                    assert_eq!(
                        ship.current_power, 0,
                        "Power should be reduced to 0 for level 4-6"
                    );
                }
                _ => unreachable!(),
            }

            if level >= 5 {
                assert_eq!(ship.current_power, 0);
                // Check for additional hull damage
                assert!(
                    ship.current_hull < 100,
                    "Ship should have taken hull damage for a Powerplant crit level 5+"
                );
            }

            // Reset crit level for next iteration
            ship.crit_level[ShipSystem::Powerplant as usize] = 0;
        }

        // Test Weapon critical hits
        for level in 1..=6 {
            ship.active_weapons = vec![true, true, true, true, true, true];
            let effects = apply_crit(level, ShipSystem::Weapon, &mut ship, &mut rng);
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            assert_eq!(ship.crit_level[ShipSystem::Weapon as usize], level);
        }

        // Test Fuel critical hits
        for level in 1..=6 {
            ship.current_fuel = 100; // Reset fuel before each test
            ship.current_hull = 100; // Reset hull before each test

            let effects = apply_crit(level, ShipSystem::Fuel, &mut ship, &mut rng);

            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));

            match level {
                1 | 2 | 3 => {
                    assert!(
                        ship.current_fuel < 100,
                        "Fuel should be reduced for level 1-3"
                    );
                    assert_eq!(
                        ship.current_hull, 100,
                        "Hull should not be affected for level 1-3"
                    );
                }
                4 | 5 | 6 => {
                    assert_eq!(
                        ship.current_fuel, 0,
                        "Fuel should be reduced to 0 for level 4-6"
                    );
                    assert!(
                        ship.current_hull < 100,
                        "Hull should be damaged for level 4-6"
                    );
                }
                _ => unreachable!(),
            }

            if level >= 4 {
                assert!(
                    effects.len() > 1,
                    "Should have additional hull damage effect for level 4+"
                );
            }

            // Reset crit level for next iteration
            ship.crit_level[ShipSystem::Fuel as usize] = 0;
        }

        // Test Drive critical hits
        for level in 1..=6 {
            ship.current_maneuver = 6;
            let effects = apply_crit(level, ShipSystem::Manuever, &mut ship, &mut rng);
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            assert_eq!(ship.crit_level[ShipSystem::Manuever as usize], level);
        }

        // Test Jump critical hits
        for level in 1..=6 {
            ship.current_jump = 6;
            let effects = apply_crit(level, ShipSystem::Jump, &mut ship, &mut rng);
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
            assert_eq!(ship.crit_level[ShipSystem::Jump as usize], level);
            if level >= 2 {
                assert_eq!(ship.current_jump, 0);
            } else {
                assert_eq!(ship.current_jump, 5);
            }
            if level >= 4 {
                assert!(effects.len() > 1); // Additional hull damage for level 4+
            }
        }

        // Test Crew critical hits
        for level in 1..=6 {
            let effects = apply_crit(level, ShipSystem::Crew, &mut ship, &mut rng);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], EffectMsg::Message { .. }));
        }

        // Test Bridge critical hits
        for level in 1..=6 {
            let effects = apply_crit(level, ShipSystem::Bridge, &mut ship, &mut rng);
            assert_eq!(effects.len(), 1);
            assert!(matches!(effects[0], EffectMsg::Message { .. }));
        }
    }

    #[test_log::test]
    fn test_attack() {
        let mut rng = StdRng::seed_from_u64(42); // Use a seeded RNG for reproducibility

        let attacker_design = Arc::new(ShipDesignTemplate {
            name: "Attacker".to_string(),
            weapons: vec![
                Weapon {
                    kind: WeaponType::Beam,
                    mount: WeaponMount::Turret(1),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Turret(2),
                },
                Weapon {
                    kind: WeaponType::Pulse,
                    mount: WeaponMount::Barbette,
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Small),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Medium),
                },
                Weapon {
                    kind: WeaponType::Missile,
                    mount: WeaponMount::Bay(BaySize::Large),
                },
            ],
            hull: 100,
            armor: 10,
            ..ShipDesignTemplate::default()
        });

        let defender_design = Arc::new(ShipDesignTemplate {
            name: "Defender".to_string(),
            hull: 200,
            armor: 0,
            ..ShipDesignTemplate::default()
        });

        let attacker = Ship::new(
            "Attacker".to_string(),
            Vec3::new(0.0, 0.0, 0.0),
            Vec3::zero(),
            FlightPlan::default(),
            attacker_design.clone(),
        );

        let mut defender = Ship::new(
            "Defender".to_string(),
            Vec3::new(1000.0, 0.0, 0.0),
            Vec3::zero(),
            FlightPlan::default(),
            defender_design.clone(),
        );

        // Test cases
        let test_cases = vec![
            (4, 0, WeaponType::Beam, WeaponMount::Turret(1), true),
            (0, 0, WeaponType::Missile, WeaponMount::Turret(2), false),
            (0, 0, WeaponType::Missile, WeaponMount::Turret(2), true),
            (0, 0, WeaponType::Pulse, WeaponMount::Barbette, true),
            (
                0,
                0,
                WeaponType::Missile,
                WeaponMount::Bay(BaySize::Small),
                true,
            ),
            (
                0,
                0,
                WeaponType::Missile,
                WeaponMount::Bay(BaySize::Medium),
                true,
            ),
            (
                0,
                0,
                WeaponType::Missile,
                WeaponMount::Bay(BaySize::Large),
                true,
            ),
            (10, 0, WeaponType::Beam, WeaponMount::Turret(1), true), // High hit mod
            (0, 10, WeaponType::Beam, WeaponMount::Turret(1), true), // High damage mod
        ];

        for (hit_mod, damage_mod, weapon_type, weapon_mount, should_hit) in test_cases {
            let weapon = Weapon {
                kind: weapon_type,
                mount: weapon_mount,
            };

            let starting_hull = defender.get_current_hull_points();

            let effects = attack(
                hit_mod,
                damage_mod,
                &attacker,
                &mut defender,
                &weapon,
                &mut rng,
            );

            // Check that we have effects. If not it means we missed which is okay for some attacks.
            // This is a hack but since the random seed is known, we map which should hit and which should miss.
            if !should_hit {
                assert!(effects.iter().filter(|e| !matches!(e, EffectMsg::Message { .. })).next().is_none(), "Miss should produce no effects");
                break;
            } else {
                assert!(effects.iter().filter(|e| !matches!(e, EffectMsg::Message { .. })).next().is_some(), "Hit should produce effects");
            }

            // Check for specific effect types based on weapon type
            match weapon_type {
                WeaponType::Beam | WeaponType::Pulse => {
                    assert!(effects
                        .iter()
                        .any(|e| matches!(e, EffectMsg::BeamHit { .. })));
                }
                WeaponType::Missile => {
                    // For missiles, we don't check for BeamHit, but we should have a damage message
                    assert!(effects
                        .iter()
                        .any(|e| matches!(e, EffectMsg::Message { .. })));
                }
                _ => panic!("Unexpected weapon type"),
            }

            // Check for damage
            assert!(
                defender.get_current_hull_points() < starting_hull,
                "Damage should be applied."
            );

            // Reset defender for next test
            defender = Ship::new(
                "Defender".to_string(),
                Vec3::new(1000.0, 0.0, 0.0),
                Vec3::zero(),
                FlightPlan::default(),
                defender_design.clone(),
            );
        }

        // Test miss scenario
        let miss_effects = attack(
            -10,
            0,
            &attacker,
            &mut defender,
            &Weapon {
                kind: WeaponType::Beam,
                mount: WeaponMount::Turret(1),
            },
            &mut rng,
        );
        assert!(miss_effects.iter().filter(|e| !matches!(e, EffectMsg::Message { .. })).next().is_none(), "Miss should produce no effects");

        // Test critical hit scenario
        let crit_effects = attack(
            20,
            0,
            &attacker,
            &mut defender,
            &Weapon {
                kind: WeaponType::Beam,
                mount: WeaponMount::Turret(1),
            },
            &mut rng,
        );
        assert!(crit_effects
            .iter()
            .any(|e| matches!(e, EffectMsg::Message { content } if content.contains("critical"))));

        // Test scenario for non-missile weapons in medium or large bays
        for size in vec![BaySize::Medium, BaySize::Large] {
            let mut effects = vec![];

            while effects.iter().filter(|e| !matches!(e, EffectMsg::Message { .. })).next().is_none() {
                defender.current_hull = 200;
                effects = attack(
                    0,
                    0,
                    &attacker,
                    &mut defender,
                    &Weapon {
                        kind: WeaponType::Particle,
                        mount: WeaponMount::Bay(size),
                    },
                    &mut rng,
                );
            }
            match size {
                BaySize::Small => (),
                BaySize::Medium => assert!(
                    defender.current_hull < 140,
                    "Medium bay should do more damage than {}",
                    200 - defender.current_hull
                ),
                BaySize::Large => assert!(
                    defender.current_hull < 80,
                    "Large bay should do more damage than {}",
                    200 - defender.current_hull
                ),
            }
            assert!(effects
                .iter()
                .any(|e| matches!(e, EffectMsg::Message { .. })));
        }
    }
}

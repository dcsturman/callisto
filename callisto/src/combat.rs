use crate::damage_tables::{EXTERNAL_DAMAGE_TABLE, HIT_DAMAGE_TABLE, INTERNAL_DAMAGE_TABLE};

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::entity::Entity;
use crate::payloads::{EffectMsg, FireAction, LaunchMissileMsg};
use crate::ship::Ship;
use rand::RngCore;
use serde::{Deserialize, Serialize};

const DIE_SIZE: u32 = 6;

pub fn roll(rng: &mut dyn RngCore) -> usize {
    (rng.next_u32() % DIE_SIZE + rng.next_u32() % DIE_SIZE + 2) as usize
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Weapon {
    Beam = 0,
    Pulse,
    Missile,
}

impl From<Weapon> for String {
    fn from(w: Weapon) -> Self {
        match w {
            Weapon::Beam => "Beam Laser".to_string(),
            Weapon::Pulse => "Pulse Laser".to_string(),
            Weapon::Missile => "Missile".to_string(),
        }
    }
}

impl From<&Weapon> for String {
    fn from(w: &Weapon) -> Self {
        match w {
            Weapon::Beam => "Beam Laser".to_string(),
            Weapon::Pulse => "Pulse Laser".to_string(),
            Weapon::Missile => "Missile".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub enum ShipSystem {
    Hull = 0,
    Structure,
    Armor,
    Jump,
    Manuever,
    Powerplant,
    Fuel,
    Computer,
    Crew,
    Turret,
    Sensors,
    Bridge,
    Hold,
}

impl From<ShipSystem> for String {
    fn from(s: ShipSystem) -> Self {
        match s {
            ShipSystem::Hull => "hull".to_string(),
            ShipSystem::Structure => "structure".to_string(),
            ShipSystem::Armor => "armor".to_string(),
            ShipSystem::Jump => "jump drive".to_string(),
            ShipSystem::Manuever => "maneuver drive".to_string(),
            ShipSystem::Powerplant => "power plant".to_string(),
            ShipSystem::Computer => "computer".to_string(),
            ShipSystem::Crew => "crew".to_string(),
            ShipSystem::Turret => "a turret".to_string(),
            ShipSystem::Sensors => "sensors".to_string(),
            ShipSystem::Fuel => "fuel".to_string(),
            ShipSystem::Hold => "hold".to_string(),
            ShipSystem::Bridge => "bridge".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct HdEntry {
    pub top_range: usize,
    pub single_hits: usize,
    pub double_hits: usize,
    pub triple_hits: usize,
}

impl HdEntry {
    pub fn new(
        top_range: usize,
        single_hits: usize,
        double_hits: usize,
        triple_hits: usize,
    ) -> Self {
        Self {
            top_range,
            single_hits,
            double_hits,
            triple_hits,
        }
    }
}
pub type HdEntryTable = [HdEntry; 12];

fn damage_lookup(table: &HdEntryTable, roll: usize) -> HdEntry {
    let table_top = table.iter().next_back().unwrap().top_range;
    let mut extra_single = 0;
    let mut extra_double = 0;

    if roll > table_top {
        extra_single = (roll - table_top) / 3;
        extra_double = (roll - table_top) / 6;
    }
    let mod_roll = usize::min(roll, table_top);
    let mut entry = table
        .iter()
        .find(|entry| mod_roll <= entry.top_range)
        .unwrap()
        .clone();
    entry.single_hits += extra_single;
    entry.double_hits += extra_double;
    entry
}

pub fn do_fire_actions(
    attacker: &str,
    ships: &mut HashMap<String, Arc<RwLock<Ship>>>,
    actions: &[FireAction],
    rng: &mut dyn RngCore,
) -> (Vec<LaunchMissileMsg>, Vec<EffectMsg>) {
    let mut new_missiles = vec![];
    let effects = actions
        .iter()
        .flat_map(|action| {
            let mut target = ships.get(&action.target).unwrap().write().unwrap();
            let weapon = action.kind.clone();
            match weapon {
                Weapon::Missile => {
                    // Missiles don't actually attack when fired.  They'll come back and call the attack function on impact.
                    new_missiles.push(LaunchMissileMsg {
                        source: attacker.to_string(),
                        target: target.get_name().to_string(),
                    });
                    vec![]
                }
                _ => {
                    debug!(
                        "(Combat.do_fire_actions) {} fires {} at {}.",
                        attacker,
                        String::from(&weapon),
                        target.get_name()
                    );
                    let mut effects: Vec<EffectMsg> = vec![EffectMsg::BeamHit {
                        origin: ships.get(attacker).unwrap().read().unwrap().get_position(),
                        position: target.get_position(),
                    }];
                    effects.append(&mut attack(
                        0,
                        0,
                        attacker,
                        &mut target,
                        action.kind.clone(),
                        rng
                    ));
                    effects
                }
            }
        })
        .collect();

    (new_missiles, effects)
}

fn sat_sub_eq(a: &mut u8, b: u8) {
    *a = u8::saturating_sub(*a, b);
}

fn hull_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.hull >= damage {
        sat_sub_eq(&mut defender.hull, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "hull",
        )]
    } else {
        let mut effects = do_damage(
            INTERNAL_DAMAGE_TABLE[table_pos].clone(),
            damage - defender.hull,
            attacker_name,
            defender,
            weapon.clone(),
            table_pos,
            rng,
        );
        if defender.hull > 0 {
            effects.push(EffectMsg::from_damage(
                attacker_name,
                defender,
                defender.hull,
                String::from(&weapon).as_str(),
                "hull",
            ));
        }
        defender.hull = 0;
        effects
    }
}

fn armor_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.usp.armor >= damage {
        sat_sub_eq(&mut defender.usp.armor, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "armor",
        )]
    } else {
        let mut effects = hull_hit(
            damage - defender.usp.armor,
            attacker_name,
            defender,
            weapon.clone(),
            table_pos,
            rng,
        );
        if defender.usp.armor > 0 {
            effects.push(EffectMsg::from_damage(
                attacker_name,
                defender,
                defender.usp.armor,
                String::from(&weapon).as_str(),
                "armor",
            ));
        }
        defender.usp.armor = 0;
        effects
    }
}

fn structure_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    _table_pos: usize,
    _rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.structure >= damage {
        sat_sub_eq(&mut defender.structure, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "structure",
        )]
    } else {
        defender.structure = 0;
        vec![
            EffectMsg::from_damage(
                attacker_name,
                defender,
                damage,
                String::from(&weapon).as_str(),
                "structure",
            ),
            EffectMsg::ShipImpact {
                position: defender.get_position(),
            },
        ]
    }
}

fn turret_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    (0..damage)
        .flat_map(|_| {
            let total_turrets: u8 = defender.usp.beam
                + defender.usp.pulse
                + defender.usp.particle
                + defender.usp.missile
                + defender.usp.sand;
            if total_turrets == 0 {
                return hull_hit(
                    damage,
                    attacker_name,
                    defender,
                    weapon.clone(),
                    table_pos,
                    rng,
                );
            }
            let turret = rng.next_u32() as u8 % total_turrets;
            let damage_loc_name;
            if turret < defender.usp.beam {
                damage_loc_name = "beam turret";
                sat_sub_eq(&mut defender.usp.beam, damage);
            } else if turret < defender.usp.beam + defender.usp.pulse {
                damage_loc_name = "pulse turret";
                sat_sub_eq(&mut defender.usp.pulse, damage);
            } else if turret < defender.usp.beam + defender.usp.pulse + defender.usp.particle {
                damage_loc_name = "particle turret";
                sat_sub_eq(&mut defender.usp.particle, damage);
            } else if turret
                < defender.usp.beam
                    + defender.usp.pulse
                    + defender.usp.particle
                    + defender.usp.missile
            {
                damage_loc_name = "missile turret";
                sat_sub_eq(&mut defender.usp.missile, damage);
            } else {
                damage_loc_name = "sand turret";
                sat_sub_eq(&mut defender.usp.sand, damage);
            }
            vec![EffectMsg::from_damage(
                attacker_name,
                defender,
                damage,
                String::from(&weapon).as_str(),
                damage_loc_name,
            )]
        })
        .collect()
}

fn jump_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.usp.jump > damage {
        sat_sub_eq(&mut defender.usp.jump, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "jump drive",
        )]
    } else {
        defender.usp.jump = 0;
        structure_hit(
            damage - defender.usp.jump,
            attacker_name,
            defender,
            weapon,
            table_pos,
            rng,
        )
    }
}

fn maneuver_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.usp.maneuver > damage {
        sat_sub_eq(&mut defender.usp.maneuver, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "maneuver drive",
        )]
    } else {
        defender.usp.maneuver = 0;
        hull_hit(
            damage - defender.usp.maneuver,
            attacker_name,
            defender,
            weapon,
            table_pos,
            rng,
        )
    }
}

fn powerplant_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    if defender.usp.powerplant > damage {
        sat_sub_eq(&mut defender.usp.powerplant, damage);
        vec![EffectMsg::from_damage(
            attacker_name,
            defender,
            damage,
            String::from(&weapon).as_str(),
            "power plant",
        )]
    } else {
        defender.usp.powerplant = 0;
        structure_hit(
            damage - defender.usp.powerplant,
            attacker_name,
            defender,
            weapon,
            table_pos,
            rng,
        )
    }
}

fn sensors_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    //TODO: Have this actually do something once we have sensors on ships.
    hull_hit(damage, attacker_name, defender, weapon, table_pos, rng)
}

fn bridge_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    //TODO: Have this actually do something once we have bridge and crew impacts
    structure_hit(damage, attacker_name, defender, weapon, table_pos, rng)
}

fn fuel_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    //TODO: Have this actually do something once we have fuel
    hull_hit(damage, attacker_name, defender, weapon, table_pos, rng)
}

fn hold_hit(
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    //TODO: Have this actually do something once we have hold
    structure_hit(damage, attacker_name, defender, weapon, table_pos, rng)
}

fn do_damage(
    location: ShipSystem,
    damage: u8,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    table_pos: usize,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {
    debug!(
        "(Combat.do_damage) {} does {:?} damage to {}'s {}.",
        attacker_name,
        damage,
        defender.get_name(),
        String::from(location.clone())
    );
    match location {
        ShipSystem::Hull => hull_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Armor => armor_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Structure => {
            structure_hit(damage, attacker_name, defender, weapon, table_pos, rng)
        }
        ShipSystem::Jump => jump_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Manuever => {
            maneuver_hit(damage, attacker_name, defender, weapon, table_pos, rng)
        }
        ShipSystem::Powerplant => {
            powerplant_hit(damage, attacker_name, defender, weapon, table_pos, rng)
        }
        ShipSystem::Computer => unimplemented!(),
        ShipSystem::Crew => unimplemented!(),
        ShipSystem::Turret => turret_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Sensors => sensors_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Fuel => fuel_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Hold => hold_hit(damage, attacker_name, defender, weapon, table_pos, rng),
        ShipSystem::Bridge => bridge_hit(damage, attacker_name, defender, weapon, table_pos, rng),
    }
}

pub fn attack(
    _hit_mod: i16,
    _damage_mod: i16,
    attacker_name: &str,
    defender: &mut Ship,
    weapon: Weapon,
    rng: &mut dyn RngCore,
) -> Vec<EffectMsg> {

    let damage_roll: usize = rng.next_u32() as usize % HIT_DAMAGE_TABLE.len();
    let damage = damage_lookup(&HIT_DAMAGE_TABLE, damage_roll);
    debug!(
        "(Combat.attack) Damage roll {} for {:?} damage using {:?}.",
        damage_roll, damage, weapon
    );

    let mut effects: Vec<EffectMsg> = (0..damage.single_hits)
        .flat_map(|_| {
            let roll = roll(rng);
            let location = EXTERNAL_DAMAGE_TABLE[roll].clone();
            do_damage(
                location,
                1,
                attacker_name,
                defender,
                weapon.clone(),
                roll,
                rng,
            )
        })
        .collect();

    effects.append(
        &mut (0..damage.double_hits)
            .flat_map(|_| {
                let roll = roll(rng);
                let location = EXTERNAL_DAMAGE_TABLE[roll].clone();
                do_damage(
                    location,
                    2,
                    attacker_name,
                    defender,
                    weapon.clone(),
                    roll,
                    rng,
                )
            })
            .collect(),
    );

    effects.append(
        &mut (0..damage.triple_hits)
            .flat_map(|_| {
                let roll = roll(rng);
                let location = EXTERNAL_DAMAGE_TABLE[roll].clone();
                do_damage(
                    location,
                    3,
                    attacker_name,
                    defender,
                    weapon.clone(),
                    roll,
                    rng,
                )
            })
            .collect(),
    );

    effects
}

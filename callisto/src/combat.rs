use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::entity::Entity;
use crate::payloads::{EffectMsg, FireAction, LaunchMissileMsg};
use crate::ship::Ship;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone)]
pub enum ShipSystem {
    Hull = 0,
    Structure,
    Armor,
    Jump,
    Manuever,
    Powerplant,
    Computer,
    Crew,
    Beam,
    Pulse,
    Particle,
    Missile,
    Sand,
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
            ShipSystem::Beam => "beam lasers".to_string(),
            ShipSystem::Pulse => "pulse lasers".to_string(),
            ShipSystem::Particle => "particle accelerators".to_string(),
            ShipSystem::Missile => "missile launchers".to_string(),
            ShipSystem::Sand => "sand casters".to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Damage {
    pub location: ShipSystem,
    pub amount: u8,
}

pub fn do_fire_actions(
    attacker: &str,
    ships: &mut HashMap<String, Arc<RwLock<Ship>>>,
    actions: Vec<FireAction>,

) -> (Vec<LaunchMissileMsg>, Vec<EffectMsg>) {

    let mut new_missiles = vec![];
    let effects = actions
        .iter()
        .map(|action| {
            let mut target = ships
                .get_mut(&action.target)
                .unwrap()
                .write()
                .unwrap();
            let weapon = action.kind.clone();
            match weapon {
                Weapon::Missile => {
                // Missiles don't actually attack when fired.  They'll come back and call the attack function on impact.
                new_missiles.push(LaunchMissileMsg { source: attacker.to_string(), target: target.get_name().to_string() });
                vec![]
                }
                _ => attack(0, 0, &attacker, &mut target, action.kind.clone())
            }
        })
        .flatten()
        .collect();

    (new_missiles, effects)
}

fn sat_sub_eq(a: &mut u8, b: u8) {
    *a = u8::saturating_sub(*a, b);
}

pub fn attack(hit_mod: i16, damage_mod: i16, attacker_name: &str, defender: &mut Ship, weapon: Weapon) -> Vec<EffectMsg> {
    let mut results = Vec::new();
    let mut effects = Vec::new();

    // TODO: Replace with proper damage tables
    results.push(Damage {
        location: ShipSystem::Hull,
        amount: 1,
    });

    results.iter().for_each(|damage| {
        effects.push(EffectMsg::ShipImpact { position: defender.get_position().clone() } );

        let weapon_name: String = weapon.clone().into();

        match damage.location {
            ShipSystem::Hull => sat_sub_eq(&mut defender.hull, damage.amount),
            ShipSystem::Armor => sat_sub_eq(&mut defender.usp.armor, damage.amount),
            ShipSystem::Structure => sat_sub_eq(&mut defender.structure, damage.amount),
            ShipSystem::Jump => sat_sub_eq(&mut defender.usp.jump, damage.amount),
            ShipSystem::Manuever => sat_sub_eq(&mut defender.usp.maneuver, damage.amount),
            ShipSystem::Powerplant => sat_sub_eq( &mut defender.usp.powerplant,  damage.amount),
            ShipSystem::Computer => sat_sub_eq(&mut defender.usp.computer, damage.amount),
            ShipSystem::Crew => sat_sub_eq(&mut defender.usp.crew, damage.amount),
            ShipSystem::Beam => sat_sub_eq(&mut defender.usp.beam, damage.amount),
            ShipSystem::Pulse => sat_sub_eq( &mut defender.usp.pulse, damage.amount),
            ShipSystem::Particle => sat_sub_eq(&mut defender.usp.particle, damage.amount),
            ShipSystem::Missile => sat_sub_eq(&mut defender.usp.missile, damage.amount),
            ShipSystem::Sand => sat_sub_eq(&mut defender.usp.sand, damage.amount),
        };

        let damage_loc_name: String = damage.location.clone().into();

        effects.push(EffectMsg::Damage { content: format!(
            "{} did {} {} damage to {}'s {}",
            attacker_name,
            damage.amount,
            weapon_name,
            defender.get_name(),
            damage_loc_name
        ) as String });
    });
    effects
}

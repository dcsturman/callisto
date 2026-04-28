use std::collections::{HashMap, HashSet};
use std::hash::BuildHasher;
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};

use crate::debug;
use crate::entity::Entities;
use crate::ship::{Ship, ShipSystem};

/// Identifies a specific queued action that a captain can boost. Mirrors the
/// shape of the underlying `ShipAction` for the kinds that are eligible to
/// receive a +1 die-roll bonus from leadership. Hash + Eq so the resolver can
/// pool boosts via a `HashSet`.
///
/// Jump is intentionally *not* a separate variant: as of the engineer-class
/// merge, Jump is one of the engineer actions and is boosted via
/// `BoostTarget::Engineer`.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Hash)]
pub enum BoostTarget {
  Fire { ship: String, weapon_id: usize },
  PointDefense { ship: String, weapon_id: usize },
  Sensor { ship: String },
  Engineer { ship: String },
  Evade { ship: String },
  AssistGunner { ship: String },
}

/// Pool of active boosts for the current resolution turn. Built in
/// `player.update()` Phase 0 from queued `ShipAction::LeadershipCheck`s and
/// passed by reference into per-category resolvers. Multi-captain stacking
/// is intentionally NOT supported: duplicate targets collapse to a single +1.
pub type BoostMap = HashSet<BoostTarget>;

/// Returns the boost (`+1` if present, else `0`) for a sensor-class action on
/// `ship_name`. Helper used by the sensor / engineer / jump resolvers.
#[must_use]
pub fn boost_for_sensor(map: &BoostMap, ship_name: &str) -> i16 {
  i16::from(map.contains(&BoostTarget::Sensor {
    ship: ship_name.to_string(),
  }))
}

#[must_use]
pub fn boost_for_engineer(map: &BoostMap, ship_name: &str) -> i16 {
  i16::from(map.contains(&BoostTarget::Engineer {
    ship: ship_name.to_string(),
  }))
}

#[must_use]
pub fn boost_for_evade(map: &BoostMap, ship_name: &str) -> i16 {
  i16::from(map.contains(&BoostTarget::Evade {
    ship: ship_name.to_string(),
  }))
}

#[must_use]
pub fn boost_for_assist_gunner(map: &BoostMap, ship_name: &str) -> i16 {
  i16::from(map.contains(&BoostTarget::AssistGunner {
    ship: ship_name.to_string(),
  }))
}

#[must_use]
pub fn boost_for_fire(map: &BoostMap, ship_name: &str, weapon_id: usize) -> i16 {
  i16::from(map.contains(&BoostTarget::Fire {
    ship: ship_name.to_string(),
    weapon_id,
  }))
}

#[must_use]
pub fn boost_for_point_defense(map: &BoostMap, ship_name: &str, weapon_id: usize) -> i16 {
  i16::from(map.contains(&BoostTarget::PointDefense {
    ship: ship_name.to_string(),
    weapon_id,
  }))
}

/// Stable ordinal ordering for `BoostTarget` kinds: `Fire` < `PointDefense`
/// < `Sensor` < `Engineer` < `Evade` < `AssistGunner`. Used for the
/// deterministic boost-truncation ordering in the leadership phase.
#[must_use]
pub fn boost_target_kind_ord(t: &BoostTarget) -> u8 {
  match t {
    BoostTarget::Fire { .. } => 0,
    BoostTarget::PointDefense { .. } => 1,
    BoostTarget::Sensor { .. } => 2,
    BoostTarget::Engineer { .. } => 3,
    BoostTarget::Evade { .. } => 4,
    BoostTarget::AssistGunner { .. } => 5,
  }
}

/// Sort key tuple for `BoostTarget`: (ship name, kind ordinal, weapon id).
/// `weapon_id` defaults to 0 for kinds that don't carry one so they all sort
/// consistently.
#[must_use]
pub fn boost_target_sort_key(t: &BoostTarget) -> (String, u8, usize) {
  let (ship, weapon) = match t {
    BoostTarget::Fire { ship, weapon_id } | BoostTarget::PointDefense { ship, weapon_id } => (ship.clone(), *weapon_id),
    BoostTarget::Sensor { ship }
    | BoostTarget::Engineer { ship }
    | BoostTarget::Evade { ship }
    | BoostTarget::AssistGunner { ship } => (ship.clone(), 0),
  };
  (ship, boost_target_kind_ord(t), weapon)
}

/// Returns true if the boost target is "alive" — for action-based targets,
/// there is a matching queued action in `actions`; for pilot-state targets
/// (Evade / `AssistGunner`), the underlying ship state is set. Used by Phase
/// 0 to drop boosts whose target is no longer applicable.
///
/// # Panics
/// Panics if a ship lock cannot be acquired for read.
#[must_use]
pub fn boost_target_alive<S: BuildHasher>(
  target: &BoostTarget, actions: &ShipActionList, ships: &HashMap<String, Arc<RwLock<Ship>>, S>,
) -> bool {
  match target {
    BoostTarget::Evade { ship } => {
      let Some(ship_lock) = ships.get(ship) else {
        return false;
      };
      let ship_ref = ship_lock.read().expect("(boost_target_alive) Unable to read ship lock.");
      return ship_ref.get_dodge_thrust() > 0;
    }
    BoostTarget::AssistGunner { ship } => {
      let Some(ship_lock) = ships.get(ship) else {
        return false;
      };
      let ship_ref = ship_lock.read().expect("(boost_target_alive) Unable to read ship lock.");
      return ship_ref.get_assist_gunners();
    }
    _ => {}
  }

  for (ship_name, ship_actions) in actions {
    match target {
      BoostTarget::Fire { ship, weapon_id } => {
        if ship_name == ship
          && ship_actions
            .iter()
            .any(|a| matches!(a, ShipAction::FireAction { weapon_id: w, .. } if w == weapon_id))
        {
          return true;
        }
      }
      BoostTarget::PointDefense { ship, weapon_id } => {
        if ship_name == ship
          && ship_actions
            .iter()
            .any(|a| matches!(a, ShipAction::PointDefenseAction { weapon_id: w } if w == weapon_id))
        {
          return true;
        }
      }
      BoostTarget::Sensor { ship } => {
        if ship_name == ship
          && ship_actions.iter().any(|a| {
            matches!(
              a,
              ShipAction::JamMissiles
                | ShipAction::BreakSensorLock { .. }
                | ShipAction::SensorLock { .. }
                | ShipAction::JamComms { .. }
            )
          })
        {
          return true;
        }
      }
      BoostTarget::Engineer { ship } => {
        if ship_name == ship && ship_actions.iter().any(is_engineer_action) {
          return true;
        }
      }
      // Handled above — pilot-state targets don't depend on the action queue.
      BoostTarget::Evade { .. } | BoostTarget::AssistGunner { .. } => {}
    }
  }
  false
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum ShipAction {
  FireAction {
    weapon_id: usize,
    target: String,
    #[serde(
      default,
      skip_serializing_if = "Option::is_none",
      //with = "::serde_with::rust::unwrap_or_skip"
  )]
    called_shot_system: Option<ShipSystem>,
  },
  PointDefenseAction {
    weapon_id: usize,
  },
  DeleteFireAction {
    weapon_id: usize,
  },
  JamMissiles,
  BreakSensorLock {
    target: String,
  },
  SensorLock {
    target: String,
  },
  JamComms {
    target: String,
  },
  Jump,
  // Engineer actions
  OverloadDrive,
  OverloadPlant,
  Repair {
    system: ShipSystem,
  },
  /// Captain-only action queued under the captain's own ship. Bundles the
  /// 2d6+leadership pre-resolution roll and the list of targets to apply +1
  /// boosts to. Resolved in `player.update()` Phase 0 before any other
  /// action category.
  LeadershipCheck {
    boosts: Vec<BoostTarget>,
  },
  // Anti-actions: explicit "clear queued sensor/engineer action" intents.
  // Sent by the client when the user cancels a queued action (chooser → none,
  // or click-to-remove from the Actions list). Consumed in `merge`; never
  // stored in the queue.
  ClearSensorAction,
  ClearEngineerAction,
  /// Anti-action mirroring `ClearSensorAction` / `ClearEngineerAction`. When
  /// merged, strips any prior `LeadershipCheck` from the queue and is itself
  /// dropped.
  ClearLeadershipCheck,
}

/// Returns true if the action is an engineer action.
///
/// Jump is included: it consumes the engineer's turn just like the other
/// engineer actions, is mutually exclusive with them in `merge`, and is
/// resolved alongside them in `engineer_actions`.
#[must_use]
pub fn is_engineer_action(action: &ShipAction) -> bool {
  matches!(
    action,
    ShipAction::OverloadDrive | ShipAction::OverloadPlant | ShipAction::Repair { .. } | ShipAction::Jump
  )
}

pub type ShipActionList = Vec<(String, Vec<ShipAction>)>;

/// Merge the new actions into the existing actions in the entities.
///
/// # Arguments
/// * `entities` - The entities to merge the actions into.  We pass this as we need both ship data and the existing actions.
/// * `new_actions` - The new actions to merge into the entities.
///
/// # Panics
/// Panics if the lock cannot be obtained to read a ship.
#[allow(clippy::too_many_lines)]
pub fn merge(entities: &mut Entities, new_actions: ShipActionList) {
  let ships = &entities.ships;
  let current = &mut entities.actions;
  // For each ship in the new action set
  for (next_ship, next_action_list) in new_actions {
    // Find if that ship has planned actions.  If so, ensure we merge correctly without multiple conflicting orders.
    // If not, just add the new actions to this ship.
    if let Some((_, current_actions)) = current.iter_mut().find(|(ship_name, _)| ship_name == &next_ship) {
      for next_action in next_action_list {
        match next_action {
          // Each sensor action replaces the previous sensor action (there can only be one per ship)
          ShipAction::JamMissiles
          | ShipAction::BreakSensorLock { .. }
          | ShipAction::SensorLock { .. }
          | ShipAction::JamComms { .. } => {
            // Strip out all sensor actions, leaving just the non-sensor actions
            current_actions.retain(|action| {
              !matches!(
                action,
                ShipAction::JamMissiles
                  | ShipAction::BreakSensorLock { .. }
                  | ShipAction::SensorLock { .. }
                  | ShipAction::JamComms { .. }
              )
            });
            current_actions.push(next_action.clone());
          }
          // Each fire action is added to the list of fire actions, but only if the weapon is not already in use.
          ShipAction::FireAction { weapon_id, .. } | ShipAction::PointDefenseAction { weapon_id } => {
            current_actions.retain(|action| {
              !matches!(action, ShipAction::FireAction{weapon_id: id, ..} if *id == weapon_id)
                && !matches!(action, ShipAction::PointDefenseAction{weapon_id: id} if *id == weapon_id)
            });
            current_actions.push(next_action.clone());
          }
          // Delete the noted action.  Given the way we merge just omitting the FireAction or PointDefenseAction does not delete it.
          // We need a specific "anti-action" so we can differentiate between a client that is just missing some information and
          // one that actually wants to eliminate the action.
          ShipAction::DeleteFireAction { weapon_id } => {
            let Some(ship_lock) = &ships.get(&next_ship) else {
              continue;
            };
            let ship = ship_lock.read().expect("(Action.merge) Unable to read ship lock.");
            let current_template = &ship.design;
            let weapon = &current_template.weapons[weapon_id];
            // Find a _similar_ weapon to the one being deleted and delete the highest number of that (to avoid race conditions)
            let mut sorted_similar_weapon_id = current_actions
              .iter()
              .filter_map(|action| match action {
                ShipAction::PointDefenseAction { weapon_id } | ShipAction::FireAction { weapon_id, .. } => {
                  if current_template.weapons[*weapon_id] == *weapon {
                    Some(*weapon_id)
                  } else {
                    None
                  }
                }
                _ => None,
              })
              .collect::<Vec<_>>();
            sorted_similar_weapon_id.sort_unstable();
            // Be defensive... if there are no similar weapons, then just continue in the loop.
            if sorted_similar_weapon_id.is_empty() {
              continue;
            }
            let max_similar_weapon_id = sorted_similar_weapon_id.last().unwrap();

            // Retain everything except the FireAction with the highest number of the similar weapon
            current_actions.retain(|action| {
              !matches!(action, ShipAction::FireAction{weapon_id, ..} if max_similar_weapon_id == weapon_id)
                && !matches!(action, ShipAction::PointDefenseAction{weapon_id} if max_similar_weapon_id == weapon_id)
            });
          }
          // Engineer actions are mutually exclusive - only one engineer action per turn.
          // A new engineer action replaces any existing engineer action.
          // Jump is treated as an engineer action.
          ShipAction::OverloadDrive | ShipAction::OverloadPlant | ShipAction::Repair { .. } | ShipAction::Jump => {
            current_actions.retain(|action| !is_engineer_action(action));
            current_actions.push(next_action.clone());
          }
          // Each leadership check replaces the previous one (only one per ship per turn).
          ShipAction::LeadershipCheck { .. } => {
            current_actions.retain(|action| !matches!(action, ShipAction::LeadershipCheck { .. }));
            current_actions.push(next_action.clone());
          }
          // Anti-actions: strip the matching kind, don't push anything.
          ShipAction::ClearSensorAction => {
            current_actions.retain(|action| {
              !matches!(
                action,
                ShipAction::JamMissiles
                  | ShipAction::BreakSensorLock { .. }
                  | ShipAction::SensorLock { .. }
                  | ShipAction::JamComms { .. }
              )
            });
          }
          ShipAction::ClearEngineerAction => {
            current_actions.retain(|action| !is_engineer_action(action));
          }
          ShipAction::ClearLeadershipCheck => {
            current_actions.retain(|action| !matches!(action, ShipAction::LeadershipCheck { .. }));
          }
        }
      }
    } else {
      // No prior actions for this ship — anti-actions have nothing to strip,
      // so drop them. Keep all other actions verbatim.
      let filtered: Vec<ShipAction> = next_action_list
        .into_iter()
        .filter(|a| {
          !matches!(
            a,
            ShipAction::ClearSensorAction | ShipAction::ClearEngineerAction | ShipAction::ClearLeadershipCheck
          )
        })
        .collect();
      if !filtered.is_empty() {
        current.push((next_ship, filtered));
      }
    }
  }

  debug!("(Action.merge) Merged actions are {:?}", entities.actions);
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::entity::Vec3;
  use crate::ship::{Ship, ShipDesignTemplate};

  fn make_ship_map(name: &str, dodge: u8, assist: bool) -> HashMap<String, Arc<RwLock<Ship>>> {
    let mut ship = Ship::new(
      name.to_string(),
      Vec3::new(0.0, 0.0, 0.0),
      Vec3::new(0.0, 0.0, 0.0),
      &Arc::new(ShipDesignTemplate::default()),
      None,
    );
    if dodge > 0 {
      ship
        .set_pilot_actions(Some(dodge), Some(assist))
        .expect("(test) set_pilot_actions failed");
    } else if assist {
      ship
        .set_pilot_actions(None, Some(assist))
        .expect("(test) set_pilot_actions failed");
    }
    let mut map: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    map.insert(name.to_string(), Arc::new(RwLock::new(ship)));
    map
  }

  #[test]
  fn test_boost_target_alive_evade_requires_dodge_thrust() {
    // Ship exists, dodge_thrust = 0 -> not alive.
    let ships = make_ship_map("S1", 0, false);
    let target = BoostTarget::Evade { ship: "S1".to_string() };
    let actions: ShipActionList = vec![];
    assert!(!boost_target_alive(&target, &actions, &ships));

    // Ship exists, dodge_thrust > 0 -> alive.
    let ships = make_ship_map("S1", 2, false);
    assert!(boost_target_alive(&target, &actions, &ships));

    // Ship missing -> not alive.
    let empty: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    assert!(!boost_target_alive(&target, &actions, &empty));
  }

  #[test]
  fn test_boost_target_alive_assist_gunner_requires_flag() {
    // Ship exists, assist_gunners = false -> not alive.
    let ships = make_ship_map("S1", 0, false);
    let target = BoostTarget::AssistGunner { ship: "S1".to_string() };
    let actions: ShipActionList = vec![];
    assert!(!boost_target_alive(&target, &actions, &ships));

    // Ship exists, assist_gunners = true -> alive.
    let ships = make_ship_map("S1", 0, true);
    assert!(boost_target_alive(&target, &actions, &ships));

    // Ship missing -> not alive.
    let empty: HashMap<String, Arc<RwLock<Ship>>> = HashMap::new();
    assert!(!boost_target_alive(&target, &actions, &empty));
  }
}

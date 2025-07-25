use serde::{Deserialize, Serialize};

use crate::debug;
use crate::entity::Entities;
use crate::ship::{ShipSystem, SHIP_TEMPLATES};

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
            let design = &ships.get(&next_ship).unwrap().read().unwrap().design;
            let current_template = SHIP_TEMPLATES
              .get()
              .expect("Ship templates not loaded,")
              .get(&design.name)
              .expect("(Action.merge) Unable to find design.");
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
          // When merging ensure only one Jump action remains in the merged list.  Should never actually happen
          // because there's no way to remove a jump action (but UX may be buggy and add it twice).
          ShipAction::Jump => {
            if !current_actions.iter().any(|action| matches!(action, ShipAction::Jump)) {
              current_actions.push(next_action.clone());
            }
          }
        }
      }
    } else {
      current.push((next_ship, next_action_list));
    }
  }

  debug!("(Action.merge) Merged actions are {:?}", entities.actions);
}

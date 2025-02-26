use serde::{Deserialize, Serialize};

use crate::ship::ShipSystem;

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
}

pub type ShipActionList = Vec<(String, Vec<ShipAction>)>;

pub fn merge(current: &mut ShipActionList, other: ShipActionList) {
  // For each ship in the new action set
  for (next_ship, next_action_list) in other {
    // Find if that ship as planned actions.  If so, ensure we merge correctly without multiple conflicting orders.
    // If not, just add the new actions to this ship.
    if let Some((_, current_actions)) = current.iter_mut().find(|(ship_name, _)| ship_name == &next_ship) {
      for next_action in next_action_list {
        match next_action {
          ShipAction::JamMissiles
          | ShipAction::BreakSensorLock { .. }
          | ShipAction::SensorLock { .. }
          | ShipAction::JamComms { .. } => {
            // Strip out all sensor actions, leaving just the FireActions
            current_actions.retain(|action| matches!(action, ShipAction::FireAction { .. }));
            current_actions.push(next_action.clone());
          }
          ShipAction::FireAction { weapon_id, .. } => {
            current_actions
              .retain(|action| !matches!(action, ShipAction::FireAction{weapon_id: id, ..} if *id == weapon_id));
            current_actions.push(next_action.clone());
          }
        }
      }
    } else {
      current.push((next_ship, next_action_list));
    }
  }
}

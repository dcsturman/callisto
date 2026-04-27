import * as React from "react";
import { useMemo } from "react";
import {
  Ship,
  ShipSystem,
  shipSystemToString,
  stringToShipSystem,
} from "lib/entities";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { setEngineerAction } from "state/actionsSlice";
import { EngineerState } from "components/controls/Actions";

// Map ShipSystem enum to display names (must match backend order)
const SYSTEM_NAMES: Record<ShipSystem, string> = {
  [ShipSystem.Sensors]: "Sensors",
  [ShipSystem.Powerplant]: "Power Plant",
  [ShipSystem.Fuel]: "Fuel",
  [ShipSystem.Weapon]: "Weapon",
  [ShipSystem.Armor]: "Armor",
  [ShipSystem.Hull]: "Hull",
  [ShipSystem.Maneuver]: "Maneuver Drive",
  [ShipSystem.Cargo]: "Cargo",
  [ShipSystem.Jump]: "Jump Drive",
  [ShipSystem.Crew]: "Crew",
  [ShipSystem.Bridge]: "Bridge",
};

interface EngineerTasksProps {
  ship: Ship;
}

export const EngineerTasks: React.FC<EngineerTasksProps> = ({ ship }) => {
  const dispatch = useAppDispatch();
  // Engineer action queued for end-of-turn evaluation. We display the result
  // through the same Effects channel as combat / sensor effects, so this
  // component no longer holds a local result state — it just queues.
  const queuedEngineer = useAppSelector(
    (state) => state.actions[ship.name]?.engineer ?? null,
  );

  // Get list of damaged systems (excluding Hull, Armor, and Crew which cannot be repaired)
  const damagedSystems = useMemo(() => {
    if (!ship.crit_level) return [];
    return ship.crit_level
      .map((level, index) => ({ system: index as ShipSystem, level }))
      .filter(
        (s) =>
          s.level > 0 &&
          s.system !== ShipSystem.Hull &&
          s.system !== ShipSystem.Armor &&
          s.system !== ShipSystem.Crew,
      );
  }, [ship.crit_level]);

  const queueAction = (action: EngineerState) => {
    dispatch(setEngineerAction({ shipName: ship.name, action }));
  };

  const handleOverloadDrive = () => {
    queueAction({ kind: "OverloadDrive" });
  };

  const handleOverloadPlant = () => {
    queueAction({ kind: "OverloadPlant" });
  };

  const handleRepair = (system: ShipSystem) => {
    queueAction({ kind: "Repair", system: shipSystemToString(system) });
  };

  // Calculate repair bonus for display
  const getRepairBonus = (system: ShipSystem): number => {
    if (
      ship.last_repair_component &&
      stringToShipSystem(ship.last_repair_component) === system &&
      ship.repair_bonus
    ) {
      return ship.repair_bonus;
    }
    return 0;
  };

  // An engineer action is queued (locally, in Redux). All engineer actions
  // are mutually exclusive; queueing a new one replaces any prior selection.
  const hasActionQueued = queuedEngineer != null;

  // Check if overload bonus is currently in effect on the ship (carries over
  // from a successful overload last turn).
  const hasOverloadDrive = (ship.temporary_maneuver ?? 0) > 0;
  const hasOverloadPlant = (ship.temporary_power_multiplier ?? 1.0) > 1.0;

  const queuedTooltip = "Engineer action queued for end of turn.";

  return (
    <div className="engineer-tasks">
      <h2 className="control-form">Engineer Tasks</h2>

      {/* Overload Actions */}
      <div className="engineer-overload-section">
        <h3 className="control-label">Overload Systems</h3>
        <div className="engineer-buttons">
          <button
            className="control-input control-button blue-button"
            onClick={handleOverloadDrive}
            disabled={hasOverloadDrive || hasActionQueued}
            title={hasActionQueued ? queuedTooltip : ""}
          >
            {hasOverloadDrive ? "Drive Overloaded" : "Overload Drive"}
          </button>
          <button
            className="control-input control-button blue-button"
            onClick={handleOverloadPlant}
            disabled={hasOverloadPlant || hasActionQueued}
            title={hasActionQueued ? queuedTooltip : ""}
          >
            {hasOverloadPlant ? "Plant Overloaded" : "Overload Plant"}
          </button>
        </div>
      </div>

      {/* Repair Actions */}
      <div className="engineer-repair-section">
        <h3 className="control-label">Repair Damaged Systems</h3>
        {damagedSystems.length === 0 ? (
          <p className="plan-accel-text">No damaged systems to repair</p>
        ) : (
          <select
            className="control-input"
            style={{ width: "100%", minWidth: "250px" }}
            onChange={(e) => {
              if (e.target.value) {
                handleRepair(parseInt(e.target.value) as ShipSystem);
                e.target.value = "";
              }
            }}
            disabled={hasActionQueued}
            title={hasActionQueued ? queuedTooltip : ""}
            defaultValue=""
          >
            <option value="">Select system to repair...</option>
            {damagedSystems.map(({ system, level }) => {
              const bonus = getRepairBonus(system);
              const bonusText = bonus > 0 ? ` (+${bonus} bonus)` : "";
              return (
                <option key={system} value={system}>
                  {SYSTEM_NAMES[system]} (Crit Level {level}){bonusText}
                </option>
              );
            })}
          </select>
        )}
      </div>
    </div>
  );
};

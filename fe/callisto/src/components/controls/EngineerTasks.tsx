import * as React from "react";
import { useState, useMemo, useEffect } from "react";
import {
  Ship,
  ShipSystem,
  EngineerActionResult,
  EngineerActionMsg,
  shipSystemToString,
  stringToShipSystem,
} from "lib/entities";
import { sendEngineerAction } from "lib/serverManager";

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
  const [lastResult, setLastResult] = useState<EngineerActionResult | null>(
    null,
  );

  // Clear results when ship changes
  useEffect(() => {
    setLastResult(null);
  }, [ship.name]);

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

  const handleOverloadDrive = () => {
    sendEngineerAction(
      { ship_name: ship.name, action: "OverloadDrive" },
      setLastResult,
    );
  };

  const handleOverloadPlant = () => {
    sendEngineerAction(
      { ship_name: ship.name, action: "OverloadPlant" },
      setLastResult,
    );
  };

  const handleRepair = (system: ShipSystem) => {
    const msg: EngineerActionMsg = {
      ship_name: ship.name,
      action: { Repair: { system: shipSystemToString(system) } },
    };
    console.log("(handleRepair) Sending repair action:", JSON.stringify(msg));
    sendEngineerAction(msg, setLastResult);
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

  // Check if engineer has already taken an action this turn
  const hasActionTaken = ship.engineer_action_taken ?? false;

  // Check if overload is already active
  const hasOverloadDrive = (ship.temporary_maneuver ?? 0) > 0;
  const hasOverloadPlant = (ship.temporary_power_multiplier ?? 1.0) > 1.0;

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
            disabled={hasOverloadDrive || hasActionTaken}
            title={
              hasActionTaken
                ? "Engineer has already taken an action this turn"
                : ""
            }
          >
            {hasOverloadDrive ? "Drive Overloaded" : "Overload Drive"}
          </button>
          <button
            className="control-input control-button blue-button"
            onClick={handleOverloadPlant}
            disabled={hasOverloadPlant || hasActionTaken}
            title={
              hasActionTaken
                ? "Engineer has already taken an action this turn"
                : ""
            }
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
            disabled={hasActionTaken}
            title={
              hasActionTaken
                ? "Engineer has already taken an action this turn"
                : ""
            }
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

      {/* Result Display - only show if result is for current ship */}
      {lastResult && lastResult.ship_name === ship.name && (
        <div
          className={`engineer-result ${lastResult.success ? "success" : "failure"}`}
          style={{
            marginTop: "10px",
            padding: "8px",
            backgroundColor: lastResult.success ? "#1a3d1a" : "#3d1a1a",
            borderRadius: "4px",
          }}
        >
          <p className="plan-accel-text" style={{ margin: 0 }}>
            {lastResult.message}
          </p>
          {lastResult.check !== undefined &&
            lastResult.target !== undefined && (
              <p
                className="plan-accel-text"
                style={{ margin: "4px 0 0 0", fontSize: "0.9em" }}
              >
                Check: {lastResult.check} vs Target: {lastResult.target}
              </p>
            )}
        </div>
      )}
    </div>
  );
};

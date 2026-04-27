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

// Map ShipSystem enum to display names (must match backend order)
export const SYSTEM_NAMES: Record<ShipSystem, string> = {
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

  // Check if overload bonus is currently in effect on the ship (carries over
  // from a successful overload last turn).
  const hasOverloadDrive = (ship.temporary_maneuver ?? 0) > 0;
  const hasOverloadPlant = (ship.temporary_power_multiplier ?? 1.0) > 1.0;

  // Derive controlled-select value from the queued engineer action.
  let selectedValue = "none";
  if (queuedEngineer?.kind === "OverloadDrive") {
    selectedValue = "overload-drive";
  } else if (queuedEngineer?.kind === "OverloadPlant") {
    selectedValue = "overload-plant";
  } else if (queuedEngineer?.kind === "Repair") {
    const sys = stringToShipSystem(queuedEngineer.system);
    if (sys != null) selectedValue = `repair-${sys}`;
  }

  const handleEngineerChange = (
    e: React.ChangeEvent<HTMLSelectElement>,
  ) => {
    const value = e.target.value;
    if (value === "none") {
      dispatch(setEngineerAction({ shipName: ship.name, action: null }));
    } else if (value === "overload-drive") {
      dispatch(
        setEngineerAction({
          shipName: ship.name,
          action: { kind: "OverloadDrive" },
        }),
      );
    } else if (value === "overload-plant") {
      dispatch(
        setEngineerAction({
          shipName: ship.name,
          action: { kind: "OverloadPlant" },
        }),
      );
    } else if (value.startsWith("repair-")) {
      const n = parseInt(value.substring("repair-".length), 10);
      if (!Number.isNaN(n)) {
        dispatch(
          setEngineerAction({
            shipName: ship.name,
            action: {
              kind: "Repair",
              system: shipSystemToString(n as ShipSystem),
            },
          }),
        );
      }
    }
  };

  return (
    <div className="engineer-tasks">
      <h2 className="control-form">Engineer Tasks</h2>
      <select
        className="control-input"
        value={selectedValue}
        onChange={handleEngineerChange}
        style={{ width: "100%", minWidth: "250px" }}
      >
        <option value="none"></option>
        <option value="overload-drive" disabled={hasOverloadDrive}>
          Overload Drive
        </option>
        <option value="overload-plant" disabled={hasOverloadPlant}>
          Overload Plant
        </option>
        {damagedSystems.map(({ system, level }) => {
          const bonus = getRepairBonus(system);
          const bonusText = bonus > 0 ? ` (+${bonus} bonus)` : "";
          return (
            <option key={system} value={`repair-${system}`}>
              Repair {SYSTEM_NAMES[system]} (Crit Level {level}){bonusText}
            </option>
          );
        })}
      </select>
      {hasOverloadDrive && <p className="plan-accel-text">Drive Overloaded</p>}
      {hasOverloadPlant && <p className="plan-accel-text">Plant Overloaded</p>}
      {damagedSystems.length === 0 && (
        <p className="plan-accel-text">No damaged systems</p>
      )}
    </div>
  );
};

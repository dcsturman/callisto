import React, { useContext } from "react";
import {
  Entity,
  EntitiesServerContext,
  ViewContext,
  ViewMode,
} from "./Universal";
import { EntitySelector, EntitySelectorType } from "./EntitySelector";
import { setRole } from "./ServerManager";

export const RoleChooser = () => {
  const viewContext = useContext(ViewContext);
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      <select
        className="select-dropdown control-name-input control-input role-input"
        value={viewContext.role}
        onChange={(e) => {
          viewContext.setRole(Number(e.target.value));
          setRole(Number(e.target.value), viewContext.shipName);
        }}>
        <option value={ViewMode.General}>General</option>
        <option value={ViewMode.Pilot}>Pilot</option>
        <option value={ViewMode.Sensors}>Sensors</option>
        <option value={ViewMode.Gunner}>Gunner</option>
        <option value={ViewMode.Observer}>Observer</option>
      </select>
      <EntitySelector
        className="select-dropdown control-name-input control-input role-input"
        filter={[EntitySelectorType.Ship]}
        setChoice={(ship: Entity | null) => {
          viewContext.setShipName(ship ? ship.name : null);
          setRole(viewContext.role, ship ? ship.name : null);
        }}
        current={
          serverEntities.entities.ships.find(
            (s) => s.name === viewContext.shipName
          ) ?? null
        }
      />
    </>
  );
};

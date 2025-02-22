import React, { useContext } from "react";
import { Entity, EntitiesServerContext, ViewContext, ViewMode } from "./Universal";
import { EntitySelector, EntitySelectorType } from "./EntitySelector";

export const RoleChooser = () => {
    const viewContext = useContext(ViewContext);
    const serverEntities = useContext(EntitiesServerContext);

    return (
        <>
        <select className="select-dropdown control-name-input control-input" value={viewContext.role} onChange={(e) => viewContext.setRole(Number(e.target.value)) }>
            <option value={ViewMode.General}>General</option>
            <option value={ViewMode.Pilot}>Pilot</option>
            <option value={ViewMode.Sensors}>Sensors</option>
            <option value={ViewMode.Gunner}>Gunner</option>
            <option value={ViewMode.Observer}>Observer</option>
        </select>
        <EntitySelector filter={[EntitySelectorType.Ship]} onChange={(ship: Entity | null) => viewContext.setShipName(ship ? ship.name : null)} current={serverEntities.entities.ships.find((s) => s.name === viewContext.shipName)?? null} />
        </>
    );
}
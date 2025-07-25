import * as React from "react";
import {useMemo} from "react";
import {Entity} from "lib/entities";
import {ViewMode} from "lib/view";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { requestRoleChoice } from "lib/serverManager";
import { findShip } from "lib/entities";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setRoleShip } from "state/userSlice";
import {entitiesSelector} from "state/serverSlice";

export const RoleChooser = () => {
  const shipName = useAppSelector(state => state.user.shipName);
  const role = useAppSelector(state => state.user.role);
  const entities = useAppSelector(entitiesSelector);

  const current = useMemo(() => findShip(entities, shipName),[entities, shipName]);
  const dispatch = useAppDispatch();

  const filter = useMemo(() => [EntitySelectorType.Ship],[]);
  const choiceHandler = useMemo(() =>(ship: Entity | null) => {
    dispatch(setRoleShip([role, ship ? ship.name : null]));
    requestRoleChoice(role, ship ? ship.name : null);
  },[dispatch, role]);
  
  return (
    <>
      <select
        className="select-dropdown control-name-input control-input role-input"
        value={role}
        onChange={(e) => {
          dispatch(setRoleShip([Number(e.target.value), shipName]));
          requestRoleChoice(Number(e.target.value), shipName);
        }}>
        <option value={ViewMode.General}>General</option>
        <option value={ViewMode.Pilot}>Pilot</option>
        <option value={ViewMode.Sensors}>Sensors</option>
        <option value={ViewMode.Gunner}>Gunner</option>
        <option value={ViewMode.Observer}>Observer</option>
      </select>
      <EntitySelector
        className="select-dropdown control-name-input control-input role-input"
        filter={filter}
        setChoice={choiceHandler}
        current={current}
      />
    </>
  );
};

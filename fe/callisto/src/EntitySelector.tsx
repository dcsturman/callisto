import * as React from "react";
import { useContext, useState } from "react";
import { Entity } from "./Universal";
import { EntitiesServerContext } from "./Universal";

export enum EntitySelectorType {
  Ship,
  Planet,
  Missile,
}

interface EntitySelectorProps {
  filter: EntitySelectorType[];
  onChange: (entity: Entity | null) => void;
  exclude?: string;
  initial?: string;
}

export const EntitySelector: React.FC<EntitySelectorProps> = ({filter, onChange, exclude, initial}) => {
  const entities = useContext(EntitiesServerContext).entities;
  const [selectedEntity, setSelectedEntity] = useState<Entity | null>(null);

  if (initial !== undefined) {
    const initial_entity =
      entities.ships.find((ship) => ship.name === initial) ||
      entities.planets.find((planet) => planet.name === initial) ||
      entities.missiles.find((missile) => missile.name === initial);

    if (initial_entity !== undefined) {
      setSelectedEntity(initial_entity);
    } else {
      console.error(`(EntitySelector) Initial entity ${initial} not found!`);
    }
  }

  function handleSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;

    if (filter.includes(EntitySelectorType.Ship)) {
      const shipTarget = entities.ships.find((ship) => ship.name === value);
      if (shipTarget != null) {
        setSelectedEntity(shipTarget);
        onChange(shipTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Planet)) {
      const planetTarget = entities.planets.find(
        (planet) => planet.name === value
      );
      if (planetTarget != null) {
        setSelectedEntity(planetTarget);
        onChange(planetTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Missile)) {
      const missileTarget = entities.missiles.find(
        (missile) => missile.name === value
      );
      if (missileTarget != null) {
        setSelectedEntity(missileTarget);
        onChange(missileTarget);
        return;
      }
    }

    setSelectedEntity(null);
    onChange(null);
  }

  return (
    <select
      className="select-dropdown control-name-input control-input"
      name="entity_selector"
      value={selectedEntity ? selectedEntity.name : ""}
      onChange={handleSelectChange}>
      <option key="none" value=""></option>
      {filter.includes(EntitySelectorType.Ship) &&
        entities.ships
          .filter((candidate) => candidate.name !== exclude)
          .map((notMeShip) => (
            <option key={notMeShip.name} value={notMeShip.name}>
              {notMeShip.name}
            </option>
          ))}
      {filter.includes(EntitySelectorType.Planet) &&
        entities.planets.map((planet) => (
          <option key={planet.name} value={planet.name}>
            {planet.name}
          </option>
        ))}
      {filter.includes(EntitySelectorType.Missile) &&
        entities.missiles.map((missile) => (
          <option key={missile.name} value={missile.name}>
            {missile.name}
          </option>
        ))}
    </select>
  );
};

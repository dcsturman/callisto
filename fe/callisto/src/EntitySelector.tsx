import * as React from "react";
import { useContext } from "react";
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
  current: Entity | null;
  exclude?: string;
  formatter?: (name: string, entity: Entity) => string;
}

export const EntitySelector: React.FC<EntitySelectorProps> = ({
  filter,
  onChange,
  current,
  exclude,
  formatter,
}) => {
  const entities = useContext(EntitiesServerContext).entities;

  // Create a formatter that handles one not being provided.
  const nf = (name: string, entity: Entity) =>
    formatter ? formatter(name, entity) : name;

  function handleSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;

    if (filter.includes(EntitySelectorType.Ship)) {
      const shipTarget = entities.ships.find((ship) => ship.name === value);
      if (shipTarget != null) {
        onChange(shipTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Planet)) {
      const planetTarget = entities.planets.find(
        (planet) => planet.name === value
      );
      if (planetTarget != null) {
        onChange(planetTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Missile)) {
      const missileTarget = entities.missiles.find(
        (missile) => missile.name === value
      );
      if (missileTarget != null) {
        onChange(missileTarget);
        return;
      }
    }

    onChange(null);
  }

  return (
    <>
      <select
        className="select-dropdown control-name-input control-input"
        name="entity_selector"
        value={current ? current.name : ""}
        onChange={handleSelectChange}>
        <option key="el-none" value=""></option>
        {filter.includes(EntitySelectorType.Ship) &&
          entities.ships
            .filter((candidate) => candidate.name !== exclude)
            .map((notMeShip) => (
              <option key={"els"+notMeShip.name} value={notMeShip.name}>
                {nf(notMeShip.name, notMeShip)}
              </option>
            ))}
        {filter.includes(EntitySelectorType.Planet) &&
          entities.planets
            .filter((candidate) => candidate.name !== exclude)
            .map((planet) => (
              <option key={"elp-"+planet.name} value={planet.name}>
                {nf(planet.name, planet)}
              </option>
            ))}
        {filter.includes(EntitySelectorType.Missile) &&
          entities.missiles
            .filter((candidate) => candidate.name !== exclude)
            .map((missile) => (
              <option key={"elm-"+missile.name} value={missile.name}>
                {nf(missile.name, missile)}
              </option>
            ))}
      </select>
    </>
  );
};

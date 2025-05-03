import * as React from "react";
import { useContext, useMemo } from "react";
import { Entity } from "./Universal";
import { EntitiesServerContext } from "./Universal";

export enum EntitySelectorType {
  Ship,
  Planet,
  Missile,
}

type EntitySelectorProps = JSX.IntrinsicElements["select"] & {
  filter: EntitySelectorType[];
  setChoice: (entity: Entity | null) => void;
  current: Entity | string  | null;
  exclude?: string;
  extra?: Entity;
  formatter?: (name: string, entity: Entity) => string;
}

export const EntitySelector: React.FC<EntitySelectorProps> = ({
  filter,
  setChoice,
  current,
  exclude,
  extra,
  formatter,
  ...props
}) => {
  const entities = useContext(EntitiesServerContext).entities;

  const currentEntity: Entity | null = useMemo(() => {
    if (current == null) {
      return null;
    }

    if (typeof current === "string") {
      if (filter.includes(EntitySelectorType.Ship)) {
        const ship = entities.ships.find((ship) => ship.name === current) || null;
        if (ship) {
          return ship;
        }
      }

      if (filter.includes(EntitySelectorType.Planet)) {
        const planet = entities.planets.find((planet) => planet.name === current) || null;
        if (planet) {
          return planet;
        }
      }

      if (filter.includes(EntitySelectorType.Missile)) {
        const missile = entities.missiles.find((missile) => missile.name === current) || null;
        if (missile) {
          return missile;
        }
      }
    } else {
      return current;
    }

    return null;
  }, [current, entities]);

  // Create a formatter that handles one not being provided.
  const nf = (name: string, entity: Entity) =>
    formatter ? formatter(name, entity) : name;

  function handleSelectChange(event: React.ChangeEvent<HTMLSelectElement>) {
    const value = event.target.value;

    if (extra && value === extra.name) {
      setChoice(extra);
      return;
    }

    if (filter.includes(EntitySelectorType.Ship)) {
      const shipTarget = entities.ships.find((ship) => ship.name === value);
      if (shipTarget != null) {
        setChoice(shipTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Planet)) {
      const planetTarget = entities.planets.find(
        (planet) => planet.name === value
      );
      if (planetTarget != null) {
        setChoice(planetTarget);
        return;
      }
    }

    if (filter.includes(EntitySelectorType.Missile)) {
      const missileTarget = entities.missiles.find(
        (missile) => missile.name === value
      );
      if (missileTarget != null) {
        setChoice(missileTarget);
        return;
      }
    }

    setChoice(null);
  }

  return (
    <>
      <select
        className={"select-dropdown control-name-input control-input"}
        name="entity_selector"
        value={currentEntity ? currentEntity.name : ""}
        onChange={handleSelectChange}
        {...props}>
        <option key="el-none" value=""></option>
        {extra && (
          <option key={"extra"} value={extra.name}>
            {extra.name}
          </option>
        )}
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

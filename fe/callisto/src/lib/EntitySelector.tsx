import * as React from "react";
import { useMemo } from "react";
import { Entity, Ship } from "./entities";
import { isUndetected } from "./contacts";

import {useAppSelector} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";

export enum EntitySelectorType {
  Ship,
  Planet,
  Missile,
}

type EntitySelectorProps = React.JSX.IntrinsicElements["select"] & {
  filter: EntitySelectorType[];
  setChoice: (entity: Entity | null) => void;
  current: Entity | string  | null;
  exclude?: string;
  extra?: Entity;
  formatter?: (name: string, entity: Entity) => string;
  /**
   * The ship doing the looking. Ships it has no sensor contact on are listed
   * but not selectable: an undetected ship cannot be fired on, locked, jammed
   * or navigated to, and showing the name greyed says why the option is there
   * but unusable.
   *
   * Only ships are gated. Planets do not hide.
   */
  observer?: Ship | null;
  /**
   * Also bar ships on the observer's own side.
   *
   * Set on the firing menu only. A ship will not shoot its own team, but
   * plotting a course to a team-mate -- or sensor locking one -- is perfectly
   * reasonable, so the navigation computer leaves them selectable.
   */
  excludeSameTeam?: boolean;
}

export const EntitySelector: React.FC<EntitySelectorProps> = ({
  filter,
  setChoice,
  current,
  exclude,
  extra,
  formatter,
  observer,
  excludeSameTeam,
  ...props
}) => {
  const entities = useAppSelector(entitiesSelector);

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
  }, [current, entities, filter]);

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
        // Belt and braces: `disabled` should stop this, but contact can be lost
        // between render and click, and acting on an invisible ship is exactly
        // what the server would reject anyway.
        if (isUndetected(observer, shipTarget.name)) {
          return;
        }
        if (
          excludeSameTeam === true &&
          observer?.team != null &&
          shipTarget.team === observer.team
        ) {
          return;
        }
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
            .map((notMeShip) => {
              const undetected = isUndetected(observer, notMeShip.name);
              const sameTeam =
                excludeSameTeam === true &&
                observer?.team != null &&
                notMeShip.team === observer.team;
              const barred = undetected || sameTeam;
              return (
                <option
                  key={"els" + notMeShip.name}
                  value={notMeShip.name}
                  disabled={barred}
                  className={barred ? "no-contact-option" : undefined}>
                  {undetected
                    ? `${notMeShip.name} (no contact)`
                    : sameTeam
                      ? `${notMeShip.name} (same side)`
                      : nf(notMeShip.name, notMeShip)}
                </option>
              );
            })}
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

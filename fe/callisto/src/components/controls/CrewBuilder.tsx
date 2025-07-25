import { useState, useEffect, useMemo } from "react";
import * as React from "react";
import { ShipDesignTemplate } from "lib/shipDesignTemplates";
import { findShip } from "lib/entities";

import { useAppSelector } from "state/hooks";
import {entitiesSelector} from "state/serverSlice";

export interface Crew {
  pilot: number;
  engineering_jump: number;
  engineering_power: number;
  engineering_maneuver: number;
  sensors: number;
  gunnery: number[];
}

export const createCrew = (num_gunners: number) => {
  return {
    pilot: 0,
    engineering_jump: 0,
    engineering_power: 0,
    engineering_maneuver: 0,
    sensors: 0,
    gunnery: new Array(num_gunners).fill(0),
  };
}


interface CrewBuilderProps {
  updateCrew: (crew: Crew) => void;
  shipName: string;
  shipDesign: ShipDesignTemplate;
}

export const CrewBuilder: React.FC<CrewBuilderProps> = ({
  updateCrew,
  shipName,
  shipDesign,
}) => {
  const entities = useAppSelector(entitiesSelector);
  const num_gunners = shipDesign.weapons.length;
  const initialCrew = useMemo(() => {
    return createCrew(num_gunners);
  }, [shipDesign, num_gunners]);

  const [customCrew, setCustomCrew] = useState(initialCrew);
  const [currentGunId, setCurrentGunId] = useState(1);
  const [currentShipName, setCurrentShipName] = useState(shipName);

  useEffect(() => {
    if (shipName !== currentShipName) {
      const new_crew = findShip(entities, shipName)?.crew || initialCrew;
      if (new_crew !== customCrew) {
        setCustomCrew(new_crew);
        updateCrew(new_crew);
      }
      setCurrentShipName(shipName);
    }
  }, [shipName, initialCrew, entities, currentShipName, customCrew, updateCrew]);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    const { name, value } = event.target;

    if (name === "gunnery") {
      // Update the current gunnery value
      const gunneryValues = customCrew.gunnery;

      // Use currentGunId - 1 as we show id's starting at 1, but for the
      // array of course want to start at 0
      gunneryValues[currentGunId - 1] = Number(value);
      const new_crew = { ...customCrew, [name]: gunneryValues } as Crew;
      setCustomCrew(new_crew);
      updateCrew(new_crew);
    } else {
      // For other properties, convert to number
      const new_crew = { ...customCrew, [name]: Number(value) } as Crew;
      setCustomCrew(new_crew);
      updateCrew(new_crew);
    }
  }

  return (
    <div className="crew-builder-window">
      <h3>{shipName}&apos;s Crew</h3>
      <label className="control-label crew-builder-input">
        Pilot
        <input
          className="control-input"
          name="pilot"
          type="text"
          value={customCrew.pilot}
          onChange={handleChange}
        />
      </label>
      <label className="control-label crew-builder-input">
        Eng (Jump)
        <input
          className="control-input"
          name="engineering_jump"
          type="text"
          value={customCrew.engineering_jump}
          onChange={handleChange}
        />
      </label>
      <label className="control-label crew-builder-input">
        Eng (Maneuver)
        <input
          className="control-input"
          name="engineering_maneuver"
          type="text"
          value={customCrew.engineering_maneuver}
          onChange={handleChange}
        />
      </label>
      <label className="control-label crew-builder-input">
        Eng (Power)
        <input
          className="control-input"
          name="engineering_power"
          type="text"
          value={customCrew.engineering_power}
          onChange={handleChange}
        />
      </label>
      <label className="control-label crew-builder-input">
        Sensors
        <input
          className="control-input"
          name="sensors"
          type="text"
          value={customCrew.sensors}
          onChange={handleChange}
        />
      </label>
      {num_gunners > 0 && (
        <label className="control-label crew-builder-input">
          Gunnery
          <span>
            <select
              className="select-dropdown control-name-input control-input crew-builder-gun-selector"
              value={currentGunId}
              onChange={(e) => setCurrentGunId(Number(e.target.value))}>
              {Array.from(Array(num_gunners).keys()).map((gun_id) => (
                <option key={`${gun_id + 1}-gunner`} value={gun_id + 1}>
                  {gun_id + 1}
                </option>
              ))}
            </select>
            <input
              className="control-input"
              name="gunnery"
              type="text"
              value={customCrew.gunnery[currentGunId - 1]}
              onChange={handleChange}
            />
          </span>
        </label>
      )}
    </div>
  );
};

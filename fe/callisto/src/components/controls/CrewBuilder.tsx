import { useState, useEffect, useMemo } from "react";
import * as React from "react";
import { findShip } from "lib/entities";

import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

export interface Crew {
  pilot: number;
  engineering_jump: number;
  engineering_power: number;
  engineering_maneuver: number;
  sensors: number;
  gunnery: number[];
  leadership: number;
}

export const createCrew = (num_gunners: number = 0) => {
  const new_crew = {
    pilot: 0,
    engineering_jump: 0,
    engineering_power: 0,
    engineering_maneuver: 0,
    sensors: 0,
    gunnery: new Array(num_gunners).fill(0),
    leadership: 0,
  };
  return new_crew;
};

interface CrewBuilderProps {
  updateCrew: (crew: Crew) => void;
  currentCrew: Crew;
  shipName: string;
}

export const CrewBuilder: React.FC<CrewBuilderProps> = ({
  updateCrew,
  currentCrew,
  shipName,
}) => {
  const entities = useAppSelector(entitiesSelector);

  const initialCrew = useMemo(() => {
    // Seeded once: the crew panel owns its own edits from here, and gunnery
    // no longer varies with the armament.
    return currentCrew ?? createCrew();
  }, []);

  const [customCrew, setCustomCrew] = useState(initialCrew);
  const [currentShipName, setCurrentShipName] = useState(shipName);

  // Update customCrew when initialCrew changes (e.g., when ship design changes)
  useEffect(() => {
    setCustomCrew(initialCrew);
    updateCrew(initialCrew);
  }, [initialCrew]);

  useEffect(() => {
    if (shipName !== currentShipName) {
      const new_crew = findShip(entities, shipName)?.crew || initialCrew;
      if (new_crew !== customCrew) {
        setCustomCrew(new_crew);
        updateCrew(new_crew);
      }
      setCurrentShipName(shipName);
    }
  }, [
    shipName,
    initialCrew,
    entities,
    currentShipName,
    customCrew,
    updateCrew,
  ]);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    const { name, value } = event.target;

    // Gunnery is not edited here: it is per-weapon, so it lives on the
    // hardpoint rows above and is rebuilt from them on submit.
    const new_crew = { ...customCrew, [name]: Number(value) } as Crew;
    setCustomCrew(new_crew);
    updateCrew(new_crew);
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
      <label className="control-label crew-builder-input">
        Leadership
        <input
          className="control-input"
          name="leadership"
          type="text"
          value={customCrew.leadership ?? 0}
          onChange={handleChange}
        />
      </label>
      <p className="crew-builder-note">
        Gunner skill is set per weapon under Hardpoints.
      </p>
    </div>
  );
};

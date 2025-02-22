import { useState, useEffect, useMemo, useContext } from "react";
import * as React from "react";
import { ShipDesignTemplate } from "./Universal";
import { EntitiesServerContext } from "./Universal";

export class Crew {
  pilot: number;
  engineering_jump: number;
  engineering_power: number;
  engineering_maneuver: number;
  sensors: number;
  gunnery: number[];

  constructor(num_gunners: number) {
    this.pilot = 0;
    this.engineering_jump = 0;
    this.engineering_power = 0;
    this.engineering_maneuver = 0;
    this.sensors = 0;
    this.gunnery = new Array(num_gunners).fill(0);
  }

  // This method is needed to ensure a parsed object from JSON.parse() actually becomes this Class.
  parse(json: {
    pilot: number;
    engineering_jump: number;
    engineering_power: number;
    engineering_maneuver: number;
    sensors: number;
    gunnery: number[];
  }) {
    this.pilot = json.pilot;
    this.engineering_jump = json.engineering_jump;
    this.engineering_power = json.engineering_power;
    this.engineering_maneuver = json.engineering_maneuver;
    this.sensors = json.sensors;
    this.gunnery = json.gunnery;
  }
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
  const num_gunners = shipDesign.weapons.length;
  const initialCrew = useMemo(() => {
    return new Crew(shipDesign.weapons.length);
  }, [shipDesign]);

  const serverEntities = useContext(EntitiesServerContext);

  const [customCrew, customCrewUpdate] = useState(initialCrew);
  const [currentGunId, setCurrentGunId] = useState(1);
  const [currentShipName, setCurrentShipName] = useState(shipName);

  useEffect(() => {
    if (shipName !== currentShipName) {
      const new_crew =
        serverEntities.entities.ships.find((ship) => ship.name === shipName)
          ?.crew || initialCrew;
      if (new_crew !== customCrew) {
        customCrewUpdate(new_crew);
        updateCrew(new_crew);
      }
      setCurrentShipName(shipName);
    }
  }, [shipName, initialCrew, serverEntities.entities.ships, currentShipName]);

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    const { name, value } = event.target;

    if (name === "gunnery") {
      // Update the current gunnery value
      const gunneryValues = customCrew.gunnery;

      // Use currentGunId - 1 as we show id's starting at 1, but for the
      // array of course want to start at 0
      gunneryValues[currentGunId - 1] = Number(value);
      const new_crew = { ...customCrew, [name]: gunneryValues } as Crew;
      customCrewUpdate(new_crew);
      updateCrew(new_crew);
    } else {
      // For other properties, convert to number
      const new_crew = { ...customCrew, [name]: Number(value) } as Crew;
      customCrewUpdate(new_crew);
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

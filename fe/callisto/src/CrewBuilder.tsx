import { useState, useEffect, useMemo } from "react";
import { ShipDesignTemplate } from "./Universal";

export class Crew {
    pilot: number;
    engineering_jump: number;
    engineering_power: number;
    engineering_maneuver: number;
    sensors: number;
    gunnery: number[];

    constructor() {
        this.pilot = 0;
        this.engineering_jump = 0;
        this.engineering_power = 0;
        this.engineering_maneuver = 0;
        this.sensors = 0;
        this.gunnery = [];
    }

    parse(json: any) {
        this.pilot = json.pilot;
        this.engineering_jump = json.engineering_jump;
        this.engineering_power = json.engineering_power;
        this.engineering_maneuver = json.engineering_maneuver;
        this.sensors = json.sensors;
        this.gunnery = json.gunnery;
    }
}

export function CrewBuilder(args: { updateCrew: (crew: Crew) => void, shipName: string, shipDesign: ShipDesignTemplate }) {
  const initialCrew = useMemo(() => new Crew(), []);

  const [customCrew, customCrewUpdate] = useState(initialCrew);
  const [currentGunId, setCurrentGunId] = useState(0);
  const [currentShipName, setCurrentShipName] = useState(args.shipName);
  useEffect (() => {
    if (args.shipName !== currentShipName) {
      customCrewUpdate(initialCrew);
      setCurrentShipName(args.shipName);
    }
  }, [args.shipName, initialCrew, currentShipName]);

  const num_gunners = args.shipDesign.weapons.length;

  function handleChange(event: React.ChangeEvent<HTMLInputElement>) {
    const { name, value } = event.target;
    
    if (name === 'gunnery') {
      // Ensure a big enough array for gunnery
      while (customCrew.gunnery.length < num_gunners) {
        customCrew.gunnery.push(0);
      }
      // Update the current gunnery value
      let gunneryValues = customCrew.gunnery;

      // Use currentGunId - 1 as we show id's starting at 1, but for the 
      // array of course want to start at 0
      gunneryValues[currentGunId - 1] = Number(value);
      customCrewUpdate({ ...customCrew, [name]: gunneryValues } as Crew);
    } else {
      // For other properties, convert to number
      customCrewUpdate({ ...customCrew, [name]: Number(value) } as Crew);
    }

    args.updateCrew(customCrew);
  }

  return (
    <div className="crew-builder-window">
      <h3>{args.shipName}'s Crew</h3>
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
      {(num_gunners > 0) &&
      <label className="control-label crew-builder-input">
          Gunnery
          <span>
          <select className="select-dropdown control-name-input control-input crew-builder-gun-selector"
            value={currentGunId}
            onChange={(e) => setCurrentGunId(Number(e.target.value))}>
            {Array.from(Array(num_gunners).keys()).map((gun_id) => (
              <option key={`${gun_id+1}-gunner`} value={gun_id+1}>
                {gun_id+1}
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
      </label>}
    </div>
  );
}

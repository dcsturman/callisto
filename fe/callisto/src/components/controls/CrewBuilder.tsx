import { useState, useEffect, useMemo } from "react";
import * as React from "react";
import { findShip } from "lib/entities";

import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

/** One engineer on the crew, rated in each of an engineer's skills. */
export interface Engineer {
  jump?: number;
  power?: number;
  maneuver?: number;
  /** Repairs weapons, sensors and the bridge. */
  mechanic?: number;
  /** Electronics (remote ops): flying the repair drones. */
  remote_ops?: number;
  /** Recorded; nothing calls for it yet. */
  life_support?: number;
}

/**
 * The crew aboard.
 *
 * Sensor operators and engineers are lists: a ship can carry several of each,
 * and the second one is another pair of hands with their own skills rather
 * than a bonus on the first one's. The server omits skills that are zero, so
 * every field here reads through a default.
 */
export interface Crew {
  pilot: number;
  sensors: number[];
  engineers: Engineer[];
  gunnery: number[];
  leadership: number;
}

export const createCrew = (num_gunners: number = 0): Crew => ({
  pilot: 0,
  sensors: [],
  engineers: [],
  gunnery: new Array(num_gunners).fill(0),
  leadership: 0,
});

/** An engineer with nothing on their record yet. */
export const createEngineer = (): Engineer => ({
  jump: 0,
  power: 0,
  maneuver: 0,
  mechanic: 0,
  remote_ops: 0,
  life_support: 0,
});

/** The skills an engineer is rated in, in the order the form shows them. */
export const ENGINEER_SKILLS: {key: keyof Engineer; label: string; head: string}[] = [
  {key: "maneuver", label: "M-drive", head: "M-dr"},
  {key: "power", label: "Power", head: "Pwr"},
  {key: "jump", label: "J-drive", head: "J-dr"},
  {key: "mechanic", label: "Mechanic", head: "Mech"},
  {key: "remote_ops", label: "Remote ops", head: "Rem"},
  {key: "life_support", label: "Life support", head: "Life"},
];

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
    const {name, value} = event.target;

    // Gunnery is not edited here: it is per-weapon, so it lives on the
    // hardpoint rows above and is rebuilt from them on submit.
    const new_crew = {...customCrew, [name]: Number(value)} as Crew;
    setCustomCrew(new_crew);
    updateCrew(new_crew);
  }

  /** Replace the whole crew, for the list edits below. */
  function replace(next: Crew) {
    setCustomCrew(next);
    updateCrew(next);
  }

  const operators = customCrew.sensors ?? [];
  const engineers = customCrew.engineers ?? [];

  return (
    <div className="crew-builder-window">
      <h3>{shipName}&apos;s Crew</h3>
      {/* One row per station, label left and value right, all on the same
          grid so the numbers line up down the panel. */}
      <div className="crew-skill-rows">
        <label className="crew-skill-row">
          <span>Pilot</span>
          <input
            className="control-input crew-skill-input"
            name="pilot"
            type="text"
            value={customCrew.pilot}
            onChange={handleChange}
          />
        </label>
        <label className="crew-skill-row">
          <span>Leadership</span>
          <input
            className="control-input crew-skill-input"
            name="leadership"
            type="text"
            value={customCrew.leadership ?? 0}
            onChange={handleChange}
          />
        </label>
      </div>

      {/* Sensor operators and engineers are crews, not skills: a ship can
          carry several of each, so each is a row that can be added to or
          taken off the watch. Both use one grid so the heads sit over the
          boxes they name. */}
      <div className="crew-group">
        <div className="crew-group-head">
          <span className="crew-group-title">Sensor operators</span>
          <button
            type="button"
            className="crew-add"
            title="Another operator on the watch"
            onClick={() => replace({...customCrew, sensors: [...operators, 0]})}>
            +
          </button>
        </div>
        {operators.length === 0 ? (
          <p className="crew-group-empty">Nobody at the sensors.</p>
        ) : (
          <div className="crew-operator-grid">
            <span className="crew-row-label" />
            <span className="crew-column-head">Skill</span>
            <span />
            {operators.map((skill, index) => (
              <React.Fragment key={index}>
                <span className="crew-row-label">#{index + 1}</span>
                <input
                  className="control-input crew-row-input"
                  type="text"
                  value={skill}
                  aria-label={`Sensor operator ${index + 1} skill`}
                  onChange={(event) =>
                    replace({
                      ...customCrew,
                      sensors: operators.map((value, at) =>
                        at === index ? Number(event.target.value) : value
                      ),
                    })
                  }
                />
                <button
                  type="button"
                  className="crew-remove"
                  title={`Take operator #${index + 1} off the watch`}
                  onClick={() =>
                    replace({...customCrew, sensors: operators.filter((_, at) => at !== index)})
                  }>
                  &times;
                </button>
              </React.Fragment>
            ))}
          </div>
        )}
      </div>

      <div className="crew-group">
        <div className="crew-group-head">
          <span className="crew-group-title">Engineers</span>
          <button
            type="button"
            className="crew-add"
            title="Another engineer on the watch"
            onClick={() => replace({...customCrew, engineers: [...engineers, createEngineer()]})}>
            +
          </button>
        </div>
        {engineers.length === 0 ? (
          <p className="crew-group-empty">Nobody in the engine room.</p>
        ) : (
          <div className="crew-engineer-grid">
            <span className="crew-row-label" />
            {ENGINEER_SKILLS.map((skill) => (
              <span key={skill.key} className="crew-column-head" title={skill.label}>
                {skill.head}
              </span>
            ))}
            <span />
            {engineers.map((engineer, index) => (
              <React.Fragment key={index}>
                <span className="crew-row-label">#{index + 1}</span>
                {ENGINEER_SKILLS.map((skill) => (
                  <input
                    key={skill.key}
                    className="control-input crew-row-input"
                    type="text"
                    value={engineer[skill.key] ?? 0}
                    aria-label={`Engineer ${index + 1} ${skill.label}`}
                    onChange={(event) =>
                      replace({
                        ...customCrew,
                        engineers: engineers.map((value, at) =>
                          at === index ? {...value, [skill.key]: Number(event.target.value)} : value
                        ),
                      })
                    }
                  />
                ))}
                <button
                  type="button"
                  className="crew-remove"
                  title={`Take engineer #${index + 1} off the watch`}
                  onClick={() =>
                    replace({...customCrew, engineers: engineers.filter((_, at) => at !== index)})
                  }>
                  &times;
                </button>
              </React.Fragment>
            ))}
          </div>
        )}
      </div>

      <p className="crew-builder-note">
        Gunner skill is set per weapon under Hardpoints.
      </p>
    </div>
  );
};

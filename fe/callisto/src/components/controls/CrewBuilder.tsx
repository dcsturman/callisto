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
  life_support: 0,
});

/** The skills an engineer is rated in, in the order the form shows them. */
export const ENGINEER_SKILLS: {key: keyof Engineer; label: string}[] = [
  {key: "maneuver", label: "M-drive"},
  {key: "power", label: "Power"},
  {key: "jump", label: "J-drive"},
  {key: "mechanic", label: "Mech"},
  {key: "life_support", label: "Life sup"},
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
        Leadership
        <input
          className="control-input"
          name="leadership"
          type="text"
          value={customCrew.leadership ?? 0}
          onChange={handleChange}
        />
      </label>

      {/* Sensor operators and engineers are crews, not skills: a ship can
          carry several of each, so each is a row that can be added to and
          taken away. A ship with one of each looks much as it always did. */}
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
        {operators.length === 0 && <p className="crew-group-empty">Nobody at the sensors.</p>}
        {operators.map((skill, index) => (
          <div className="crew-row" key={index}>
            <span className="crew-row-label">#{index + 1}</span>
            <input
              className="control-input crew-row-input"
              type="text"
              value={skill}
              aria-label={`Sensor operator ${index + 1} skill`}
              onChange={(event) =>
                replace({
                  ...customCrew,
                  sensors: operators.map((value, at) => (at === index ? Number(event.target.value) : value)),
                })
              }
            />
            <button
              type="button"
              className="crew-remove"
              title="Off the watch"
              onClick={() => replace({...customCrew, sensors: operators.filter((_, at) => at !== index)})}>
              &times;
            </button>
          </div>
        ))}
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
        {engineers.length === 0 && <p className="crew-group-empty">Nobody in the engine room.</p>}
        {engineers.length > 0 && (
          <div className="crew-engineer-head">
            <span className="crew-row-label" />
            {ENGINEER_SKILLS.map((skill) => (
              <span key={skill.key} className="crew-engineer-skill-label">
                {skill.label}
              </span>
            ))}
            <span />
          </div>
        )}
        {engineers.map((engineer, index) => (
          <div className="crew-row crew-engineer-row" key={index}>
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
              title="Off the watch"
              onClick={() => replace({...customCrew, engineers: engineers.filter((_, at) => at !== index)})}>
              &times;
            </button>
          </div>
        ))}
      </div>

      <p className="crew-builder-note">
        Gunner skill is set per weapon under Hardpoints.
      </p>
    </div>
  );
};

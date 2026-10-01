import * as React from "react";

import {Ship} from "lib/entities";
import {ENGINEER_SKILLS} from "components/controls/CrewBuilder";
import {setCrewOnDuty} from "lib/serverManager";

/**
 * Which of the crew is working this station, for a ship carrying more than
 * one of them.
 *
 * Hidden entirely when there is one: naming the only operator aboard is noise,
 * and the server falls back to whoever is there. Each is listed by position
 * and skill -- "#2 · 1" -- because the crew have no names, and the number is
 * what the player is choosing between.
 */
export function SensorOperatorPicker(args: {ship: Ship}) {
  const operators = args.ship.crew.sensors ?? [];
  if (operators.length < 2) {
    return null;
  }
  const chosen = Math.min(args.ship.sensor_operator ?? 0, operators.length - 1);
  return (
    <label className="control-label on-duty-picker" title="Which operator is working the sensors this round">
      Operator
      <select
        className="select-dropdown control-input"
        value={chosen}
        onChange={(event) =>
          setCrewOnDuty(args.ship.name, {sensor_operator: Number(event.target.value)})
        }>
        {operators.map((skill, index) => (
          <option key={index} value={index}>
            #{index + 1} · skill {skill}
          </option>
        ))}
      </select>
    </label>
  );
}

/**
 * Which engineer is taking this round's action.
 *
 * An engineer is several skills, so each is listed by the ones they have --
 * "#2 · M2 P3" -- which is what decides who gets sent to a job.
 */
export function EngineerPicker(args: {ship: Ship}) {
  const engineers = args.ship.crew.engineers ?? [];
  if (engineers.length < 2) {
    return null;
  }
  const chosen = Math.min(args.ship.engineer_on_duty ?? 0, engineers.length - 1);
  return (
    <label className="control-label on-duty-picker" title="Which engineer is taking this round's job">
      Engineer
      <select
        className="select-dropdown control-input"
        value={chosen}
        onChange={(event) => setCrewOnDuty(args.ship.name, {engineer: Number(event.target.value)})}>
        {engineers.map((engineer, index) => {
          const rated = ENGINEER_SKILLS.filter((skill) => (engineer[skill.key] ?? 0) > 0)
            .map((skill) => `${skill.label[0]}${engineer[skill.key]}`)
            .join(" ");
          return (
            <option key={index} value={index}>
              #{index + 1}
              {rated === "" ? " · unrated" : ` · ${rated}`}
            </option>
          );
        })}
      </select>
    </label>
  );
}

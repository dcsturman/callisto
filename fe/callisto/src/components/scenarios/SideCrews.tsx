import * as React from "react";
import { ScenarioSide } from "lib/entities";

/** Who crews a side: people, the AI, or some of each, ship by ship. */
export type CrewMode = "Human" | "AI" | "Mixed";

/** Each side's choice, by team, and for a mixed side which ships the AI
 *  crews. */
export type SideCrewChoice = Record<
  string,
  { mode: CrewMode; aiShips: string[] }
>;

/** The choice a scenario file starts each side with: all its ships the
 *  AI's, none, or some. */
export function initialCrews(sides: ScenarioSide[]): SideCrewChoice {
  return Object.fromEntries(
    sides.map((side) => {
      const ai = side.ships.filter((ship) => ship.ai).map((ship) => ship.name);
      const mode: CrewMode =
        ai.length === 0
          ? "Human"
          : ai.length === side.ships.length
            ? "AI"
            : "Mixed";
      return [side.team, { mode, aiShips: ai }];
    }),
  );
}

/** Every ship the choice hands to the AI. */
export function aiShipsOf(
  sides: ScenarioSide[],
  crews: SideCrewChoice,
): string[] {
  return sides.flatMap((side) => {
    const choice = crews[side.team];
    if (!choice || choice.mode === "Human") return [];
    if (choice.mode === "AI") return side.ships.map((ship) => ship.name);
    return choice.aiShips;
  });
}

const MODES: CrewMode[] = ["Human", "AI", "Mixed"];

/** The new-game screen's crew choice: a row per side, and for a mixed side
 *  a checkbox per ship. */
export const SideCrews: React.FC<{
  sides: ScenarioSide[];
  crews: SideCrewChoice;
  setCrews: (crews: SideCrewChoice) => void;
}> = ({ sides, crews, setCrews }) => {
  const update = (team: string, change: Partial<SideCrewChoice[string]>) =>
    setCrews({
      ...crews,
      [team]: { ...(crews[team] ?? { mode: "Human", aiShips: [] }), ...change },
    });

  return (
    <fieldset className="side-crews">
      <legend>Crews</legend>
      {sides.some((side) => crews[side.team]?.mode === "Mixed") && (
        <div className="side-crews-hint">
          On a mixed side, the AI crews the ticked ships.
        </div>
      )}
      {sides.map((side) => {
        const choice = crews[side.team] ?? { mode: "Human", aiShips: [] };
        return (
          <div key={side.team} className="side-crews-side">
            <div className="side-crews-row">
              <span className="side-crews-team">
                {side.team}{" "}
                <span className="side-crews-count">
                  ({side.ships.length}{" "}
                  {side.ships.length === 1 ? "ship" : "ships"})
                </span>
              </span>
              <span
                className="side-crews-modes"
                role="radiogroup"
                aria-label={`Who crews ${side.team}`}
              >
                {MODES.map((mode) => (
                  <label key={mode} className="side-crews-mode">
                    <input
                      type="radio"
                      name={`crew-${side.team}`}
                      value={mode}
                      checked={choice.mode === mode}
                      onChange={() => update(side.team, { mode })}
                    />
                    {mode}
                  </label>
                ))}
              </span>
            </div>
            {choice.mode === "Mixed" && (
              <ul
                className="side-crews-ships"
                aria-label={`${side.team} ships the AI crews`}
              >
                {side.ships.map((ship) => (
                  <li key={ship.name}>
                    <label>
                      <input
                        type="checkbox"
                        checked={choice.aiShips.includes(ship.name)}
                        onChange={(event) =>
                          update(side.team, {
                            aiShips: event.target.checked
                              ? [...choice.aiShips, ship.name]
                              : choice.aiShips.filter(
                                  (name) => name !== ship.name,
                                ),
                          })
                        }
                      />
                      {ship.name}
                    </label>
                  </li>
                ))}
              </ul>
            )}
          </div>
        );
      })}
    </fieldset>
  );
};

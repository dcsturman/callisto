import * as React from "react";
import { useEffect, useRef, useState } from "react";
import { exit_scenario } from "lib/serverManager";
import { SideSeen, outcome, rememberSides, timeUp } from "lib/victory";
import { useAppDispatch, useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { setJoinedScenario } from "state/userSlice";

/**
 * The end of a scenario: once every side but one has no ship left, say who
 * won, and offer to leave or to keep playing. Keep Going puts it away until
 * the outcome changes -- a reset, say, brings the losers back.
 */
export function VictoryScreen() {
  const entities = useAppSelector(entitiesSelector);
  const joinedScenario = useAppSelector((state) => state.user.joinedScenario);
  const dispatch = useAppDispatch();
  // The sides seen in this scenario; a new scenario starts its own roll,
  // before anything is drawn from the last one's.
  const seen = useRef({ scenario: joinedScenario, sides: new Map<string, SideSeen>() });
  const [dismissed, setDismissed] = useState<string | null>(null);
  if (seen.current.scenario !== joinedScenario) {
    seen.current = { scenario: joinedScenario, sides: new Map() };
  }

  seen.current.sides = rememberSides(seen.current.sides, entities.ships);
  const ended = outcome(seen.current.sides, entities.ships) ?? timeUp(entities, seen.current.sides);

  // Back to a live game (a reset, or a ship added): forget the dismissal, so
  // the next ending is announced too.
  useEffect(() => {
    if (ended == null && dismissed != null) {
      setDismissed(null);
    }
  }, [ended, dismissed]);

  if (ended == null || dismissed === ended.headline) {
    return null;
  }

  const exit = () => {
    dispatch(setJoinedScenario(null));
    exit_scenario();
  };

  return (
    <div className="confirm-dialog-backdrop">
      <div
        className="confirm-dialog victory-screen"
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="victory-title"
        aria-describedby="victory-message"
      >
        <h2 id="victory-title" className="victory-title">
          {ended.headline}
        </h2>
        <p id="victory-message" className="confirm-dialog-message">
          {ended.detail ??
            (ended.escaped
            ? "Every ship that set out to get away is gone."
            : ended.winner != null
              ? `${ended.gone.map((side) => side.label).join(" and ")} ${ended.gone.length === 1 ? "has" : "have"} no ships left.`
              : "No side has a ship left.")}
        </p>
        <div className="confirm-dialog-button-row">
          <button type="button" onClick={() => setDismissed(ended.headline)}>
            Keep Going
          </button>
          <button type="button" onClick={exit} autoFocus>
            Exit Scenario
          </button>
        </div>
      </div>
    </div>
  );
}

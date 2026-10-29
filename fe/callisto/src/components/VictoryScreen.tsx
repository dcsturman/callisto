import * as React from "react";
import { useEffect, useRef, useState } from "react";
import { exit_scenario, holdRounds } from "lib/serverManager";
import { leaveLesson } from "lib/tutorial/launch";
import { AppMode } from "state/tutorialSlice";
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
  const inLesson = useAppSelector((state) => state.tutorial.appMode === AppMode.Tutorial);
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
  const ended = outcome(seen.current.sides, entities.ships, entities) ?? timeUp(entities, seen.current.sides);

  // Back to a live game (a reset, or a ship added): forget the dismissal, so
  // the next ending is announced too.
  useEffect(() => {
    if (ended == null && dismissed != null) {
      setDismissed(null);
    }
  }, [ended, dismissed]);

  // While the question is open the game waits for the answer: rounds do
  // not advance on their own (an AI side would otherwise play on behind
  // it). Keep Going lets them go; Exit leaves the game.
  const asking = ended != null && dismissed !== ended.headline;
  const holding = useRef(false);
  useEffect(() => {
    if (!asking) {
      return;
    }
    holdRounds(true);
    holding.current = true;
    return () => {
      if (holding.current) {
        holdRounds(false);
        holding.current = false;
      }
    };
  }, [asking]);

  if (ended == null || dismissed === ended.headline) {
    return null;
  }

  const exit = () => {
    // Let go while still in the game; leaving would refuse it.
    if (holding.current) {
      holdRounds(false);
      holding.current = false;
    }
    // From a lesson, back to the lesson list, as the coach's own button does.
    if (inLesson) {
      leaveLesson();
      return;
    }
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

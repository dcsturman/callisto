import * as React from "react";
import { useState } from "react";
import { Ship } from "lib/entities";
import { useAppDispatch, useAppSelector } from "state/hooks";
// Before the slices: `actionsSlice` imports `serverManager`, which builds the
// store from the slices, so entering that ring through a slice leaves the
// store with no `actions` reducer.
import { captainAction } from "lib/serverManager";
import { templatesSelector } from "state/serverSlice";
import { setReviveWatch } from "state/actionsSlice";
import { crewPercent } from "lib/crewStrength";
import { BoostTarget } from "components/controls/Actions";

interface CaptainTasksProps {
  ship: Ship;
}

// Compact summary of a single boost target for the captain HUD.
function describeBoost(b: BoostTarget): string {
  switch (b.kind) {
    case "Fire":
      return `Fire on ${b.ship} (W#${b.weapon_id})`;
    case "PointDefense":
      return `PD on ${b.ship} (W#${b.weapon_id})`;
    case "Sensor":
      return `Sensor on ${b.ship}`;
    case "Detection":
      return `${b.ship} searching for ${b.target}`;
    case "Engineer":
      return `Engineer on ${b.ship}`;
    case "Evade":
      return `Evade on ${b.ship}`;
    case "AssistGunner":
      return `Assist Gunner on ${b.ship}`;
  }
}

export const CaptainTasks: React.FC<CaptainTasksProps> = ({ ship }) => {
  const boosts = useAppSelector(
    (state) => state.actions[ship.name]?.leadershipCheck?.boosts ?? [],
  );
  const reviving = useAppSelector((state) => state.actions[ship.name]?.reviveWatch ?? null);
  const design = useAppSelector(templatesSelector)[ship.design];
  const dispatch = useAppDispatch();

  // Roll state lives on the ship itself (round-trips via EntityResponse). We
  // derive the display message + button-disabled state from there.
  const rolled = ship.leadership_rolled ?? false;
  const points = ship.leadership_points ?? 0;

  // Status text only appears post-roll; pre-roll the button speaks for itself.
  let postRollText: string | null = null;
  if (rolled && points > 0) {
    postRollText = `Captain can inspire ${points} task${points === 1 ? "" : "s"}.`;
  } else if (rolled) {
    postRollText = `Captain cannot boost tasks this turn (effect ${points}).`;
  }

  const buttonLabel = "Leadership";

  // The frozen watch (High Guard p. 27): spare crew in low berths, revived
  // to replace crew lost to criticals. The revive is the captain's action
  // for the round, in place of the leadership check, and the crew is back
  // next round.
  const complement = design?.crew ?? 0;
  const short = Math.max(0, complement - ship.current_crew);
  const inBerths = ship.frozen_watch ?? 0;
  const canRevive = Math.min(short, inBerths);
  const [reviveCount, setReviveCount] = useState<number>(0);
  const wanted = Math.min(canRevive, reviveCount > 0 ? reviveCount : canRevive);

  return (
    <div className="captain-tasks">
      <button
        type="button"
        className="control-input control-button blue-button"
        data-tutorial="leadership-button"
        disabled={rolled || reviving != null}
        onClick={() => captainAction(ship.name)}
        title={
          reviving != null
            ? "Reviving the frozen watch is the captain's action this round"
            : rolled
              ? "Already rolled this turn"
              : "Roll the leadership check"
        }
        style={{ width: "100%", boxSizing: "border-box" }}
      >
        {buttonLabel}
      </button>
      {(inBerths > 0 || reviving != null) && (
        <div className="captain-revive" data-tutorial="frozen-watch">
          <div>
            Frozen watch: {inBerths} in the berths · crew {ship.current_crew}/{complement} (
            {crewPercent(ship.current_crew, complement)}%)
          </div>
          {reviving != null ? (
            <div className="captain-revive-row">
              <span className="captain-revive-queued">Reviving {reviving} as the round ends</span>
              <button
                type="button"
                className="control-input control-button"
                onClick={() => dispatch(setReviveWatch({ shipName: ship.name, count: 0 }))}
                title="Withdraw the revive and take the leadership check instead"
              >
                Withdraw
              </button>
            </div>
          ) : (
            <div className="captain-revive-row">
              <input
                type="number"
                className="control-input"
                min={0}
                max={canRevive}
                value={wanted}
                disabled={canRevive === 0}
                onChange={(event) => setReviveCount(Math.max(0, Number(event.target.value)))}
                title="How many to revive: no more than are in the berths, and no more than the complement is short"
              />
              <button
                type="button"
                className="control-input control-button blue-button"
                disabled={canRevive === 0 || wanted === 0 || rolled}
                onClick={() => dispatch(setReviveWatch({ shipName: ship.name, count: wanted }))}
                title={
                  rolled
                    ? "The leadership check is already this round's action"
                    : canRevive === 0
                      ? short === 0
                        ? "The crew is at full strength"
                        : "Nobody left in the berths"
                      : "Revive them: the captain's action this round, replacing the leadership check; they are at their posts next round"
                }
              >
                Revive
              </button>
            </div>
          )}
        </div>
      )}
      {postRollText && (
        <p className="plan-accel-text" data-tutorial="leadership-result">
          {postRollText}
        </p>
      )}
      {boosts.length > 0 && (
        <ul className="captain-boost-list">
          {/* Only the first N, in the order given, are inspired: the list is
              the captain's priorities. The rest stay queued in case an
              earlier one's action falls away, and are shown as waiting. */}
          {boosts.map((b, idx) => {
            const waiting = rolled && idx >= Math.max(points, 0);
            return (
              <li
                key={idx}
                className={waiting ? "captain-boost-waiting" : undefined}
                title={waiting ? "Beyond what the roll inspires this round" : undefined}
              >
                {describeBoost(b)}
                {waiting && " (not inspired)"}
              </li>
            );
          })}
        </ul>
      )}
    </div>
  );
};

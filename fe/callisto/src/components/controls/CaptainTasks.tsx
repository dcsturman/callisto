import * as React from "react";
import { Ship } from "lib/entities";
import { useAppSelector } from "state/hooks";
import { captainAction } from "lib/serverManager";
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

  // Roll state lives on the ship itself (round-trips via EntityResponse). We
  // derive the display message + button-disabled state from there.
  const rolled = ship.leadership_rolled ?? false;
  const points = ship.leadership_points ?? 0;

  // Status text only appears post-roll; pre-roll the button speaks for itself.
  let postRollText: string | null = null;
  if (rolled && points > 0) {
    postRollText = `Captain can inspire ${points} task${points === 1 ? "" : "s"}.`;
  } else if (rolled) {
    postRollText = `Captain cannot boost tasks this turn (rolled ${points}).`;
  }

  const buttonLabel = "Leadership";

  return (
    <div className="captain-tasks">
      <div className="section-tag">Captain</div>
      <button
        type="button"
        className="control-input control-button blue-button"
        disabled={rolled}
        onClick={() => captainAction(ship.name)}
        title={rolled ? "Already rolled this turn" : "Roll the leadership check"}
        style={{ width: "100%", boxSizing: "border-box" }}
      >
        {buttonLabel}
      </button>
      {postRollText && <p className="plan-accel-text">{postRollText}</p>}
      {boosts.length > 0 && (
        <ul className="captain-boost-list">
          {boosts.map((b, idx) => (
            <li key={idx}>{describeBoost(b)}</li>
          ))}
        </ul>
      )}
    </div>
  );
};

import { ShipDesignTemplate, WeaponMount } from "./Universal";
import { FireState } from "./Controls";

// Icons for each type of weapon
import { ReactComponent as Turret1 } from "./icons/turret1.svg";
import { ReactComponent as Turret2 } from "./icons/turret2.svg";
import { ReactComponent as Turret3 } from "./icons/turret3.svg";
import { ReactComponent as Barbette } from "./icons/barbette.svg";
import { ReactComponent as SmallBay } from "./icons/bay-s.svg";
import { ReactComponent as MediumBay } from "./icons/bay-m.svg";
import { ReactComponent as LargeBay } from "./icons/bay-l.svg";

// Icons to show fire states.
import { ReactComponent as RayIcon } from "./icons/laser.svg";
import { ReactComponent as MissileIcon } from "./icons/missile.svg";

// Consistent set of colors for both type of weapons and fire states.
const WEAPON_COLORS: { [key: string]: string } = {
  Beam: "red",
  Pulse: "blue",
  Missile: "green",
  Particle: "yellow",
};

export const WeaponButton = (props: { weapon: string; mount: WeaponMount; count: number; onClick: () => void }) => {
  if (typeof props.mount === "string") {
    return (
      <div className="weapon-button-container">
      <Barbette
        className="weapon-button barbette-button"
        style={{
          fill: WEAPON_COLORS[props.weapon],
        }}
        onClick={props.onClick}
      />
      <span className="weapon-button-count">{props.count}</span>
      </div>
    );
  }
  if ("Bay" in props.mount) {
    let size = props.mount.Bay;
    if (size === "Small") {
      return (
        <div className="weapon-button-container"><SmallBay
          className="weapon-button bay-button"
          style={{
            fill: WEAPON_COLORS[props.weapon],
          }}
          onClick={props.onClick}
        />
        <span className="weapon-button-count">{props.count}</span>
        </div>
      );
    } else if (size === "Medium") {
      return (
        <div className="weapon-button-container"><MediumBay
          className="weapon-button bay-button"
          style={{
            fill: WEAPON_COLORS[props.weapon],
          }}
          onClick={props.onClick}
        />
        <span className="weapon-button-count">{props.count}</span>
        </div>
      );
    } else {
      return (
        <div className="weapon-button-container"><LargeBay
          className="weapon-button bay-button"
          style={{
            fill: WEAPON_COLORS[props.weapon],
          }}
          onClick={props.onClick}
        />
        <span className="weapon-button-count">{props.count}</span>
        </div>
      );
    }
  } else if ("Turret" in props.mount) {
    let num = props.mount.Turret;
    if (num === 1) {
      return (
        <div className="weapon-button-container"><Turret1
          className="weapon-button turret-button"
          style={{
            fill: WEAPON_COLORS[props.weapon],
          }}
          onClick={props.onClick}
        />
        <span className="weapon-button-count">{props.count}</span>
        </div>
      );
    }
    if (num === 2) {
      return (
        <div className="weapon-button-container"><Turret2
          className="weapon-button turret-button"
          style={{
            fill: WEAPON_COLORS[props.weapon],
          }}
          onClick={props.onClick}
        />
        <span className="weapon-button-count">{props.count}</span>
        </div>
      );
    }
    return (
      <div className="weapon-button-container"><Turret3
        className="weapon-button turret-button"
        style={{
          fill: WEAPON_COLORS[props.weapon],
        }}
        onClick={props.onClick}
      />
      <span className="weapon-button-count">{props.count}</span>
      </div>
    );
  }
  return <></>;
};


export function FireActions(args: { actions: FireState, design: ShipDesignTemplate }) {
    return (
      <div className="control-form">
        <h2>Fire Actions</h2>
        {args.actions.map((action, index) =>
          ["Beam", "Pulse", "Particle"].includes(args.design.weapons[action.weapon_id].kind) ?
            <p key={index + "_fire_img"}>
              <RayIcon className="beam-type-icon" style={{fill: WEAPON_COLORS[args.design.weapons[action.weapon_id].kind]}}/> to {action.target}
            </p>
          : <p key={index + "_fire_img"}>
              <MissileIcon className="missile-type-icon" style={{fill: WEAPON_COLORS[args.design.weapons[action.weapon_id].kind]}}/> to {action.target}
            </p>
          
        )}
      </div>
    );
  }
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
import { Tooltip } from "react-tooltip";

// Consistent set of colors for both type of weapons and fire states.
const WEAPON_COLORS: { [key: string]: string } = {
  Beam: "red",
  Pulse: "blue",
  Missile: "green",
  Particle: "yellow",
};

export const WeaponButton = (props: {
  weapon: string;
  mount: WeaponMount;
  count: number;
  onClick: () => void;
}) => {
  if (typeof props.mount === "string") {
    return (
      <>
        <div
          className="weapon-button"
          data-tooltip-id={props.weapon + props.mount}
          data-tooltip-content={`${props.weapon} Barbette`}
          data-tooltip-delay-show={700}>
          <Barbette
            className="weapon-symbol barbette-button"
            style={{
              fill: WEAPON_COLORS[props.weapon],
            }}
            onClick={props.onClick}
          />
          <span className="weapon-symbol-count">{props.count}</span>
        </div>
        <Tooltip
          id={props.weapon + props.mount}
          className="tooltip-body weapon-button-tooltip"
        />
      </>
    );
  }
  if ("Bay" in props.mount) {
    let size = props.mount.Bay;
    if (size === "Small") {
      return (
        <>
          <div
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "small-bay"}
            data-tooltip-content={`Small ${props.weapon} Bay`}
            data-tooltip-delay-show={700}>
            <SmallBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </div>
          <Tooltip
            id={props.weapon + "small-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    } else if (size === "Medium") {
      return (
        <>
          <div
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "med-bay"}
            data-tooltip-content={`Medium ${props.weapon} Bay`}
            data-tooltip-delay-show={700}>
            <MediumBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </div>
          <Tooltip
            id={props.weapon + "med-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    } else {
      return (
        <>
          <div
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "large-bay"}
            data-tooltip-content={`Large ${props.weapon} Bay`}
            data-tooltip-delay-show={700}>
            <LargeBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </div>
          <Tooltip
            id={props.weapon + "large-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    }
  } else if ("Turret" in props.mount) {
    let num = props.mount.Turret;
    if (num === 1) {
      return (
        <>
          <div
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={`Single ${props.weapon} Turret`}
            data-tooltip-delay-show={700}>
            <Turret1
              className="weapon-symbol turret-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </div>
          <Tooltip
            id={props.weapon + num + "turret"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    }
    if (num === 2) {
      return (
        <>
          <div
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={`Double ${props.weapon} Turret`}
            data-tooltip-delay-show={700}>
            <Turret2
              className="weapon-symbol turret-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </div>
          <Tooltip
            id={props.weapon + num + "turret"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    }
    return (
      <>
        <div
          className="weapon-button"
          onClick={props.onClick}
          data-tooltip-id={props.weapon + num + "turret"}
          data-tooltip-content={`Triple ${props.weapon} Turret`}
          data-tooltip-delay-show={700}>
          <Turret3
            className="weapon-symbol turret-button"
            style={{
              fill: WEAPON_COLORS[props.weapon],
            }}
          />
          <span className="weapon-symbol-count">{props.count}</span>
        </div>
        <Tooltip
          id={props.weapon + num + "turret"}
          className="tooltip-body weapon-button-tooltip"
        />
      </>
    );
  }
  return <></>;
};

export function FireActions(args: {
  actions: FireState;
  design: ShipDesignTemplate;
}) {
  return (
    <div className="control-form">
      <h2>Fire Actions</h2>
      {args.actions.map((action, index) =>
        ["Beam", "Pulse", "Particle"].includes(
          args.design.weapons[action.weapon_id].kind
        ) ? (
          <p key={index + "_fire_img"}>
            <RayIcon
              className="beam-type-icon"
              style={{
                fill: WEAPON_COLORS[args.design.weapons[action.weapon_id].kind],
              }}
            />{" "}
            to {action.target}
          </p>
        ) : (
          <p key={index + "_fire_img"}>
            <MissileIcon
              className="missile-type-icon"
              style={{
                fill: WEAPON_COLORS[args.design.weapons[action.weapon_id].kind],
              }}
            />{" "}
            to {action.target}
          </p>
        )
      )}
    </div>
  );
}

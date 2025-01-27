import React, { useContext, useState } from "react";
import { findRangeBand } from "./Util";
import {
  ShipDesignTemplate,
  WeaponMount,
  EntitiesServerContext,
  Ship,
  SHIP_SYSTEMS
} from "./Universal";
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
import { vectorDistance } from "./Util";

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
  disable: boolean;
}) => {
  if (typeof props.mount === "string") {
    return (
      <>
        <button
          className="weapon-button"
          data-tooltip-id={props.weapon + props.mount}
          data-tooltip-content={`${props.weapon} Barbette`}
          data-tooltip-delay-show={700}
          onClick={props.onClick}
          disabled={props.disable}
          >
          <Barbette
            className="weapon-symbol barbette-button"
            style={{
              fill: WEAPON_COLORS[props.weapon],
            }}
          />
          <span className="weapon-symbol-count">{props.count}</span>
        </button>
        <Tooltip
          id={props.weapon + props.mount}
          className="tooltip-body weapon-button-tooltip"
        />
      </>
    );
  }
  if ("Bay" in props.mount) {
    const size = props.mount.Bay;
    if (size === "Small") {
      return (
        <>
          <button
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "small-bay"}
            data-tooltip-content={`Small ${props.weapon} Bay`}
            data-tooltip-delay-show={700}
            disabled={props.disable}>
            <SmallBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
          <Tooltip
            id={props.weapon + "small-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    } else if (size === "Medium") {
      return (
        <>
          <button
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "med-bay"}
            data-tooltip-content={`Medium ${props.weapon} Bay`}
            data-tooltip-delay-show={700}
            disabled={props.disable}>
            <MediumBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
          <Tooltip
            id={props.weapon + "med-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    } else {
      return (
        <>
          <button
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "large-bay"}
            data-tooltip-content={`Large ${props.weapon} Bay`}
            data-tooltip-delay-show={700}
            disabled={props.disable}>
            <LargeBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
          <Tooltip
            id={props.weapon + "large-bay"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    }
  } else if ("Turret" in props.mount) {
    const num = props.mount.Turret;
    if (num === 1) {
      return (
        <>
          <button
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={`Single ${props.weapon} Turret`}
            data-tooltip-delay-show={700}
            disabled={props.disable}>
            <Turret1
              className="weapon-symbol turret-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
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
          <button
            className="weapon-button"
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={`Double ${props.weapon} Turret`}
            data-tooltip-delay-show={700}
            disabled={props.disable}>
            <Turret2
              className="weapon-symbol turret-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
          <Tooltip
            id={props.weapon + num + "turret"}
            className="tooltip-body  weapon-button-tooltip"
          />
        </>
      );
    }
    return (
      <>
        <button
          className="weapon-button"
          onClick={props.onClick}
          data-tooltip-id={props.weapon + num + "turret"}
          data-tooltip-content={`Triple ${props.weapon} Turret`}
          data-tooltip-delay-show={700}
          disabled={props.disable}>
          <Turret3
            className="weapon-symbol turret-button"
            style={{
              fill: WEAPON_COLORS[props.weapon],
            }}
          />
          <span className="weapon-symbol-count">{props.count}</span>
        </button>
        <Tooltip
          id={props.weapon + num + "turret"}
          className="tooltip-body weapon-button-tooltip"
        />
      </>
    );
  }
  return <></>;
};

function CalledShotMenu(args: {
  attacker: Ship,
  target: Ship,
  calledShot: string | null;
  setCalledShot: (system: string | null) => void;
}) {
  const range = findRangeBand(vectorDistance(args.attacker.position, args.target.position));

  const [system, setSystem] = useState<string | null>(args.calledShot);

  if (range !== "Short") {
    return <></>;
  }

  return (
      <select 
        className="called-shot-menu"
        name="called_shot_system"
        value={system ? system : "No called shot"}
        onChange={(e) => { if (e.target.value === "No called shot") {
          args.setCalledShot(null);
          setSystem(null);
        } else {
          args.setCalledShot(e.target.value);
          setSystem(e.target.value);
        }}}>
        <option key="none" value="No called shot">No called shot</option>
        {SHIP_SYSTEMS.map((system) => (
          <option key={system} value={system}>
            {system}
          </option>
        ))}
      </select>
  );
}

export function FireActions(args: {
  actions: FireState;
  design: ShipDesignTemplate;
  computerShipName: string;
}) {
  const serverEntities = useContext(EntitiesServerContext);
  const computerShip = serverEntities.entities.ships.find(
    (ship) => ship.name === args.computerShipName
  );

  console.log("**** FireActions JSON: " + JSON.stringify(args.actions));
  return (
    <div className="control-form">
      <h2>Fire Actions</h2>
      {args.actions.map((action, index) =>
        ["Beam", "Pulse", "Particle"].includes(
          args.design.weapons[action.weapon_id].kind
        ) ? (
          <div className="fire-actions-div" key={index + "_fire_img"}>
            <p>
            <RayIcon
              className="beam-type-icon"
              style={{
                fill: WEAPON_COLORS[args.design.weapons[action.weapon_id].kind],
              }}
            />{" "}
            to {action.target}
            </p>
            <CalledShotMenu attacker={computerShip!} target={serverEntities.entities.ships.find((ship) => ship.name === action.target)!} calledShot={action.called_shot_system} setCalledShot={(system) => action.called_shot_system = system} />
            </div>
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

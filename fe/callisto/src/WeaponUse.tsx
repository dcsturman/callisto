import React, { useContext, useState, useMemo, useEffect } from "react";
import { findRangeBand } from "./Util";
import {
  Entity,
  ShipDesignTemplate,
  WeaponMount,
  EntitiesServerContext,
  DesignTemplatesContext,
  Ship,
  SHIP_SYSTEMS,
} from "./Universal";
import { EntitySelector, EntitySelectorType } from "./EntitySelector";
import { FireState, ActionContext } from "./Actions";

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
  disabled: boolean;
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
          disabled={props.disabled}>
          <Barbette
            className="weapon-symbol barbette-button"
            style={{
              fill: WEAPON_COLORS[props.weapon],
            }}
          />
          <span className="weapon-symbol-count">{props.count}</span>
        </button>
        <Tooltip id={props.weapon + props.mount} className="tooltip-body weapon-button-tooltip" />
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
            disabled={props.disabled}>
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
            disabled={props.disabled}>
            <MediumBay
              className="weapon-symbol bay-button"
              style={{
                fill: WEAPON_COLORS[props.weapon],
              }}
            />
            <span className="weapon-symbol-count">{props.count}</span>
          </button>
          <Tooltip id={props.weapon + "med-bay"} className="tooltip-body  weapon-button-tooltip" />
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
            disabled={props.disabled}>
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
            disabled={props.disabled}>
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
            disabled={props.disabled}>
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
          disabled={props.disabled}>
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
  attacker: Ship;
  target: Ship;
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
      onChange={(e) => {
        if (e.target.value === "No called shot") {
          args.setCalledShot(null);
          setSystem(null);
        } else {
          args.setCalledShot(e.target.value);
          setSystem(e.target.value);
        }
      }}>
      <option key="none" value="No called shot">
        No called shot
      </option>
      {SHIP_SYSTEMS.map((system) => (
        <option key={system} value={system}>
          {system}
        </option>
      ))}
    </select>
  );
}

type FireControlProps = {
  computerShip: Ship;
};

export const FireControl: React.FC<FireControlProps> = ({ computerShip }) => {
  const actionContext = useContext(ActionContext);
  const designs = useContext(DesignTemplatesContext);

  const weaponDetails = useMemo(() => {
    return designs.templates[computerShip.design].compressedWeapons();
  }, [computerShip.design, designs.templates]);

  const availableCounts = useMemo(() => {
    const counts = {} as { [key: string]: number };
    const design = designs.templates[computerShip.design];
    // Count up all the actions by weapon
    if (actionContext.actions[computerShip.name]?.fire) {
      for (const action of actionContext.actions[computerShip.name].fire) {
        counts[design.getWeaponName(action.weapon_id)] = (counts[design.getWeaponName(action.weapon_id)] || 0) + 1;
      }
    }

    const available = {} as { [key: string]: number };
    // Subtract all the counts (if the exist) from the total counts
    for (const weapon in weaponDetails) {
      available[weapon] = weaponDetails[weapon].total - (counts[weapon] || 0);
    }
    return available;
  }, [actionContext, computerShip.name, computerShip.design, weaponDetails, designs]);

  const [fireTarget, setFireTarget] = useState<Entity | null>(null);

  useEffect(() => {
    if (computerShip.name === fireTarget?.name) {
      setFireTarget(null);
    }
  }, [computerShip, fireTarget]);

  function handleFireCommand(attacker: string, target: string, weapon_name: string) {
    const computerShipDesign = designs.templates[computerShip.design];
    if (!computerShipDesign) {
      console.error("(Controls.handleFireCommand) No computer ship design for " + attacker + ".");
      return;
    }

    const weapon_id = computerShipDesign.findNthWeapon(weapon_name, weaponDetails[weapon_name].total - availableCounts[weapon_name] + 1);
    if (availableCounts[weapon_name] === 0) {
      console.log(
        "(Controls.handleFireCommand) No more weapons of type " + weapon_id + " for " + attacker + "."
      );
      return;
    }
    actionContext.fireWeapon(attacker, weapon_id, target);
  }

  return (
    <>
      <div className="control-launch-div">
        Target:
        <EntitySelector
          filter={[EntitySelectorType.Ship]}
          setChoice={setFireTarget}
          current={fireTarget}
          exclude={computerShip.name}
          formatter={(name, entity) => {
            if (computerShip) {
              return `${name} (${findRangeBand(
                vectorDistance(computerShip.position, entity.position)
              )})`;
            } else {
              return "";
            }
          }}
        />
      </div>
      <div className="weapon-list">
        {Object.entries(designs.templates[computerShip.design].compressedWeapons()).map(
            ([weapon_name, weapon]) =>
              !weapon_name.includes("Sand") && (
                <WeaponButton
                  key={"weapon-" + computerShip.name + "-" + weapon_name}
                  weapon={weapon.kind}
                  mount={weapon.mount}
                  count={availableCounts[weapon_name]}
                  onClick={() => {
                    handleFireCommand(
                      computerShip.name,
                      fireTarget ? fireTarget.name : "",
                      weapon_name
                    );
                  }}
                  disabled={!fireTarget}
                />
              )
          )}
      </div>
    </>
  );
};
export function FireActions(args: {
  actions: FireState;
  design: ShipDesignTemplate;
  computerShipName: string;
}) {
  const serverEntities = useContext(EntitiesServerContext);
  const actionsContext = useContext(ActionContext);

  const computerShip = serverEntities.entities.ships.find(
    (ship) => ship.name === args.computerShipName
  );

  const onClick = (weapon_id: number) => {
    actionsContext.unfireWeapon(args.computerShipName, weapon_id);
  }

  return (
    <div className="control-form">
      <h2>Fire Actions</h2>
      {args.actions.map((action, index) => {
        let kind = null;
        if (args.design.weapons[action.weapon_id].kind === "Beam") {
          kind = "Beam";
        } else if (args.design.weapons[action.weapon_id].kind === "Pulse") {
          kind = "Pulse";
        } else if (args.design.weapons[action.weapon_id].kind === "Particle") {
          kind = "Particle";
        } else {
          kind = "Missile";
        };

        return ["Beam", "Pulse", "Particle"].includes(kind) ?  (
          <div className="fire-actions-div" key={index + "_fire_img"} onClick={() => onClick(action.weapon_id)}>
            <p>
              <RayIcon
                className="beam-type-icon"
                style={{
                  fill: WEAPON_COLORS[kind],
                }}
              />{" "}
              to {action.target}
            </p>
            <CalledShotMenu
              attacker={computerShip!}
              target={serverEntities.entities.ships.find((ship) => ship.name === action.target)!}
              calledShot={action.called_shot_system}
              setCalledShot={(system) => (action.called_shot_system = system)}
            />
          </div>
        ) : (
          <div key={index + "_fire_img"} onClick={() => onClick(action.weapon_id)}>
            <p>
              <MissileIcon
                className="missile-type-icon"
                style={{
                  fill: WEAPON_COLORS[kind],
                }}
              />{" "}
              to {action.target}
            </p>
          </div>
        )
      })}
    </div>
  );
}

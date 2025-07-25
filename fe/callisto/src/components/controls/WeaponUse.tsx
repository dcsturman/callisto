import * as React from "react";
import {useMemo, useState, useEffect, useCallback} from "react";
import {findRangeBand} from "lib/Util";
import {SHIP_SYSTEMS} from "lib/universal";
import {Ship, Entity, findShip} from "lib/entities";
import {
  ShipDesignTemplate,
  compressedWeaponsFromTemplate,
  getWeaponName,
  findNthWeapon,
} from "lib/shipDesignTemplates";
import {WeaponMount} from "lib/weapon";
import {EntitySelector, EntitySelectorType} from "lib/EntitySelector";
import {FireState, PointDefenseState} from "components/controls/Actions";

// Icons for each type of weapon
import {ReactComponent as Turret1} from "assets/icons/turret1.svg";
import {ReactComponent as Turret2} from "assets/icons/turret2.svg";
import {ReactComponent as Turret3} from "assets/icons/turret3.svg";
import {ReactComponent as Barbette} from "assets/icons/barbette.svg";
import {ReactComponent as SmallBay} from "assets/icons/bay-s.svg";
import {ReactComponent as MediumBay} from "assets/icons/bay-m.svg";
import {ReactComponent as LargeBay} from "assets/icons/bay-l.svg";

// Icons to show fire states.
import {ReactComponent as RayIcon} from "assets/icons/laser.svg";
import {ReactComponent as MissileIcon} from "assets/icons/missile.svg";
import {Tooltip} from "react-tooltip";
import {vectorDistance} from "lib/Util";

// State operators
import {useAppSelector, useAppDispatch} from "state/hooks";
import {
  pointDefenseWeapon,
  fireWeapon,
  unfireWeapon,
  updateFireCalledShot,
} from "state/actionsSlice";
import {entitiesSelector} from "state/serverSlice";

// Consistent set of colors for both type of weapons and fire states.
const WEAPON_COLORS: {[key: string]: string} = {
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
          id={props.weapon + "-barbette-button"}
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
            id={props.weapon + "-small-bay-button"}
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
            id={props.weapon + "-medium-bay-button"}
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
            id={props.weapon + "-large-bay-button"}
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
            id={props.weapon + "-single-turret-button"}
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
            id={props.weapon + "-double-turret-button"}
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
          id={props.weapon + "-triple-turret-button"}
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
  const [system, setSystem] = useState<string | null>(args.calledShot);

  if (!args.attacker || !args.target) {
    return <></>;
  }
  const range = findRangeBand(vectorDistance(args.attacker.position, args.target.position));

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

type FireControlProps = unknown;

export const FireControl: React.FC<FireControlProps> = () => {
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const entities = useAppSelector(entitiesSelector);
  const shipTemplates = useAppSelector((state) => state.server.templates);
  const actions = useAppSelector((state) => state.actions);
  const computerShip = useMemo(
    () => findShip(entities, computerShipName),
    [computerShipName, entities]
  );
  const computerShipDesign = useMemo(
    () => (computerShip ? shipTemplates[computerShip.design] : null),
    [shipTemplates, computerShip]
  );
  const dispatch = useAppDispatch();

  const weaponDetails = useMemo(() => {
    return compressedWeaponsFromTemplate(computerShipDesign);
  }, [computerShipDesign]);

  const availableCounts = useMemo(() => {
    const counts = {} as {[key: string]: number};
    // Count up all the actions by weapon
    if (computerShipDesign && actions[computerShipName!]?.fire) {
      for (const action of actions[computerShipName!].fire) {
        counts[getWeaponName(computerShipDesign, action.weapon_id)] =
          (counts[getWeaponName(computerShipDesign, action.weapon_id)] || 0) + 1;
      }
    }

    // Count up all the actions in point defense
    if (computerShipDesign && actions[computerShipName!]?.pointDefense) {
      for (const action of actions[computerShipName!].pointDefense) {
        counts[getWeaponName(computerShipDesign, action.weapon_id)] =
          (counts[getWeaponName(computerShipDesign, action.weapon_id)] || 0) + 1;
      }
    }

    const available = {} as {[key: string]: number};
    // Subtract all the counts (if the exist) from the total counts
    for (const weapon in weaponDetails) {
      available[weapon] = weaponDetails[weapon].total - (counts[weapon] || 0);
    }
    return available;
  }, [computerShipName, computerShipDesign, weaponDetails, actions]);

  const [fireTarget, setFireTarget] = useState<Entity | null>(null);

  useEffect(() => {
    if (computerShipName === fireTarget?.name) {
      setFireTarget(null);
    }
  }, [computerShipName, fireTarget]);

  const POINT_DEFENSE_NAME = useMemo(() => "<Point Defense>", []);
  const POINT_DEFENSE_PHANTOM = useMemo(() => ({
    name: POINT_DEFENSE_NAME,
    position: [0, 0, 0],
    velocity: [0, 0, 0],
  } as Entity), [POINT_DEFENSE_NAME]);

  const handleFireCommand = useCallback(
    (attacker: string, target: string, weapon_name: string) => {
      if (!computerShipDesign) {
        console.error("(Controls.handleFireCommand) No computer ship design for " + attacker + ".");
        return;
      }

      const weapon_id = findNthWeapon(
        computerShipDesign,
        weapon_name,
        weaponDetails[weapon_name].total - availableCounts[weapon_name] + 1
      );
      if (availableCounts[weapon_name] === 0) {
        console.log(
          "(Controls.handleFireCommand) No more weapons of type " +
            weapon_id +
            " for " +
            attacker +
            "."
        );
        return;
      }

      if (target === POINT_DEFENSE_NAME) {
        dispatch(pointDefenseWeapon({shipName: attacker, weapon_id: weapon_id}));
      } else {
        dispatch(
          fireWeapon({shipName: attacker, weapon_id: weapon_id, target: target, entities: entities})
        );
      }
    },
    [computerShipDesign, weaponDetails, availableCounts, dispatch, entities, POINT_DEFENSE_NAME]
  );

  const formatter = useCallback(
    (name: string, entity: Entity) => {
      if (computerShip) {
        return `${name} (${findRangeBand(vectorDistance(computerShip.position, entity.position))})`;
      } else {
        return "";
      }
    },
    [computerShip]
  );

  const filter = useMemo(() => [EntitySelectorType.Ship], []);

  const handleWeaponClick = useCallback(
    (weapon_name: string) => {
      if (!computerShipName) {
        return;
      }
      handleFireCommand(computerShipName, fireTarget ? fireTarget.name : "", weapon_name);
    },
    [handleFireCommand, computerShipName, fireTarget]
  );

  const isWeaponDisabled = useCallback(
    (weapon: {kind: string; mount: WeaponMount}) => {
      return (
        !fireTarget ||
        ((fireTarget.name === POINT_DEFENSE_NAME) &&
          !(
            (weapon.kind.includes("Beam") || weapon.kind.includes("Pulse")) &&
            weapon.mount !== "Turret"
          ))
      );
    },
    [fireTarget, POINT_DEFENSE_NAME]
  );

  const weaponButtons = useMemo(
    () =>
      computerShipName && Object.entries(compressedWeaponsFromTemplate(computerShipDesign)).map(
        ([weapon_name, weapon]) =>
          !weapon_name.includes("Sand") && (
            <WeaponButton
              key={"weapon-" + computerShipName + "-" + weapon_name}
              weapon={weapon.kind}
              mount={weapon.mount}
              count={availableCounts[weapon_name]}
              onClick={() => handleWeaponClick(weapon_name)}
              disabled={isWeaponDisabled(weapon)}
            />
          )
      ),
    [computerShipName, computerShipDesign, availableCounts, handleWeaponClick, isWeaponDisabled]
  );

  return (
    <>
      <div className="control-launch-div">
        target:
        <EntitySelector
          id={"fire-target"}
          filter={filter}
          setChoice={setFireTarget}
          current={fireTarget}
          exclude={computerShipName!}
          extra={POINT_DEFENSE_PHANTOM}
          formatter={formatter}
        />
      </div>
      <div className="weapon-list">
        {weaponButtons}
      </div>
    </>
  );
};

export function FireActions(args: {
  fireActions: FireState;
  pointDefenseActions: PointDefenseState;
  design: ShipDesignTemplate;
}) {
  const entities = useAppSelector(entitiesSelector);
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const dispatch = useAppDispatch();

  const computerShip = useMemo(
    () => findShip(entities, computerShipName),
    [computerShipName, entities]
  );

  const onClick = (weapon_id: number) => {
    dispatch(unfireWeapon({shipName: computerShipName!, weapon_id: weapon_id}));
  };

  return (
    <div className="control-form">
      <h2>Fire Actions</h2>
      {args.pointDefenseActions.map((action, index) => {
        let kind = null;
        if (args.design.weapons[action.weapon_id].kind === "Beam") {
          kind = "Beam";
        } else if (args.design.weapons[action.weapon_id].kind === "Pulse") {
          kind = "Pulse";
        } else {
          console.error(
            "(FireActions) Illegal weapon kind for point defense: " +
              args.design.weapons[action.weapon_id].kind
          );
          return (
            <div className="fire-actions-div" key={index + "bug"}>
              This is a bug
            </div>
          );
        }

        return ["Beam", "Pulse"].includes(kind) ? (
          <div className="fire-actions-div" key={index + "_fire_img"}>
            <div onClick={() => onClick(action.weapon_id)}>
              <p>
                <RayIcon
                  className="beam-type-icon"
                  style={{
                    fill: WEAPON_COLORS[kind],
                  }}
                />{" "}
                on Point Defense
              </p>
            </div>
          </div>
        ) : (
          <></>
        );
      })}

      {args.fireActions.map((action, index) => {
        let kind = null;
        if (args.design.weapons[action.weapon_id].kind === "Beam") {
          kind = "Beam";
        } else if (args.design.weapons[action.weapon_id].kind === "Pulse") {
          kind = "Pulse";
        } else if (args.design.weapons[action.weapon_id].kind === "Particle") {
          kind = "Particle";
        } else {
          kind = "Missile";
        }

        return ["Beam", "Pulse", "Particle"].includes(kind) ? (
          <div className="fire-actions-div" key={index + "_fire_img"}>
            <div onClick={() => onClick(action.weapon_id)}>
              <p>
                <RayIcon
                  className="beam-type-icon"
                  style={{
                    fill: WEAPON_COLORS[kind],
                  }}
                />{" "}
                to {action.target}
              </p>
            </div>
            <CalledShotMenu
              attacker={computerShip!}
              target={findShip(entities, action.target)!}
              calledShot={action.called_shot_system}
              setCalledShot={(system) =>
                dispatch(
                  updateFireCalledShot({shipName: computerShipName!, index: index, system: system})
                )
              }
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
        );
      })}
    </div>
  );
}

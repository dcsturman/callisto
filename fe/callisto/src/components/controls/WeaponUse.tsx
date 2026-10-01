import * as React from "react";
import { useMemo, useState, useEffect, useCallback } from "react";
import { findRangeBand } from "lib/Util";
import { SHIP_SYSTEMS } from "lib/universal";
import { Ship, Entity, findShip, stringToShipSystem } from "lib/entities";
import {
  compressedWeapons,
  describeScreens,
  getWeaponName,
  findNthWeapon,
  shipWeapons,
} from "lib/shipDesignTemplates";
import {
  Weapon,
  WeaponMount,
  actionWeaponKind,
  isActionableWeapon,
  isLaserKind,
  isLauncherKind,
  isPassiveWeapon,
  fullSalvo,
  salvoChoices,
  weaponKinds,
  createWeapon,
  weaponToString,
  weaponKindLabel,
  WEAPON_COLORS,
} from "lib/weapon";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import {
  FireAction,
  FireState,
  PointDefenseState,
  SensorAction,
  SensorState,
  EngineerState,
  BoostTarget,
  DEFAULT_SENSOR_STATE,
  boostTargetEquals,
} from "components/controls/Actions";
import { ViewMode, hasRole } from "lib/view";
import { PowerSystem } from "lib/power";
import { SYSTEM_NAMES } from "components/controls/EngineerTasks";
import { setCrewActions } from "lib/serverManager";

// Icons for each type of weapon
import Turret1 from "assets/icons/turret1.svg?react";
import Turret2 from "assets/icons/turret2.svg?react";
import Turret3 from "assets/icons/turret3.svg?react";
import Barbette from "assets/icons/barbette.svg?react";
import FixedMount from "assets/icons/fixed-mount.svg?react";
import SmallBay from "assets/icons/bay-s.svg?react";
import MediumBay from "assets/icons/bay-m.svg?react";
import LargeBay from "assets/icons/bay-l.svg?react";

// Icons to show fire states.
import RayIcon from "assets/icons/laser.svg?react";
import MissileIcon from "assets/icons/missile.svg?react";
import { GiBinoculars, GiRadarSweep, GiRocket } from "react-icons/gi";
import { FaCog } from "react-icons/fa";
import { Tooltip } from "react-tooltip";
import { vectorDistance } from "lib/Util";

// State operators
import { useAppSelector, useAppDispatch } from "state/hooks";
import {
  pointDefenseWeapon,
  fireWeapon,
  unfireWeapon,
  updateFireCalledShot,
  updateFireSalvo,
  updateFireControl,
  setPointDefenseWard,
  setSensorAction,
  setEngineerAction,
  toggleBoost,
} from "state/actionsSlice";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { isUndetected, sameSide } from "lib/contacts";

// Consistent set of colors for both type of weapons and fire states.
/** Kinds the crew never orders, so they never get a fire button of their own. */
const PASSIVE_KINDS = new Set(["Sand", "PointDefense", "Repulsor"]);

/// Searching gets a colour of its own: it is the only row here that is not an
/// order, so it should not be mistaken for one of the sensop's actions.
const SEARCH_ICON_COLOR = "violet";

const SENSOR_ICON_COLORS: { [key in SensorAction]?: string } = {
  [SensorAction.JamMissiles]: "green",
  [SensorAction.JamComms]: "blue",
  [SensorAction.SensorLock]: "red",
  [SensorAction.BreakSensorLock]: "orange",
};

const ENGINEER_ICON_COLORS: { [kind: string]: string } = {
  OverloadDrive: "green",
  OverloadPlant: "yellow",
  Repair: "blue",
  Jump: "magenta",
};

const PILOT_ICON_COLORS = {
  Evade: "cyan",
  AssistGunner: "purple",
};

export const WeaponButton = (props: {
  weapon: string;
  mount: WeaponMount;
  count: number;
  onClick: () => void;
  disabled: boolean;
  /**
   * The other weapons sharing this mount, if any.
   *
   * A mixed turret gets one button per gun it can fire, so a pulse/sand turret
   * renders exactly like a pure pulse one -- same icon, same tooltip -- and a
   * ship carrying both, as the Threshing Oar does, shows two buttons that look
   * identical for no visible reason. Naming what else is in the mount, and
   * marking the button, tells them apart.
   */
  alongside?: string[];
}) => {
  // Tooltips name the weapon for a person, so they use the readable label
  // rather than the wire identifier.  Colours are still keyed off the raw kind.
  const label = weaponKindLabel(props.weapon);

  const mixed = props.alongside != null && props.alongside.length > 0;
  const tip = (text: string) =>
    mixed
      ? `${text} — shares the mount with ${props.alongside!
          .map(weaponKindLabel)
          .join(", ")}`
      : text;
  const buttonClass = mixed ? "weapon-button weapon-button-mixed" : "weapon-button";

  // FixedMount is a bare string like Barbette, so it has to be matched first or
  // it falls into the Barbette arm and draws the wrong weapon entirely.
  if (props.mount === "FixedMount") {
    return (
      <>
        <button
          id={props.weapon + "-fixed-mount-button"}
          className={buttonClass}
          data-tooltip-id={props.weapon + props.mount}
          data-tooltip-content={tip(`${label} Fixed Mount`)}
          data-tooltip-delay-show={700}
          onClick={props.onClick}
          disabled={props.disabled}
        >
          <FixedMount
            className="weapon-symbol fixed-mount-button"
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
  if (typeof props.mount === "string") {
    return (
      <>
        <button
          id={props.weapon + "-barbette-button"}
          className={buttonClass}
          data-tooltip-id={props.weapon + props.mount}
          data-tooltip-content={tip(`${label} Barbette`)}
          data-tooltip-delay-show={700}
          onClick={props.onClick}
          disabled={props.disabled}
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
            id={props.weapon + "-small-bay-button"}
            className={buttonClass}
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "small-bay"}
            data-tooltip-content={tip(`Small ${label} Bay`)}
            data-tooltip-delay-show={700}
            disabled={props.disabled}
          >
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
            className={buttonClass}
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "med-bay"}
            data-tooltip-content={tip(`Medium ${label} Bay`)}
            data-tooltip-delay-show={700}
            disabled={props.disabled}
          >
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
            id={props.weapon + "-large-bay-button"}
            className={buttonClass}
            onClick={props.onClick}
            data-tooltip-id={props.weapon + "large-bay"}
            data-tooltip-content={tip(`Large ${label} Bay`)}
            data-tooltip-delay-show={700}
            disabled={props.disabled}
          >
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
            className={buttonClass}
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={tip(`Single ${label} Turret`)}
            data-tooltip-delay-show={700}
            disabled={props.disabled}
          >
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
            className={buttonClass}
            onClick={props.onClick}
            data-tooltip-id={props.weapon + num + "turret"}
            data-tooltip-content={tip(`Double ${label} Turret`)}
            data-tooltip-delay-show={700}
            disabled={props.disabled}
          >
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
          className={buttonClass}
          onClick={props.onClick}
          data-tooltip-id={props.weapon + num + "turret"}
          data-tooltip-content={tip(`Triple ${label} Turret`)}
          data-tooltip-delay-show={700}
          disabled={props.disabled}
        >
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
  const range = findRangeBand(
    vectorDistance(args.attacker.position, args.target.position),
  );

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
      }}
    >
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
    [computerShipName, entities],
  );
  // The ship's own armament, which is not necessarily its design's — see
  // `shipWeapons`.  Every weapon_id below indexes into this list.
  const computerShipWeapons = useMemo(
    () => shipWeapons(computerShip, shipTemplates),
    [shipTemplates, computerShip],
  );
  const dispatch = useAppDispatch();

  const weaponDetails = useMemo(
    () => compressedWeapons(computerShipWeapons),
    [computerShipWeapons],
  );

  const availableCounts = useMemo(() => {
    const counts = {} as { [key: string]: number };
    // Count up all the actions by weapon
    if (actions[computerShipName!]?.fire) {
      for (const action of actions[computerShipName!].fire) {
        counts[getWeaponName(computerShipWeapons, action.weapon_id)] =
          (counts[getWeaponName(computerShipWeapons, action.weapon_id)] || 0) +
          1;
      }
    }

    // Count up all the actions in point defense
    if (actions[computerShipName!]?.pointDefense) {
      for (const action of actions[computerShipName!].pointDefense) {
        counts[getWeaponName(computerShipWeapons, action.weapon_id)] =
          (counts[getWeaponName(computerShipWeapons, action.weapon_id)] || 0) +
          1;
      }
    }

    const available = {} as { [key: string]: number };
    // Subtract all the counts (if the exist) from the total counts
    for (const weapon in weaponDetails) {
      available[weapon] = weaponDetails[weapon].total - (counts[weapon] || 0);
    }
    return available;
  }, [computerShipName, computerShipWeapons, weaponDetails, actions]);

  const [fireTarget, setFireTarget] = useState<Entity | null>(null);

  useEffect(() => {
    if (computerShipName === fireTarget?.name) {
      setFireTarget(null);
    }
  }, [computerShipName, fireTarget]);

  const POINT_DEFENSE_NAME = useMemo(() => "<Point Defense>", []);
  const POINT_DEFENSE_PHANTOM = useMemo(
    () =>
      ({
        name: POINT_DEFENSE_NAME,
        position: [0, 0, 0],
        velocity: [0, 0, 0],
      }) as Entity,
    [POINT_DEFENSE_NAME],
  );

  const handleFireCommand = useCallback(
    (attacker: string, target: string, weapon_name: string, firingKind?: string) => {
      if (computerShipWeapons.length === 0) {
        console.error(
          "(Controls.handleFireCommand) No weapons known for " + attacker + ".",
        );
        return;
      }

      const weapon_id = findNthWeapon(
        computerShipWeapons,
        weapon_name,
        weaponDetails[weapon_name].total - availableCounts[weapon_name] + 1,
      );
      if (availableCounts[weapon_name] === 0) {
        console.log(
          "(Controls.handleFireCommand) No more weapons of type " +
            weapon_id +
            " for " +
            attacker +
            ".",
        );
        return;
      }

      if (target === POINT_DEFENSE_NAME) {
        dispatch(
          pointDefenseWeapon({ shipName: attacker, weapon_id: weapon_id }),
        );
      } else {
        dispatch(
          fireWeapon({
            shipName: attacker,
            weapon_id: weapon_id,
            target: target,
            entities: entities,
            firing_kind: firingKind,
          }),
        );
      }
    },
    [
      computerShipWeapons,
      weaponDetails,
      availableCounts,
      dispatch,
      entities,
      POINT_DEFENSE_NAME,
    ],
  );

  const formatter = useCallback(
    (name: string, entity: Entity) => {
      if (computerShip) {
        return `${name} (${findRangeBand(vectorDistance(computerShip.position, entity.position))})`;
      } else {
        return "";
      }
    },
    [computerShip],
  );

  const filter = useMemo(() => [EntitySelectorType.Ship], []);

  const handleWeaponClick = useCallback(
    (weapon_name: string, firingKind?: string) => {
      if (!computerShipName) {
        return;
      }
      handleFireCommand(
        computerShipName,
        fireTarget ? fireTarget.name : "",
        weapon_name,
        firingKind,
      );
    },
    [handleFireCommand, computerShipName, fireTarget],
  );

  const isWeaponDisabled = useCallback(
    (weapon: { kind: string; mount: WeaponMount }) => {
      return (
        !fireTarget ||
        (fireTarget.name === POINT_DEFENSE_NAME &&
          !(
            isLaserKind(weapon.kind ?? "") &&
            weapon.mount !== "Turret"
          ))
      );
    },
    [fireTarget, POINT_DEFENSE_NAME],
  );

  const weaponButtons = useMemo(
    () =>
      computerShipName &&
      Object.entries(compressedWeapons(computerShipWeapons)).flatMap(
        ([weapon_name, weapon]) => {
          if (!isActionableWeapon(weapon)) {
            return [];
          }
          // A mixed turret may only use one type per round, so it gets a button
          // per orderable type rather than one for the mount.  A uniform mount
          // has a single kind and so still renders exactly one button.
          const kinds = weaponKinds(
            weapon.guns != null
              ? { mount: weapon.mount, guns: weapon.guns }
              : { kind: weapon.kind, mount: weapon.mount },
          ).filter((kind) => !PASSIVE_KINDS.has(kind));
          const choices = kinds.length > 0 ? kinds : [weapon.kind];
          const mixed = choices.length > 1;
          return choices.map((kind) => (
            <WeaponButton
              key={"weapon-" + computerShipName + "-" + weapon_name + "-" + kind}
              weapon={kind}
              mount={weapon.mount}
              count={availableCounts[weapon_name]}
              onClick={() =>
                handleWeaponClick(weapon_name, mixed ? kind : undefined)
              }
              disabled={isWeaponDisabled({ kind, mount: weapon.mount })}
              // Everything else in the mount, including the guns that cannot be
              // fired: sand is exactly what distinguishes a mixed turret from a
              // plain one, and it never gets a button of its own.
              alongside={weaponKinds(
                weapon.guns != null
                  ? { mount: weapon.mount, guns: weapon.guns }
                  : { kind: weapon.kind, mount: weapon.mount },
              ).filter((other) => other !== kind)}
            />
          ));
        },
      ),
    [
      computerShipName,
      computerShipWeapons,
      availableCounts,
      handleWeaponClick,
      isWeaponDisabled,
    ],
  );

  // Defences that run themselves get no button, but a referee still needs to
  // know the ship has them -- otherwise point defence is invisible everywhere
  // once a ship is in play.
  const passiveDefences = useMemo(() => {
    const entries = Object.values(compressedWeapons(computerShipWeapons)).filter((weapon) =>
      isPassiveWeapon(
        weapon.guns != null
          ? { mount: weapon.mount, guns: weapon.guns }
          : createWeapon(weapon.kind, weapon.mount),
      ),
    );
    const weapons = entries.map((weapon) => {
      const name = weaponToString(
        weapon.guns != null
          ? { mount: weapon.mount, guns: weapon.guns }
          : createWeapon(weapon.kind, weapon.mount),
      );
      return weapon.total > 1 ? `${name} x${weapon.total}` : name;
    });
    // Screens are not weapons and live on the design rather than the ship, but
    // they are automatic defences and belong in the same readout.
    const design = computerShip?.design
      ? shipTemplates[computerShip.design]
      : undefined;
    return [...weapons, ...describeScreens(design?.screens)];
  }, [computerShipWeapons, computerShip, shipTemplates]);

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
          observer={computerShip}
          excludeSameTeam
        />
      </div>
      <div className="weapon-list">{weaponButtons}</div>
      {passiveDefences.length > 0 && (
        <div className="weapon-passive-list">
          <span className="weapon-passive-label">Automatic:</span>{" "}
          {passiveDefences.join(", ")}
        </div>
      )}
    </>
  );
};

/**
 * The round's queued orders for one ship, gathered from the action queue.
 *
 * `only` narrows it to one station's orders, which is what a station card
 * passes. The captain's card passes nothing and gets the lot: they inspire
 * other people's checks, and hunting five cards for the actions to tick was
 * unworkable.
 *
 * Renders nothing when there is nothing queued, so a quiet card stays quiet.
 */
export function QueuedOrders(args: {ship: Ship; only?: ViewMode[]}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const queued = useAppSelector((state) => state.actions[args.ship.name]);

  // A station card shows its own orders; the captain's shows every station's.
  // Either way a player only sees what their roles let them see.
  const shows = (role: ViewMode) =>
    args.only == null
      ? hasRole(roles, role, ViewMode.Captain)
      : args.only.includes(role);

  const fireActions = shows(ViewMode.Gunner) ? (queued?.fire ?? []) : [];
  const pdActions = shows(ViewMode.Gunner) ? (queued?.pointDefense ?? []) : [];
  const sensorActions = shows(ViewMode.Sensors) ? (queued?.sensors ?? []) : [];
  const engineerActions = shows(ViewMode.Engineer) ? (queued?.engineers ?? []) : [];
  const pilotState = shows(ViewMode.Pilot)
    ? {
        dodgeThrust: args.ship.dodge_thrust ?? 0,
        assistGunners: args.ship.assist_gunners ?? false,
      }
    : null;

  // Ships this one could be looking for. Detection is free and needs no
  // order, so these are not queued actions -- they are here because a captain
  // can concentrate the sensop on one of them, and that is the only part of
  // detection leadership reaches.
  const watchingSensors = shows(ViewMode.Sensors);
  const searchTargets = useMemo(
    () =>
      watchingSensors
        ? entities.ships.filter(
            (target) =>
              target.name !== args.ship.name &&
              isUndetected(args.ship, target) &&
              !sameSide(args.ship, target),
          )
        : [],
    [entities.ships, args.ship, watchingSensors],
  );

  const anything =
    fireActions.length > 0 ||
    pdActions.length > 0 ||
    sensorActions.some((sensor) => sensor.action !== SensorAction.None) ||
    engineerActions.some((engineer) => engineer != null) ||
    searchTargets.length > 0 ||
    (pilotState != null && (pilotState.dodgeThrust > 0 || pilotState.assistGunners));
  if (!anything) {
    return null;
  }

  return (
    <Actions
      fireActions={fireActions}
      pointDefenseActions={pdActions}
      sensorActions={sensorActions}
      engineerActions={engineerActions}
      pilotState={pilotState}
      searchTargets={searchTargets}
      weapons={shipWeapons(args.ship, templates)}
      gunnery={args.ship.crew?.gunnery}
    />
  );
}


/**
 * How much of the Fire Control pool this shot draws.
 *
 * "Allows the computer to fire a number of turrets per round equal to the
 * listed number. Alternatively, it can give a positive DM to an attack equal
 * to the listed number or any combination of the two" (Core Rulebook p. 161).
 * So the score is a pool of points each round: one point fires a mount
 * outright, and the rest can go on improving shots. A mount with nobody on it
 * is the usual place to spend the first point, which is why the checkbox says
 * so when the gunner's seat is empty.
 */
function FireControlRow(args: {
  action: FireAction;
  index: number;
  pool: number;
  spent: number;
  shipName: string;
  hasGunner: boolean;
}) {
  const dispatch = useAppDispatch();
  const mine = (args.action.fire_control_dm ?? 0) + (args.action.computer_fired ? 1 : 0);
  const left = args.pool - args.spent;
  const dm = args.action.fire_control_dm ?? 0;
  const choices = Array.from({length: dm + Math.max(0, left) + 1}, (_, value) => value);

  return (
    <div className="fire-control-row" title={`Fire Control/${args.pool}: ${args.pool - args.spent} of ${args.pool} points unspent`}>
      <label
        className="fire-control-toggle"
        title={
          args.hasGunner
            ? "The computer fires this mount instead of its gunner, bringing no skill of its own."
            : "Nobody is on this mount, so the computer fires it. One Fire Control point."
        }>
        <input
          type="checkbox"
          checked={args.action.computer_fired === true}
          disabled={!args.action.computer_fired && left <= 0}
          onChange={(event) =>
            dispatch(
              updateFireControl({
                shipName: args.shipName,
                index: args.index,
                computerFired: event.target.checked,
              })
            )
          }
        />
        computer fires
      </label>
      <label className="fire-control-dm" title="Fire Control points added to this shot">
        DM
        <select
          className="salvo-select"
          value={dm}
          onChange={(event) =>
            dispatch(
              updateFireControl({
                shipName: args.shipName,
                index: args.index,
                dm: Number(event.target.value),
              })
            )
          }>
          {choices.map((value) => (
            <option key={value} value={value}>
              {value === 0 ? "—" : `+${value}`}
            </option>
          ))}
        </select>
      </label>
      <span className="fire-control-left">
        {mine > 0 ? `${mine} used, ` : ""}
        {Math.max(0, left)} left
      </span>
    </div>
  );
}

export function Actions(args: {
  fireActions: FireState;
  pointDefenseActions: PointDefenseState;
  /**
   * Ships this one could be searching for: another side, not yet found.
   *
   * Not actions — detection is free and needs no order. They appear so a
   * captain can concentrate the sensop on one of them, which is the only part
   * of detection leadership reaches. Supplied by the caller because the caller
   * knows which roles should see them.
   */
  searchTargets: Ship[];
  /** One per sensor operator, by their place in the crew. */
  sensorActions: SensorState[];
  /** One per engineer. */
  engineerActions: EngineerState[];
  pilotState: { dodgeThrust: number; assistGunners: boolean } | null;
  // The acting ship's own armament, not its design's: `weapon_id` indexes this.
  weapons: Weapon[];
  /** Gunner skill per mount, for spotting the ones with nobody on them. */
  gunnery?: number[];
}) {
  const entities = useAppSelector(entitiesSelector);
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const roles = useAppSelector((state) => state.user.roles);
  const userShipName = useAppSelector((state) => state.user.shipName);
  // Boost checkboxes render for Captains and Generals (per spec: "There should
  // be a check box for the captain (and general of course) view to the far
  // right of each action."). Captains always issue boosts against their own
  // assigned ship. Generals (the GM-style role) might have no assigned ship —
  // in that case fall back to the currently-viewed ship so the General can
  // roll leadership on whichever ship's popup they're inspecting.
  const captainShipName = userShipName ?? computerShipName;
  const showBoostCheckbox = hasRole(roles, ViewMode.Captain);
  const boostDispatchEnabled = showBoostCheckbox && captainShipName != null;
  const captainBoosts = useAppSelector((state) => {
    if (!captainShipName) return [] as BoostTarget[];
    return state.actions[captainShipName]?.leadershipCheck?.boosts ?? [];
  });
  const dispatch = useAppDispatch();

  // The Fire Control pool for the round, and what the orders already drew
  // from it. The program's score is spent on firing mounts outright and on
  // improving gunners' shots, in any mix (Core Rulebook p. 161).
  const actingShip = useMemo(
    () => (computerShipName ? findShip(entities, computerShipName) : null),
    [entities, computerShipName]
  );
  const fireControlPool = useMemo(
    () =>
      actingShip?.software_running?.find((software) => software.kind === "FireControl")?.level ?? 0,
    [actingShip]
  );
  const fireControlSpent = useMemo(
    () =>
      args.fireActions.reduce(
        (total, action) =>
          total + (action.fire_control_dm ?? 0) + (action.computer_fired ? 1 : 0),
        0
      ),
    [args.fireActions]
  );

  // Point Defence software, and who it could cover: a ship running it can
  // shoot down what is coming at a neighbour rather than at itself.
  const pointDefenceReach = useMemo(
    () =>
      actingShip?.software_running?.find((software) => software.kind === "PointDefence")?.level ?? 0,
    [actingShip]
  );
  const coverable = useMemo(
    () =>
      actingShip == null
        ? []
        : entities.ships
            .filter((other) => other.name !== actingShip.name)
            .map((other) => other.name),
    [entities.ships, actingShip]
  );

  // Captain's leadership cap. The cap is the rolled `leadership_points`, but
  // only after the captain has actually rolled this turn — pre-roll
  // `leadership_points` is stale from last turn, so gate on `leadership_rolled`.
  const captainShip = useMemo(
    () => (captainShipName ? findShip(entities, captainShipName) : null),
    [entities, captainShipName],
  );
  const cap = useMemo(() => {
    if (!captainShip) return 0;
    if (!(captainShip.leadership_rolled ?? false)) return 0;
    return Math.max(0, captainShip.leadership_points ?? 0);
  }, [captainShip]);
  const atLimit = captainBoosts.length >= cap;

  const isBoosted = (target: BoostTarget) =>
    captainBoosts.some((b) => boostTargetEquals(b, target));

  const onBoostToggle = (target: BoostTarget) => {
    if (!captainShipName) return;
    dispatch(toggleBoost({ shipName: captainShipName, target }));
  };

  const renderBoostCheckbox = (target: BoostTarget, noCheckReason?: string) => {
    if (!showBoostCheckbox) return null;
    const checked = boostDispatchEnabled ? isBoosted(target) : false;
    // Checked boxes stay toggleable so the user can free a slot. Only
    // unchecked-at-limit and the no-ship-bound case disable.
    // Some actions resolve without a skill check, so there is nothing for a
    // boost to modify. The row keeps its box so the column still lines up, but
    // it is disabled and says why.
    const disabled =
      noCheckReason != null || !boostDispatchEnabled || (!checked && atLimit);
    let title: string;
    if (noCheckReason != null) {
      title = noCheckReason;
    } else if (!boostDispatchEnabled) {
      title = "Sit on a ship to apply leadership boosts";
    } else if (!(captainShip?.leadership_rolled ?? false)) {
      title = "Roll the captain action first";
    } else if (atLimit && !checked) {
      title = "Captain limit reached — uncheck another to free a slot";
    } else {
      title = "Boost this action with leadership";
    }
    return (
      <span className="boost-checkbox-cell">
        <input
          type="checkbox"
          className="boost-checkbox"
          checked={checked}
          onChange={() => onBoostToggle(target)}
          disabled={disabled}
          title={title}
        />
      </span>
    );
  };

  const computerShip = useMemo(
    () => findShip(entities, computerShipName),
    [computerShipName, entities],
  );

  const onClick = (weapon_id: number) => {
    dispatch(
      unfireWeapon({ shipName: computerShipName!, weapon_id: weapon_id }),
    );
  };

  // Clicking a queued action withdraws it -- that operator's or engineer's
  // alone, since each is a different pair of hands.
  const onSensorRowClick = (operator: number) => {
    if (!computerShipName) return;
    dispatch(
      setSensorAction({
        shipName: computerShipName,
        operator,
        action: DEFAULT_SENSOR_STATE,
      }),
    );
  };

  const onEngineerRowClick = (engineer: number) => {
    if (!computerShipName) return;
    dispatch(setEngineerAction({ shipName: computerShipName, engineer, action: null }));
  };

  const sensorLabelOf = (sensor: SensorState): string | null => {
    switch (sensor.action) {
      case SensorAction.None:
        return null;
      case SensorAction.JamMissiles:
        return "Jam Missiles";
      case SensorAction.SensorLock:
        return "Lock on " + sensor.target;
      case SensorAction.BreakSensorLock:
        return "Break Lock on " + sensor.target;
      case SensorAction.JamComms:
        return "Jam " + sensor.target;
    }
  };

  const engineerLabelOf = (action: EngineerState): string | null => {
    if (action == null) {
      return null;
    }
    switch (action.kind) {
      case "OverloadDrive":
        return "Overload Drive";
      case "OverloadPlant":
        return "Overload Plant";
      case "Repair": {
        const sys = stringToShipSystem(action.system);
        return "Repair " + (sys != null ? SYSTEM_NAMES[sys] : action.system);
      }
      case "Jump":
        return "Jump";
      case "SetPower":
        return `${action.online ? "Power up" : "Power down"} ${powerSystemLabel(action.system)}`;
    }
  };

  // A power order names what it is switching. A weapon is named by its mount,
  // which is how the rest of the console refers to it.
  const powerSystemLabel = (system: PowerSystem): string => {
    if (typeof system === "string") {
      return system === "Maneuver" ? "the m-drive" : system === "Jump" ? "the j-drive" : system.toLowerCase();
    }
    if ("Feature" in system) {
      return `ship feature ${system.Feature + 1}`;
    }
    const weapon = args.weapons[system.Weapon];
    return weapon == null ? `mount ${system.Weapon + 1}` : weaponToString(weapon);
  };

  // Several operators or engineers mean the row has to say whose it is. One
  // of each says nothing, as before.
  const crewTag = (index: number, count: number) => (count > 1 ? `#${index + 1} ` : "");

  return (
    <div className="control-form">
      <h2>Actions</h2>
      {showBoostCheckbox && (
        <div className="actions-column-header">
          <span className="boost-column-label">Assist</span>
        </div>
      )}
      {args.fireActions.map((action, index) => {
        // The weapon's own type.  This used to be a chain that mapped anything
        // other than Beam, Pulse or Particle to the literal string "Missile",
        // so every weapon added since -- fusion, meson, plasma, railgun, mass
        // driver, ion -- was drawn and coloured as a missile.
        const kind = actionWeaponKind(
          args.weapons[action.weapon_id],
          action.firing_kind,
        );

        const fireBoostTarget: BoostTarget = {
          kind: "Fire",
          ship: computerShipName ?? "",
          weapon_id: action.weapon_id,
        };

        // Anything that is not a launcher is direct fire and draws as a beam.
        // This used to be a hardcoded list of Beam, Pulse and Particle, so every
        // weapon added since -- fusion, meson, plasma, railgun, mass driver,
        // ion -- fell through and drew a missile.
        return !isLauncherKind(kind) ? (
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
                  updateFireCalledShot({
                    shipName: computerShipName!,
                    index: index,
                    system: system,
                  }),
                )
              }
            />
            {renderBoostCheckbox(fireBoostTarget)}
          </div>
        ) : (
          <div
            className="fire-actions-div"
            key={index + "_fire_img"}
          >
            <div onClick={() => onClick(action.weapon_id)}>
              <p>
                <MissileIcon
                  className="missile-type-icon"
                  style={{
                    fill: WEAPON_COLORS[kind],
                  }}
                />{" "}
                <span className="fire-action-weapon">
                  {weaponKindLabel(kind)}
                </span>{" "}
                to {action.target}
              </p>
            </div>
            {/* How much of the rack to throw. A full salvo is the default;
                a gunner may want one away as a warning shot, or to keep the
                rest for a second target. */}
            {(() => {
              const full = fullSalvo(args.weapons[action.weapon_id], kind);
              if (full == null || full <= 1) {
                return null;
              }
              const chosen = action.salvo_size ?? full;
              return (
                <select
                  className="salvo-select"
                  value={chosen}
                  title={`How many to launch, of ${full}`}
                  onChange={(event) => {
                    const size = Number(event.target.value);
                    dispatch(
                      updateFireSalvo({
                        shipName: computerShipName!,
                        index: index,
                        size: size === full ? null : size,
                      }),
                    );
                  }}
                >
                  {salvoChoices(full).map((size) => (
                    <option key={size} value={size}>
                      {size === full ? `all ${size}` : size}
                    </option>
                  ))}
                </select>
              );
            })()}
            {/* The computer's share of this shot, when the ship is running
                Fire Control. One point has the computer fire a mount with
                nobody on it; the rest can be spent improving the shot. */}
            {fireControlPool > 0 && (
              <FireControlRow
                action={action}
                index={index}
                pool={fireControlPool}
                spent={fireControlSpent}
                shipName={computerShipName ?? ""}
                hasGunner={(args.gunnery?.[action.weapon_id] ?? 0) > 0}
              />
            )}
            {renderBoostCheckbox(
              fireBoostTarget,
              "Launching makes no check, so there is nothing to boost",
            )}
          </div>
        );
      })}

      {args.pointDefenseActions.map((action, index) => {
        // A mixed turret has no single kind, so ask which gun is on duty.
        const kind = actionWeaponKind(args.weapons[action.weapon_id]);
        if (!isLaserKind(kind)) {
          console.error(
            "(Actions) Illegal weapon kind for point defense: " +
              args.weapons[action.weapon_id].kind,
          );
          return (
            <div className="fire-actions-div" key={index + "bug"}>
              This is a bug
            </div>
          );
        }

        const pdBoostTarget: BoostTarget = {
          kind: "PointDefense",
          ship: computerShipName ?? "",
          weapon_id: action.weapon_id,
        };

        // Point defence is lasers only (High Guard p. 30).
        return isLaserKind(kind) ? (
          <div className="fire-actions-div" key={index + "_pd_img"}>
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
            {/* A ship running Point Defence software can cover a neighbour
                instead of itself (High Guard p. 75). Only worth offering
                when there is a neighbour and the software to do it. */}
            {pointDefenceReach > 0 && coverable.length > 0 && (
              <select
                className="salvo-select"
                value={action.protecting ?? ""}
                title={`Point Defence/${pointDefenceReach}: cover another ship nearby instead of this one`}
                onChange={(event) =>
                  dispatch(
                    setPointDefenseWard({
                      shipName: computerShipName ?? "",
                      weapon_id: action.weapon_id,
                      protecting: event.target.value === "" ? null : event.target.value,
                    })
                  )
                }>
                <option value="">our own ship</option>
                {coverable.map((name) => (
                  <option key={name} value={name}>
                    cover {name}
                  </option>
                ))}
              </select>
            )}
            {renderBoostCheckbox(pdBoostTarget)}
          </div>
        ) : (
          <></>
        );
      })}

      {/* Detection is free and happens every round, so there is no order to
          queue and nothing to click off. These rows exist so a captain can put
          the sensop's attention on one particular ship. */}
      {args.searchTargets.map((target) => (
        <div className="fire-actions-div" key={"search-" + target.name}>
          <div>
            <p>
              <GiRadarSweep
                className="beam-type-icon"
                style={{ fill: SEARCH_ICON_COLOR }}
              />{" "}
              Searching for {target.name}
            </p>
          </div>
          {renderBoostCheckbox({
            kind: "Detection",
            ship: computerShipName ?? "",
            target: target.name,
          })}
        </div>
      ))}

      {args.pilotState != null && args.pilotState.dodgeThrust > 0 && (
        <div className="fire-actions-div">
          <div
            onClick={() =>
              computerShipName &&
              setCrewActions(
                computerShipName,
                0,
                args.pilotState!.assistGunners,
              )
            }
          >
            <p>
              <GiRocket
                className="beam-type-icon"
                style={{ fill: PILOT_ICON_COLORS.Evade }}
              />{" "}
              Evade
            </p>
          </div>
          {renderBoostCheckbox({ kind: "Evade", ship: computerShipName ?? "" })}
        </div>
      )}

      {args.pilotState != null && args.pilotState.assistGunners && (
        <div className="fire-actions-div">
          <div
            onClick={() =>
              computerShipName &&
              setCrewActions(
                computerShipName,
                args.pilotState!.dodgeThrust,
                false,
              )
            }
          >
            <p>
              <GiRocket
                className="beam-type-icon"
                style={{ fill: PILOT_ICON_COLORS.AssistGunner }}
              />{" "}
              Assist Gunner
            </p>
          </div>
          {renderBoostCheckbox({
            kind: "AssistGunner",
            ship: computerShipName ?? "",
          })}
        </div>
      )}

      {args.sensorActions.map((sensor, operator) => {
        const label = sensorLabelOf(sensor);
        if (label == null) {
          return null;
        }
        return (
          <div className="fire-actions-div" key={`sensor-${operator}`}>
            <div onClick={() => onSensorRowClick(operator)}>
              <p>
                <GiBinoculars
                  className="beam-type-icon"
                  style={{ fill: SENSOR_ICON_COLORS[sensor.action] }}
                />{" "}
                {crewTag(operator, args.sensorActions.length)}
                {label}
              </p>
            </div>
            {renderBoostCheckbox({
              kind: "Sensor",
              ship: computerShipName ?? "",
              operator,
            })}
          </div>
        );
      })}

      {args.engineerActions.map((action, engineer) => {
        const label = engineerLabelOf(action);
        if (label == null || action == null) {
          return null;
        }
        return (
          <div className="fire-actions-div" key={`engineer-${engineer}`}>
            <div onClick={() => onEngineerRowClick(engineer)}>
              <p>
                <FaCog
                  className="beam-type-icon"
                  style={{ fill: ENGINEER_ICON_COLORS[action.kind] }}
                />{" "}
                {crewTag(engineer, args.engineerActions.length)}
                {label}
              </p>
            </div>
            {renderBoostCheckbox({
              kind: "Engineer",
              ship: computerShipName ?? "",
              engineer,
            })}
          </div>
        );
      })}
    </div>
  );
}

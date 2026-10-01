import * as React from "react";
import {useMemo} from "react";

import {Ship, ShipSystem, stringToShipSystem} from "lib/entities";
import {SYSTEM_NAMES} from "components/controls/EngineerTasks";
import {useAppDispatch, useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";
import {setComputerRepair} from "state/actionsSlice";
import {ShipDesignTemplate} from "lib/shipDesignTemplates";

/** A stable empty list, so the selector does not re-render on every tick. */
const EMPTY_REPAIRS: ShipSystem[] = [];

/** Whether the hull carries the drones Auto-Repair works through. */
const hasRepairDrones = (design: ShipDesignTemplate | undefined): boolean =>
  (design?.features ?? []).some((feature) => feature.kind === "RepairDrones");

/** Severity a hit has to reach before a system is in real trouble. */
const SEVERE = 4;

/**
 * The engineer's damage control board.
 *
 * Every system, how badly it is hurt, and what a repair would roll. The
 * numbers were all there already -- the severity on the ship, the accumulated
 * bonus from working the same job, the engineer's own skill -- but spread
 * between a dropdown and the results log, so nobody could see at a glance
 * what was worth trying.
 */
export function DamageBoard(args: {ship: Ship; engineer?: number}) {
  const templates = useAppSelector(templatesSelector);
  const design = templates[args.ship.design];
  const dispatch = useAppDispatch();
  const engineer = (args.ship.crew.engineers ?? [])[args.engineer ?? 0];

  const rows = useMemo(() => {
    const levels = args.ship.crit_level ?? [];
    return levels
      .map((level, index) => ({system: index as ShipSystem, level}))
      .filter((row) => row.level > 0);
  }, [args.ship.crit_level]);

  // What the repair of this system would roll: the engineer's skill for that
  // job, less the severity, plus anything already earned by working at it.
  const repairDm = (system: ShipSystem, level: number): number => {
    const skill =
      system === ShipSystem.Jump
        ? (engineer?.jump ?? 0)
        : system === ShipSystem.Powerplant
          ? (engineer?.power ?? 0)
          : system === ShipSystem.Weapon || system === ShipSystem.Sensors || system === ShipSystem.Bridge
            ? (engineer?.mechanic ?? 0)
            : (engineer?.maneuver ?? 0);
    const earned =
      args.ship.last_repair_component != null &&
      stringToShipSystem(args.ship.last_repair_component) === system
        ? (args.ship.repair_bonus ?? 0)
        : 0;
    return skill + earned - level;
  };

  // Auto-Repair's pool for the round, and what is already ordered out of it.
  // Whatever is left rides along with an engineer's own repair as a DM, so
  // nothing is wasted by not spending it here.
  const autoRepairPool = hasRepairDrones(design)
    ? (args.ship.software_running?.find((software) => software.kind === "AutoRepair")?.level ?? 0)
    : 0;
  const computerRepairs = useAppSelector(
    (state) => state.actions[args.ship.name]?.computerRepairs ?? EMPTY_REPAIRS
  );
  const autoRepairLeft = autoRepairPool - computerRepairs.length;

  const overloads = [
    {label: "M-drive", attempts: args.ship.overload_drive_attempts ?? 0},
    {label: "Power plant", attempts: args.ship.overload_plant_attempts ?? 0},
  ].filter((entry) => entry.attempts > 0);

  return (
    <div className="damage-board">
      {rows.length === 0 ? (
        <p className="sensor-empty">No damage.</p>
      ) : (
        <ul className="damage-rows">
          {rows.map((row) => {
            const dm = repairDm(row.system, row.level);
            const ordered = computerRepairs.includes(row.system);
            return (
              <li key={row.system} className="damage-row">
                <span className="damage-system">{SYSTEM_NAMES[row.system]}</span>
                <SeverityPips level={row.level} />
                <span
                  className={dm >= 0 ? "damage-dm" : "damage-dm damage-dm-poor"}
                  title={`A repair rolls 2D ${dm >= 0 ? "+" : ""}${dm} against 8`}>
                  {dm >= 0 ? `+${dm}` : dm}
                </span>
                {/* The computer can work a system itself when the ship has
                    Auto-Repair running and drones to send out -- which is
                    how a ship with no engineer to spare still gets fixed. */}
                {autoRepairPool > 0 && row.system !== ShipSystem.Hull && (
                  <label
                    className="damage-drones"
                    title={
                      ordered
                        ? "The drones work this system this round, at Engineer 1."
                        : autoRepairLeft > 0
                          ? "Send the drones to this system: one Auto-Repair point, at Engineer 1."
                          : "No Auto-Repair points left this round."
                    }>
                    <input
                      type="checkbox"
                      checked={ordered}
                      disabled={!ordered && autoRepairLeft <= 0}
                      onChange={(event) =>
                        dispatch(
                          setComputerRepair({
                            shipName: args.ship.name,
                            system: row.system,
                            repair: event.target.checked,
                          })
                        )
                      }
                    />
                    drones
                  </label>
                )}
              </li>
            );
          })}
        </ul>
      )}
      {/* Repeated overloads are a penalty the engineer carries for the rest of
          the fight, and nothing else on screen says so. */}
      {overloads.length > 0 && (
        <ul className="damage-overloads">
          {overloads.map((entry) => (
            <li key={entry.label} className="damage-overload">
              {entry.label} overloaded {entry.attempts}×: next attempt at DM
              {-2 * entry.attempts}
            </li>
          ))}
        </ul>
      )}
      {design != null && (
        <div className="damage-hull">
          Hull {args.ship.current_hull}/{design.hull} · Armour {args.ship.current_armor}/{design.armor}
        </div>
      )}
    </div>
  );
}

/**
 * Severity as a row of pips rather than a number: six of them, filled to the
 * severity, so a glance says how close a system is to being written off.
 */
function SeverityPips(args: {level: number}) {
  return (
    <span
      className={args.level >= SEVERE ? "severity severity-bad" : "severity"}
      title={`Severity ${args.level}`}>
      {[1, 2, 3, 4, 5, 6].map((pip) => (
        <span
          key={pip}
          className={pip <= args.level ? "severity-pip severity-pip-lit" : "severity-pip"}
        />
      ))}
    </span>
  );
}

export default DamageBoard;

import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {
  PowerLine,
  PowerSystem,
  availablePower,
  isPowered,
  powerDemand,
  powerLines,
  powerSpare,
  powerSystemKey,
  samePowerSystem,
} from "lib/power";
import {shipWeapons} from "lib/shipDesignTemplates";
import {EngineerState} from "components/controls/Actions";
import {setEngineerAction} from "state/actionsSlice";
import {useAppDispatch, useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";

/**
 * The engineer's power console.
 *
 * A column per system, lit to the height of its draw: full and glowing while
 * it runs, dark when the engineer has shut it down or damage has taken it.
 * The plant's output is the line across the top, so a ship drawing more than
 * it makes is obvious before anyone reads a number.
 *
 * Switching a system off is this engineer's action for the round (Core
 * Rulebook p. 171), so the buttons queue it like any other order.
 */
export function PowerBoard(args: {ship: Ship}) {
  const templates = useAppSelector(templatesSelector);
  const dispatch = useAppDispatch();
  const queuedActions = useAppSelector((state) => state.actions[args.ship.name]?.engineers ?? []);

  // A power order is somebody's action for the round. The board belongs to the
  // ship rather than to one engineer, so it gives the job to the first of them
  // with nothing else to do, and falls back to the first engineer when they
  // are all busy -- replacing whatever that one was going to do, as any other
  // order would.
  const crew = args.ship.crew.engineers ?? [];
  const engineer = useMemo(() => {
    const free = Array.from({length: Math.max(1, crew.length)}, (_, index) => index).find(
      (index) => (queuedActions[index] ?? null) == null
    );
    return free ?? 0;
  }, [crew.length, queuedActions]);
  const queued = queuedActions[engineer] ?? null;

  const design = templates[args.ship.design];
  const lines = useMemo(
    () =>
      design == null
        ? []
        : powerLines(args.ship, design, shipWeapons(args.ship, templates)),
    [args.ship, design, templates]
  );

  if (design == null) {
    return null;
  }

  const supply = availablePower(args.ship);
  const demand = powerDemand(lines);
  const spare = powerSpare(args.ship, lines);
  const short = demand - supply;

  const order = (system: PowerSystem, online: boolean) => {
    const action: EngineerState = {kind: "SetPower", system, online};
    dispatch(setEngineerAction({shipName: args.ship.name, engineer, action}));
  };

  const queuedFor = (system: PowerSystem): boolean =>
    queued?.kind === "SetPower" && samePowerSystem(queued.system, system);

  // A queued order on basic systems: `online: false` is the order to run them
  // at half. Undefined when no such order is waiting.
  const halfOrdered =
    queued?.kind === "SetPower" && queued.system === "Basic" ? !queued.online : null;

  return (
    <div className="power-board">
      <div className="power-summary">
        <span className="power-supply">{supply} available</span>
        <span className={short > 0 ? "power-spare power-short" : "power-spare"}>
          {short > 0 ? `${short} short` : `${spare} spare`}
        </span>
      </div>
      {/* Each column is full at its own requirement, so a bar that is not
          full is a system that is not getting what it needs -- which is the
          thing the engineer is looking for. */}
      <div className="power-bars">
        {lines.map((line) => (
          <PowerColumn
            key={powerSystemKey(line.system)}
            line={line}
            spare={spare}
            queued={queuedFor(line.system)}
            onToggle={() => order(line.system, !line.online)}
          />
        ))}
      </div>
      {/* Basic systems cannot be shut off, but the rules let a desperate crew
          run them at half (High Guard p. 17). Throwing that switch is an
          engineer's round like any other order, so the box shows the order
          as queued until the round resolves -- it used to flip back and
          could not be undone. */}
      <label className="power-half" title="Run life support, gravity and heat at half power">
        <input
          type="checkbox"
          checked={halfOrdered ?? (args.ship.basic_power_halved ?? false)}
          onChange={() =>
            order("Basic", halfOrdered ?? (args.ship.basic_power_halved ?? false))
          }
        />
        Basic systems at half
        {halfOrdered != null && <span className="power-queued-note">ordered</span>}
      </label>
    </div>
  );
}

/**
 * One system as a lit column.
 *
 * Height is its draw against the biggest number on the board, and colour says
 * what it is doing: running, shut down, or about to be switched this round.
 */
function PowerColumn(args: {line: PowerLine; spare: number; queued: boolean; onToggle: () => void}) {
  const {line} = args;
  // The jump drive draws nothing until the ship jumps, so it has no share to
  // show. What the engineer needs from its column is whether the spare power
  // would cover it -- an empty tube next to "78 spare" read as a fault.
  const standby = line.onDemand;
  const covered = standby && args.spare >= line.draw;
  const share = standby
    ? covered
      ? 1
      : Math.min(1, args.spare / Math.max(1, line.draw))
    : line.draw === 0
      ? 1
      : Math.min(1, line.received / line.draw);
  const height = Math.max(2, Math.round(share * 100));
  const running = isPowered(line);
  const state = args.queued
    ? "queued"
    : !line.online
      ? "dark"
      : standby
        ? covered
          ? "standby"
          : "starved"
        : running
          ? "live"
          : "starved";
  const title = [
    `${line.label}: needs ${line.draw} Power`,
    !line.online
      ? "shut down"
      : standby
        ? covered
          ? `on standby — ${args.spare} spare covers it`
          : `on standby — only ${args.spare} spare, ${line.draw - args.spare} short`
        : running
          ? line.received < line.draw
            ? `running on ${line.received}`
            : "running"
          : `starved -- getting ${line.received} of ${line.draw}`,
    args.queued ? "— order queued this round" : "",
    line.switchable ? "" : "— cannot be shut down",
  ]
    .filter((part) => part !== "")
    .join(" ");

  return (
    <div className={`power-column power-column-${state}`} title={title}>
      <div className="power-column-track">
        <div className="power-column-fill" style={{height: `${height}%`}} />
      </div>
      <span className="power-column-draw">{line.draw}</span>
      <button
        type="button"
        className="power-column-switch"
        disabled={!line.switchable}
        onClick={args.onToggle}>
        {line.online ? "ON" : "OFF"}
      </button>
      <span className="power-column-label">{shortLabel(line.label)}</span>
    </div>
  );
}

/** Column heads are narrow, so the label is cut to what identifies it. */
const shortLabel = (label: string): string =>
  label
    .replace(" systems", "")
    .replace(/\s*\(.*\)$/, "")
    .replace("barbette", "bbt")
    .replace("turret", "trt");

export default PowerBoard;

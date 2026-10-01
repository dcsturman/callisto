import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {
  PowerLine,
  PowerSystem,
  availablePower,
  powerDemand,
  powerLines,
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
  const spare = supply - demand;
  // The tallest thing on the board sets the scale, so a drive that dwarfs
  // everything else does not flatten the rest into nothing.
  const scale = Math.max(supply, ...lines.map((line) => line.draw), 1);

  const order = (system: PowerSystem, online: boolean) => {
    const action: EngineerState = {kind: "SetPower", system, online};
    dispatch(setEngineerAction({shipName: args.ship.name, engineer, action}));
  };

  const queuedFor = (system: PowerSystem): boolean =>
    queued?.kind === "SetPower" && samePowerSystem(queued.system, system);

  return (
    <div className="power-board">
      <div className="power-summary">
        <span className="power-supply">{supply} available</span>
        <span className={spare < 0 ? "power-spare power-short" : "power-spare"}>
          {spare < 0 ? `${-spare} short` : `${spare} spare`}
        </span>
      </div>
      <div className="power-bars">
        {lines.map((line) => (
          <PowerColumn
            key={typeof line.system === "string" ? line.system : `weapon-${line.system.Weapon}`}
            line={line}
            scale={scale}
            queued={queuedFor(line.system)}
            onToggle={() => order(line.system, !line.online)}
          />
        ))}
      </div>
      {/* Basic systems cannot be shut off, but the rules let a desperate crew
          run them at half (High Guard p. 17). It is not an action -- it is a
          switch on the wall -- so it rides with the next order. */}
      <label className="power-half" title="Run life support, gravity and heat at half power">
        <input
          type="checkbox"
          checked={args.ship.basic_power_halved ?? false}
          onChange={() =>
            dispatch(
              setEngineerAction({
                shipName: args.ship.name,
                engineer,
                action: {
                  kind: "SetPower",
                  system: "Basic",
                  online: !(args.ship.basic_power_halved ?? false),
                },
              })
            )
          }
        />
        Basic systems at half
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
function PowerColumn(args: {
  line: PowerLine;
  scale: number;
  queued: boolean;
  onToggle: () => void;
}) {
  const {line} = args;
  const height = Math.max(2, Math.round((line.draw / args.scale) * 100));
  const state = args.queued ? "queued" : line.online ? "live" : "dark";
  const title = [
    `${line.label}: ${line.draw} Power`,
    line.onDemand ? "only while jumping" : line.online ? "running" : "shut down",
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

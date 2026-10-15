import * as React from "react";
import {useMemo} from "react";

import {Ship, stationsDown} from "lib/entities";
import {availablePower, powerLines, powerSpare} from "lib/power";
import {shipWeapons} from "lib/shipDesignTemplates";
import {useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";

/**
 * Whether this ship can jump, and what is stopping it.
 *
 * The captain asks the engineer "can we jump yet", and the answer used to be
 * spread over four panels: fuel in the dossier, the hundred-diameter limit in
 * the 3D view, the astrogation station in the bridge readout, and the power
 * budget here. Each condition is listed with its own answer, so the engineer
 * can say which one to fix.
 */
export function JumpBoard(args: {ship: Ship}) {
  const templates = useAppSelector(templatesSelector);
  const entities = useAppSelector(entitiesSelector);
  const design = templates[args.ship.design];

  const lines = useMemo(
    () =>
      design == null ? [] : powerLines(args.ship, design, shipWeapons(args.ship, templates)),
    [args.ship, design, templates]
  );

  if (design == null || design.jump === 0) {
    return null;
  }

  // A jump burns a tenth of the ship's tonnage per jump number, so a jump-2
  // costs a fifth of the hull (High Guard p. 11). Tonnage, not hull points: a
  // Scout/Courier is 100 tons with 40 hull points, and its jump-2 costs 20.
  const fuelPerJump = Math.floor(design.displacement / 10);
  const fuelNeeded = fuelPerJump * args.ship.current_jump;
  // The ship jumps as far as it is rated for unless the tanks say otherwise.
  const jumpAvailable =
    fuelPerJump === 0
      ? 0
      : Math.min(args.ship.current_jump, Math.floor(args.ship.current_fuel / fuelPerJump));
  const shortJump = jumpAvailable > 0 && jumpAvailable < args.ship.current_jump;
  const jumpDraw = lines.find((line) => line.system === "Jump")?.draw ?? 0;
  const spare = powerSpare(args.ship, lines);

  // The nearest planet's hundred-diameter limit, which is the one that
  // matters: the server lets a ship jump only when every planet is clear.
  const closest = entities.planets
    .map((planet) => ({
      name: planet.name,
      distance: Math.hypot(
        planet.position[0] - args.ship.position[0],
        planet.position[1] - args.ship.position[1],
        planet.position[2] - args.ship.position[2]
      ),
      limit: planet.radius * 200,
    }))
    .sort((a, b) => a.distance / a.limit - b.distance / b.limit)[0];

  const jumpControl =
    args.ship.software_running?.find((software) => software.kind === "JumpControl")?.level ?? 0;

  const stations = stationsDown(args.ship);
  const astrogationOut = stations.some((state) => state.startsWith("Astrogation"));
  const computerOut = stations.some((state) => state.startsWith("Computer"));

  const conditions: {label: string; met: boolean; warn?: boolean; detail: string}[] = [
    {
      label: "Fuel",
      met: jumpAvailable > 0,
      // Short of a full tank is not a refusal: the ship jumps less far, which
      // the engineer should know before the captain picks a destination.
      warn: shortJump,
      detail: shortJump
        ? `${args.ship.current_fuel} aboard — jump-${jumpAvailable} only (jump-${args.ship.current_jump} needs ${fuelNeeded})`
        : `${args.ship.current_fuel} of ${fuelNeeded} for jump-${args.ship.current_jump}`,
    },
    {
      label: "Clear of gravity",
      met: closest == null || closest.distance > closest.limit,
      detail:
        closest == null
          ? "nothing nearby"
          : `${Math.round(closest.distance / 1000).toLocaleString("en-US")} km from ${closest.name}, limit ${Math.round(closest.limit / 1000).toLocaleString("en-US")}`,
    },
    {
      label: "Power",
      met: spare >= jumpDraw,
      detail: `${jumpDraw} needed, ${Math.max(0, spare)} spare of ${availablePower(args.ship)}`,
    },
    {
      // Jump Control plots the jump; without it running there is no jump,
      // whatever the drive is rated for (CRB p. 161).
      label: "Jump Control",
      met: jumpControl > 0,
      warn: jumpControl > 0 && jumpControl < args.ship.current_jump,
      detail:
        jumpControl === 0
          ? "not running"
          : jumpControl < args.ship.current_jump
            ? `/${jumpControl} running — plots jump-${jumpControl} only`
            : `/${jumpControl} running`,
    },
    {
      label: "Astrogation",
      met: !astrogationOut && !computerOut,
      detail: astrogationOut ? "station out" : computerOut ? "computer out" : "plotted",
    },
    {
      label: "Drive",
      met: args.ship.current_jump > 0,
      detail: `jump ${args.ship.current_jump} of ${design.jump}`,
    },
  ];

  const ready = conditions.every((condition) => condition.met);
  const limited = ready && conditions.some((condition) => condition.warn);

  return (
    <div className="jump-board">
      <div
        className={
          limited
            ? "jump-verdict jump-limited"
            : ready
              ? "jump-verdict jump-ready"
              : "jump-verdict jump-not-ready"
        }>
        {limited ? `Ready — jump-${jumpAvailable} only` : ready ? "Ready to jump" : "Cannot jump"}
      </div>
      <ul className="jump-conditions">
        {conditions.map((condition) => (
          <li
            key={condition.label}
            className={
              !condition.met
                ? "jump-condition jump-condition-blocking"
                : condition.warn
                  ? "jump-condition jump-condition-limited"
                  : "jump-condition"
            }>
            <span className="jump-condition-mark" aria-hidden="true">
              {!condition.met ? "✕" : condition.warn ? "⚠" : "✓"}
            </span>
            <span className="jump-condition-label">{condition.label}</span>
            <span className="jump-condition-detail">{condition.detail}</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default JumpBoard;

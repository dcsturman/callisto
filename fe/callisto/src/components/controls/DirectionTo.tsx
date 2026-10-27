import * as React from "react";
import { useState } from "react";
import { Entity, Ship } from "lib/entities";
import { EntitySelector, EntitySelectorType } from "lib/EntitySelector";
import { bearing, burnAlong } from "lib/bearing";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

const FILTER = [EntitySelectorType.Ship, EntitySelectorType.Planet];

/** Fill the Set Accel boxes, for the pilot to check and Set. */
const fillSetAccel = (accel: [number, number, number]) => {
  (["x", "y", "z"] as const).forEach((axis, i) => {
    const input = document.getElementById(`set-accel-input-${axis}`) as HTMLInputElement | null;
    if (input != null) input.value = accel[i].toString();
  });
};

/**
 * Which way something is from this ship, and how far: where it is now,
 * nothing about how either is moving. The navigation computer plots a course
 * to meet a target; this only answers "which way is it?", for a burn straight
 * at it or, more often, straight away. Toward and Away fill Set Accel at full
 * thrust; Set still has to be pressed.
 */
export function DirectionTo(args: { ship: Ship }) {
  const entities = useAppSelector(entitiesSelector);
  const [target, setTarget] = useState<string | null>(null);

  const there = [...entities.ships, ...entities.planets].find((entity) => entity.name === target);
  const toward = there == null ? null : bearing(args.ship.position, there.position);
  // Thrust left after the pilot's own orders, as Set Accel allows.
  const thrust = Math.max(
    0,
    args.ship.current_maneuver - args.ship.dodge_thrust - (args.ship.assist_gunners ? 1 : 0),
  );

  return (
    <div className="direction-to" data-tutorial="direction-to">
      <h2 className="control-form">Direction to</h2>
      <EntitySelector
        filter={FILTER}
        current={target}
        setChoice={(entity: Entity | null) => setTarget(entity?.name ?? null)}
        exclude={args.ship.name}
        observer={args.ship}
        allowUndetected
        aria-label="Direction to"
      />
      {toward != null && (
        <>
          <pre className="plan-accel-text" title="Unit vector from this ship, in the same axes as Set Accel">
            ({toward.direction.map((value) => value.toFixed(2)).join(", ")}) ·{" "}
            {Math.round(toward.metres / 1000).toLocaleString("en-US")} km
          </pre>
          <div className="direction-to-buttons">
            <button
              type="button"
              className="control-input control-button blue-button"
              title={`Fill Set Accel with ${thrust} G straight at ${target}`}
              onClick={() => fillSetAccel(burnAlong(toward.direction, thrust, 1))}>
              Toward
            </button>
            <button
              type="button"
              className="control-input control-button blue-button"
              title={`Fill Set Accel with ${thrust} G straight away from ${target}`}
              onClick={() => fillSetAccel(burnAlong(toward.direction, thrust, -1))}>
              Away
            </button>
          </div>
        </>
      )}
    </div>
  );
}

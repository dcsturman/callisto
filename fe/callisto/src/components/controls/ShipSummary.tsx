import * as React from "react";
import { useAppSelector } from "state/hooks";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { Ship } from "lib/entities";

/**
 * Compact at-a-glance roster of every ship in the current scenario. Lives
 * in its own box above ViewControls (gravity-well / 100-diameter-limit
 * checkboxes). Each line shows: ship name, hull as `current(max)`, and
 * the magnitude of the current acceleration vector in G.
 *
 * Hull max is looked up from the design template — the wire payload only
 * carries `current_hull`. Thrust magnitude is `||plan[0][0]||` where the
 * server already converted m/s² → G in `serverManager.handleEntities`.
 */
export function ShipSummary() {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);

  if (!entities.ships.length) {
    return null;
  }

  const rows = entities.ships
    .slice()
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((ship: Ship) => {
      const maxHull = templates[ship.design]?.hull ?? null;
      const [ax, ay, az] = ship.plan[0][0];
      const thrust = Math.sqrt(ax * ax + ay * ay + az * az);
      return {
        name: ship.name,
        current: ship.current_hull,
        max: maxHull,
        thrust,
      };
    });

  return (
    <div className="ship-summary-window">
      <h2 className="ship-summary-title">Ships</h2>
      <ul className="ship-summary-rows">
        {rows.map((row) => (
          <li key={row.name} className="ship-summary-row">
            <span className="ship-summary-name">{row.name}</span>
            <span className="ship-summary-hull">
              {row.current}
              {row.max !== null ? `(${row.max})` : ""}
            </span>
            <span className="ship-summary-thrust">{row.thrust.toFixed(1)} G</span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default ShipSummary;

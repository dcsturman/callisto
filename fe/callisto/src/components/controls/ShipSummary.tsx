import * as React from "react";
import { useAppSelector } from "state/hooks";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { Ship } from "lib/entities";
import { isUndetected } from "lib/contacts";
import { teamLabelColor } from "lib/teams";

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
  // Gated against the ship the player has actually been given, not whichever
  // ship they happen to have open. A referee in the all-ships view clicks
  // through every ship in turn to give orders, and that should not keep
  // re-blinding the roster; they can see the whole board, so the roster shows
  // it, coloured by team.
  //
  // This is the same rule the 3D view uses, deliberately: the two displays
  // should agree about what this player can see.
  const viewingShipName = useAppSelector((state) => state.user.shipName);
  const observer =
    viewingShipName == null
      ? null
      : entities.ships.find((s) => s.name === viewingShipName) ?? null;

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
        // Hull and thrust are exactly what a sensor contact would tell you --
        // thrust in G is the manoeuvre-drive DM on the detection table -- so a
        // ship you have no contact on shows its presence and nothing else.
        undetected: isUndetected(observer, ship.name),
        team: ship.team,
      };
    });

  return (
    <div className="ship-summary-window">
      <h2 className="ship-summary-title">Ships</h2>
      <ul className="ship-summary-rows">
        {rows.map((row) => (
          <li
            key={row.name}
            className={
              row.undetected
                ? "ship-summary-row ship-summary-row-undetected"
                : "ship-summary-row"
            }>
            <span
              className="ship-summary-name"
              style={{color: teamLabelColor(row.team, {undetected: row.undetected})}}>
              {row.name}
            </span>
            <span className="ship-summary-hull">
              {row.undetected
                ? "\u2014"
                : `${row.current}${row.max !== null ? `(${row.max})` : ""}`}
            </span>
            <span className="ship-summary-thrust">
              {row.undetected ? "\u2014" : `${row.thrust.toFixed(1)} G`}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default ShipSummary;

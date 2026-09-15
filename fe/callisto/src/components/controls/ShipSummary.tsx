import * as React from "react";
import { useAppSelector } from "state/hooks";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { Ship } from "lib/entities";
import { isUndetected } from "lib/contacts";
import { teamLabelColor } from "lib/teams";
import { formatRange, rangeBetween } from "lib/range";

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

  // The plan the pilot is dialling in, if there is one, so the projected range
  // moves while a burn is being chosen rather than only after it is committed.
  // Only ever applied to the observer -- it is the one ship whose intentions
  // this client knows.
  const proposedPlan = useAppSelector((state) => state.ui.proposedPlan);

  // Range is measured *from* somewhere, and in the referee's all-ships view
  // there is no such somewhere. Rather than a column of dashes, the column is
  // dropped and the box stays narrow.
  const showRangeColumn = observer != null;

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
        undetected: isUndetected(observer, ship),
        team: ship.team,
        range:
          observer == null || observer.name === ship.name
            ? null
            : rangeBetween(observer, ship, proposedPlan?.plan),
      };
    });

  return (
    <div className="ship-summary-window">
      <h2 className="ship-summary-title">Ships</h2>
      <ul
        className={
          showRangeColumn
            ? "ship-summary-rows ship-summary-rows-with-range"
            : "ship-summary-rows"
        }>
        {/* Naming the units once here is what lets the values below drop their
            "G" and "km" suffixes, which pays for the extra column. */}
        <li className="ship-summary-row ship-summary-header" aria-hidden="true">
          <span />
          <span className="ship-summary-hull">hull</span>
          <span className="ship-summary-thrust">thr</span>
          {showRangeColumn && (
            <span className="ship-summary-range">range (km)</span>
          )}
        </li>
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
                ? "?"
                : `${row.current}${row.max !== null ? `(${row.max})` : ""}`}
            </span>
            <span className="ship-summary-thrust">
              {row.undetected ? "?" : row.thrust.toFixed(1)}
            </span>
            {showRangeColumn && (
              <span className="ship-summary-range">
                {/* Undetected reads "?" exactly as hull and thrust do; the
                    observer's own row has no range to itself. */}
                {row.undetected
                  ? "?"
                  : row.range == null
                    ? "—"
                    : formatRange(row.range)}
              </span>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

export default ShipSummary;

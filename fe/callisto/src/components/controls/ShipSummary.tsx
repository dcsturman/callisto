import * as React from "react";
import { useAppSelector } from "state/hooks";
import { entitiesSelector, templatesSelector } from "state/serverSlice";
import { Ship } from "lib/entities";
import { isUndetected } from "lib/contacts";
import { teamLabelColor } from "lib/teams";
import { bandName, formatBands, formatRange, rangeBetween } from "lib/range";

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
/**
 * A colour per band, warm as the range closes. Distant is grey: nothing can be
 * held out there, so it reads as absent rather than as a band you are in.
 */
const BAND_COLORS: {[band: string]: string} = {
  Short: "#ff6b6b",
  Medium: "#ffa94d",
  Long: "#ffd43b",
  "Very Long": "#74c0fc",
  Distant: "#909296",
};

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

  // Range is measured from somewhere, and that somewhere is not the same
  // question as who can see what. A referee with a ship open is working that
  // ship's orders and wants its ranges -- how far everything is from the hull
  // they are flying -- while still seeing the whole board, which is why this
  // is separate from `observer` above rather than the same value.
  const computerShipName = useAppSelector((state) => state.ui.computerShipName);
  const rangeFrom =
    observer ??
    (computerShipName == null
      ? null
      : entities.ships.find((s) => s.name === computerShipName) ?? null);

  // The plan the pilot is dialling in, if there is one, so the projected range
  // moves while a burn is being chosen rather than only after it is committed.
  // Only ever applied to the observer -- it is the one ship whose intentions
  // this client knows.
  const proposedPlan = useAppSelector((state) => state.ui.proposedPlan);

  // With no ship of their own and none selected there is no such somewhere.
  // Rather than a column of dashes, the column is dropped and the box stays
  // narrow.
  const showRangeColumn = rangeFrom != null;

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
          rangeFrom == null || rangeFrom.name === ship.name
            ? null
            : rangeBetween(rangeFrom, ship, proposedPlan?.plan),
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
            <>
              <span
                className="ship-summary-range"
                title={`Measured from ${rangeFrom?.name ?? ""}`}>
                {observer == null ? `range from ${rangeFrom?.name ?? ""}` : "range (km)"}
              </span>
              {/* The band is what the rules are written in: a gunner's DM and
                  a weapon's reach are per band, and a pilot dialling a burn
                  wants to know the band it ends in. Both columns move as the
                  burn is dialled, since the projection uses the plan being
                  chosen rather than the one committed. */}
              <span className="ship-summary-band">end of round</span>
            </>
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
              <>
                <span className="ship-summary-range">
                  {/* Undetected reads "?" exactly as hull and thrust do; the
                      observer's own row has no range to itself. */}
                  {row.undetected
                    ? "?"
                    : row.range == null
                      ? "—"
                      : formatRange(row.range)}
                </span>
                <span
                  className="ship-summary-band"
                  // Coloured by the band the round ends in, which is the one
                  // being steered for.
                  style={
                    row.undetected || row.range == null
                      ? undefined
                      : {color: BAND_COLORS[bandName(row.range.next)]}
                  }>
                  {row.undetected ? "?" : row.range == null ? "—" : formatBands(row.range)}
                </span>
              </>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}

export default ShipSummary;

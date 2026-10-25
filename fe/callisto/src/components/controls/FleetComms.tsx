import * as React from "react";
import { useMemo } from "react";
import { AiStatus, Ship } from "lib/entities";
import { CornerPanel } from "lib/CornerPanel";
import { teamLabelColor } from "lib/teams";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";
import { isReferee } from "lib/view";
import { ShipNameTag, WithShipNames } from "components/controls/ShipNameTag";

/** One AI side: its ships, and what their captains last said. */
interface Side {
  key: string;
  label: string;
  team: Ship["team"];
  ships: Ship[];
}

/** How a side's round stands: deciding until every ship has reported. */
const sideStatus = (ships: Ship[]): AiStatus | null => {
  const statuses = ships.map((ship) => ship.ai_report?.status).filter((s): s is AiStatus => s != null);
  if (statuses.length === 0) {
    return null;
  }
  if (statuses.includes("Deciding")) {
    return "Deciding";
  }
  return statuses.includes("NoAnswer") ? "NoAnswer" : "Ready";
};

const STATUS_TEXT: Record<AiStatus, string> = {
  Deciding: "deciding\u2026",
  Ready: "orders given",
  NoAnswer: "no answer this round; keeping last orders",
};

/**
 * Fleet Comms: what the captains of AI-crewed ships are reporting.
 *
 * The referee hears every AI side, so they need not open each ship to know
 * what it is doing or why; a player hears only their own team's. Grouped by
 * side, as the AI decides: the side's plan once, then a line from each
 * captain on her orders this round.
 *
 * The reports travel in the world like standing orders, so the hiding is
 * done here; a server-side picture per player is still to come.
 */
export function FleetComms() {
  const entities = useAppSelector(entitiesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);

  const referee = isReferee(roles, shipName);
  // A fleet commander hears their own team, as a crew does.
  const myTeam = fleetTeam ?? entities.ships.find((ship) => ship.name === shipName)?.team;

  const sides = useMemo(() => {
    const byKey = new Map<string, Side>();
    entities.ships
      .filter((ship) => ship.controller === "AI")
      .filter((ship) => referee || (myTeam != null && ship.team === myTeam))
      .forEach((ship) => {
        const key = ship.team ?? `ship:${ship.name}`;
        const side = byKey.get(key) ?? { key, label: ship.team ?? ship.name, team: ship.team, ships: [] };
        side.ships.push(ship);
        byKey.set(key, side);
      });
    return [...byKey.values()].sort((a, b) => a.label.localeCompare(b.label));
  }, [entities.ships, referee, myTeam]);

  if (sides.length === 0) {
    return null;
  }

  return (
    <CornerPanel id="fleet-comms" title="Fleet Comms" className="computer-window">
      {sides.map((side) => {
          const status = sideStatus(side.ships);
          const plan = side.ships.find((ship) => ship.ai_report?.plan)?.ai_report?.plan;
          const round = Math.max(...side.ships.map((ship) => ship.ai_report?.round ?? 0));
          return (
            <section key={side.key} className="fleet-comms-side">
              <h2 style={{ color: teamLabelColor(side.team) }}>
                {side.team != null ? `${side.label} side` : side.label}
                {status != null && (
                  <span className={`fleet-comms-status fleet-comms-${status.toLowerCase()}`}>
                    {round > 0 ? ` \u2014 round ${round}, ` : " \u2014 "}
                    {STATUS_TEXT[status]}
                  </span>
                )}
              </h2>
              {plan != null && (
                <p className="fleet-comms-plan">
                  <WithShipNames text={plan} ships={entities.ships} />
                </p>
              )}
              {side.ships.map((ship) => (
                <p key={ship.name} className="fleet-comms-line">
                  <ShipNameTag ship={ship} />:{" "}
                  {ship.ai_report?.status === "Deciding" ? (
                    "\u2026"
                  ) : ship.ai_report?.rationale != null ? (
                    <WithShipNames text={ship.ai_report.rationale} ships={entities.ships} />
                  ) : (
                    "no report"
                  )}
                </p>
              ))}
            </section>
          );
        })}
    </CornerPanel>
  );
}

import * as React from "react";
import { useEffect, useMemo, useState } from "react";
import { CornerPanel } from "lib/CornerPanel";
import { Ship } from "lib/entities";
import { isReferee } from "lib/view";
import { OrdersScope, setOrders } from "lib/serverManager";
import { squadronShips, teamSquadrons } from "lib/squadrons";
import { TEAMS, TEAM_CSS, Team } from "lib/teams";
import { useAppSelector } from "state/hooks";
import { entitiesSelector } from "state/serverSlice";

/** One thing orders can be given to: the fleet, a squadron or a ship, and
 *  the ship the order is sent through. */
interface Target {
  key: string;
  label: string;
  scope: OrdersScope;
  through: string;
  current: string;
}

const textOf = (orders: string | undefined) => (orders ?? "").trim();

/** Everything a commander of `ships` may give orders to, widest first. */
const targetsFor = (ships: Ship[], team: string | null, fleet: boolean): Target[] => {
  if (ships.length === 0) {
    return [];
  }
  const fleetTarget: Target[] = fleet
    ? [
        {
          key: "fleet",
          label: "Fleet",
          scope: "Fleet",
          through: ships[0].name,
          current: textOf(ships.find((ship) => ship.fleet_orders)?.fleet_orders),
        },
      ]
    : [];
  const squadrons: Target[] = teamSquadrons(ships, team).map((squadron) => {
    const members = ships.filter((ship) => squadron.ships.includes(ship.name));
    return {
      key: `squadron:${squadron.name}`,
      label: `Squadron ${squadron.name}`,
      scope: "Squadron",
      through: squadron.ships[0],
      current: textOf(members.find((ship) => ship.squadron_orders)?.squadron_orders),
    };
  });
  const each: Target[] = [...ships]
    .sort((a, b) => a.name.localeCompare(b.name))
    .map((ship) => ({
      key: `ship:${ship.name}`,
      label: ship.name,
      scope: "Ship",
      through: ship.name,
      current: textOf(ship.orders),
    }));
  return [...fleetTarget, ...squadrons, ...each];
};

/**
 * Standing orders, for a commander to give their fleet, a squadron or a ship
 * in words. An AI fleet commander reads them -- the fleet's, each
 * squadron's and each ship's own -- and turns them into its squadrons'
 * scripts; Fleet Comms shows what it made of them.
 *
 * The referee gives any team's orders, a fleet commander their own team's,
 * a squadron commander their squadron's and its ships'.
 */
export function OrdersPanel() {
  const entities = useAppSelector(entitiesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const shipName = useAppSelector((state) => state.user.shipName);
  const fleetTeam = useAppSelector((state) => state.user.fleetTeam);
  const squadron = useAppSelector((state) => state.user.squadron);
  const referee = isReferee(roles, shipName);

  const teams = useMemo(
    () => TEAMS.filter((team) => entities.ships.some((ship) => ship.team === team)),
    [entities.ships],
  );
  const [chosenTeam, setChosenTeam] = useState<Team | null>(null);
  const team: string | null =
    fleetTeam ??
    squadron?.team ??
    (chosenTeam != null && teams.includes(chosenTeam) ? chosenTeam : (teams[0] ?? null));

  const targets = useMemo(() => {
    if (squadron != null) {
      const members = squadronShips(entities.ships, squadron.team, squadron.name);
      return targetsFor(
        entities.ships.filter((ship) => members.includes(ship.name)),
        squadron.team,
        false,
      );
    }
    return targetsFor(
      entities.ships.filter((ship) => (ship.team ?? null) === team),
      team,
      true,
    );
  }, [entities.ships, squadron, team]);

  const [chosenKey, setChosenKey] = useState<string | null>(null);
  const target = targets.find((t) => t.key === chosenKey) ?? targets[0];
  const [draft, setDraft] = useState(target?.current ?? "");
  // A new target, or its orders changed by someone else: show them as they stand.
  useEffect(() => setDraft(target?.current ?? ""), [target?.key, target?.current]);

  if (!(referee || fleetTeam != null || squadron != null) || targets.length === 0 || target == null) {
    return null;
  }

  const send = () => setOrders(target.through, draft, target.scope);
  const inForce = targets.filter((t) => t.current !== "");

  return (
    <CornerPanel id="orders-panel" title="Orders" className="computer-window" tutorial="orders-panel">
      {referee && fleetTeam == null && squadron == null && teams.length > 1 && (
        <select
          className="team-select"
          aria-label="Team"
          value={team ?? ""}
          style={{ color: team ? TEAM_CSS[team as Team] : undefined }}
          onChange={(e) => {
            setChosenTeam(e.target.value as Team);
            setChosenKey(null);
          }}
        >
          {teams.map((name) => (
            <option key={name} value={name}>
              {name}
            </option>
          ))}
        </select>
      )}
      <div className="orders-panel-to">
        <span>To</span>
        <select
          className="team-select"
          aria-label="Give orders to"
          value={target.key}
          onChange={(e) => setChosenKey(e.target.value)}
        >
          {targets.map((t) => (
            <option key={t.key} value={t.key}>
              {t.label}
              {t.current !== "" ? " ✓" : ""}
            </option>
          ))}
        </select>
      </div>
      <textarea
        className="ship-command-orders-text orders-panel-text"
        aria-label="Orders"
        rows={3}
        placeholder={
          target.scope === "Fleet"
            ? "For example: hold the gate; fighters keep theirs off the cruisers."
            : target.scope === "Squadron"
              ? "For example: close to Medium by round 6, then engage their escorts."
              : "For example: stay by the flagship."
        }
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
            send();
          }
        }}
      />
      <div className="orders-panel-buttons">
        <button type="button" className="blue-button orders-panel-give" disabled={draft.trim() === target.current} onClick={send}>
          Give orders
        </button>
      </div>
      {inForce.length > 0 && (
        <ul className="orders-panel-list">
          {inForce.map((t) => (
            <li key={t.key}>
              <button type="button" className="squadrons-link" onClick={() => setChosenKey(t.key)}>
                {t.label}
              </button>
              : {t.current}{" "}
              <button
                type="button"
                className="squadrons-link"
                aria-label={`Withdraw the orders to ${t.label}`}
                onClick={() => setOrders(t.through, "", t.scope)}
              >
                Withdraw
              </button>
            </li>
          ))}
        </ul>
      )}
    </CornerPanel>
  );
}

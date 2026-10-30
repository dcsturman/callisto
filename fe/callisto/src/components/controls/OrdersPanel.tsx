import * as React from "react";
import { useEffect, useMemo, useState } from "react";
import { CornerPanel } from "lib/CornerPanel";
import { Ship } from "lib/entities";
import { isReferee } from "lib/view";
import { OrdersScope, setOrders, setShipController } from "lib/serverManager";
import { squadronShips, teamSquadrons } from "lib/squadrons";
import { aboardSummary } from "lib/carriers";
import { TEAMS, TEAM_CSS, Team } from "lib/teams";
import { useAppSelector } from "state/hooks";
import { aiCrewsSelector, entitiesSelector } from "state/serverSlice";

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
      // A carrier's craft are not ships of the list, so she says how many
      // she carries: "Sirocco (80 Light Fighter aboard)".
      label: aboardSummary(ship) == null ? ship.name : `${ship.name} (${aboardSummary(ship)})`,
      scope: "Ship",
      through: ship.name,
      current: textOf(ship.orders),
    }));
  return [...fleetTarget, ...squadrons, ...each];
};

/** Orders in force, ships given the same orders read as one line. */
interface InForce {
  label: string;
  current: string;
  targets: Target[];
}

/** "Spear 01", "Spear 01, Spear 02", or "Spear 01, Spear 02 and 13 more". */
const shipsLabel = (names: string[]) =>
  names.length <= 3 ? names.join(", ") : `${names.slice(0, 2).join(", ")} and ${names.length - 2} more`;

const groupInForce = (targets: Target[]): InForce[] => {
  const groups: InForce[] = [];
  targets.forEach((t) => {
    const same = t.scope === "Ship" && groups.find((g) => g.targets[0].scope === "Ship" && g.current === t.current);
    if (same) {
      same.targets.push(t);
      same.label = shipsLabel(same.targets.map((member) => member.label));
    } else {
      groups.push({ label: t.label, current: t.current, targets: [t] });
    }
  });
  return groups;
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
  const aiCrews = useAppSelector(aiCrewsSelector);

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
  // A squadron picked in the roster or the 3D view is the one to give orders
  // to: with a hundred ships the dropdown is no way to find it.
  const focus = useAppSelector((state) => state.ui.focusSquadron);
  useEffect(() => {
    if (focus == null) {
      return;
    }
    if (referee && fleetTeam == null && squadron == null && focus.team != null) {
      setChosenTeam(focus.team as Team);
    }
    setChosenKey(`squadron:${focus.name}`);
  }, [focus, referee, fleetTeam, squadron]);
  const target = targets.find((t) => t.key === chosenKey) ?? targets[0];
  const [draft, setDraft] = useState(target?.current ?? "");
  // A new target, or its orders changed by someone else: show them as they stand.
  useEffect(() => setDraft(target?.current ?? ""), [target?.key, target?.current]);

  if (!(referee || fleetTeam != null || squadron != null) || targets.length === 0 || target == null) {
    return null;
  }

  const send = () => setOrders(target.through, draft, target.scope);
  // Only an AI-crewed ship reads orders: say so when some here are crewed by
  // people, and offer to hand them over.
  const commanded = targets.filter((t) => t.scope === "Ship").map((t) => t.through);
  const peopled = entities.ships.filter((ship) => commanded.includes(ship.name) && ship.controller !== "AI");
  const inForce = groupInForce(targets.filter((t) => t.current !== ""));

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
      {peopled.length > 0 && (
        <p className="orders-panel-unread">
          {peopled.length === commanded.length
            ? "These ships are crewed by people, so nothing reads these orders. "
            : `${shipsLabel(peopled.map((ship) => ship.name))} ${peopled.length === 1 ? "is" : "are"} crewed by people and will not read them. `}
          {aiCrews && (
            <button
              type="button"
              className="squadrons-link"
              onClick={() => peopled.forEach((ship) => setShipController(ship.name, "AI"))}
            >
              Hand {peopled.length === 1 ? "her" : "them"} to the AI
            </button>
          )}
        </p>
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
          {inForce.map((group) => (
            <li key={group.targets[0].key} title={group.targets.map((t) => t.label).join(", ")}>
              <button type="button" className="squadrons-link" onClick={() => setChosenKey(group.targets[0].key)}>
                {group.label}
              </button>
              : {group.current}{" "}
              <button
                type="button"
                className="squadrons-link"
                aria-label={`Withdraw the orders to ${group.label}`}
                onClick={() => group.targets.forEach((t) => setOrders(t.through, "", t.scope))}
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

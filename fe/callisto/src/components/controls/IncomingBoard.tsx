import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {vectorDistance} from "lib/Util";
import {roundsToImpact} from "lib/missileEta";
import {useAppSelector} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";

/**
 * Salvoes in flight at this ship -- how many, from whom, and how soon -- and
 * her own, in flight at others: how many at whom, and when they land.
 *
 * The guns that could fire at us are on the targets board above, beside the
 * ship they belong to, since "what can that ship do to me" is one question
 * and not two. This is the part that is already in the air.
 */
export function IncomingBoard(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);

  const inbound = useMemo(
    () =>
      entities.missiles
        .filter((missile) => missile.target === args.ship.name)
        .map((missile) => ({
          name: missile.name,
          source: missile.source,
          distance: vectorDistance(missile.position, args.ship.position),
          // Thrust and all: see roundsToImpact. A salvo that will not arrive
          // within its horizon sorts last.
          rounds: roundsToImpact(missile, args.ship) ?? Infinity,
        })),
    [entities.missiles, args.ship]
  );

  // Ours in the air: by target, then by the round they land.
  const outbound = useMemo(() => {
    const groups = new Map<string, {target: string; rounds: number; count: number}>();
    entities.missiles
      .filter((missile) => missile.source === args.ship.name)
      .forEach((missile) => {
        const target = entities.ships.find((ship) => ship.name === missile.target);
        const rounds = target == null ? Infinity : (roundsToImpact(missile, target) ?? Infinity);
        const key = `${missile.target}\u0000${rounds}`;
        const group = groups.get(key) ?? {target: missile.target, rounds, count: 0};
        group.count += 1;
        groups.set(key, group);
      });
    return [...groups.values()].sort((a, b) => a.rounds - b.rounds || a.target.localeCompare(b.target));
  }, [entities.missiles, entities.ships, args.ship.name]);

  const byRound = useMemo(() => {
    const groups = new Map<number, {count: number; sources: Set<string>}>();
    for (const missile of inbound) {
      const group = groups.get(missile.rounds) ?? {count: 0, sources: new Set<string>()};
      group.count += 1;
      group.sources.add(missile.source);
      groups.set(missile.rounds, group);
    }
    return [...groups.entries()].sort((a, b) => a[0] - b[0]);
  }, [inbound]);

  return (
    <div className="incoming-board" data-tutorial="incoming">
      <h3 className="incoming-heading">Salvoes inbound</h3>
      {inbound.length === 0 ? (
        <p className="sensor-empty">Nothing inbound.</p>
      ) : (
        <ul className="incoming-rows">
          {byRound.map(([rounds, group]) => (
            <li key={rounds} className="incoming-row">
              <span className="incoming-count">{group.count}</span>
              <span className="incoming-from">from {[...group.sources].join(", ")}</span>
              <span className={rounds <= 1 ? "incoming-eta incoming-now" : "incoming-eta"}>
                {rounds <= 1 ? "this round" : rounds === Infinity ? "not closing" : `${rounds} rounds`}
              </span>
            </li>
          ))}
        </ul>
      )}
      {outbound.length > 0 && (
        <>
          <h3 className="incoming-heading">Ours in flight</h3>
          <ul className="incoming-rows">
            {outbound.map((group) => (
              <li key={`${group.target}-${group.rounds}`} className="incoming-row">
                <span className="incoming-count">{group.count}</span>
                <span className="incoming-from">at {group.target}</span>
                <span className="incoming-eta">
                  {group.rounds <= 1 ? "this round" : group.rounds === Infinity ? "not closing" : `${group.rounds} rounds`}
                </span>
              </li>
            ))}
          </ul>
        </>
      )}
    </div>
  );
}

export default IncomingBoard;

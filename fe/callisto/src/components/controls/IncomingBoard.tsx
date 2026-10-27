import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {vectorDistance} from "lib/Util";
import {roundsToImpact} from "lib/missileEta";
import {useAppSelector} from "state/hooks";
import {entitiesSelector} from "state/serverSlice";

/**
 * Salvoes in flight at this ship: how many, from whom, and how soon.
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
      <h3 className="incoming-heading">Salvoes</h3>
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
      {/* Who can shoot at us from where they are, and with what. */}
    </div>
  );
}

export default IncomingBoard;

import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {vectorDistance} from "lib/Util";
import {TURN_IN_SECONDS} from "lib/universal";
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
        .map((missile) => {
          const distance = vectorDistance(missile.position, args.ship.position);
          // Closing speed along the line of flight is what decides how long
          // there is; a missile under thrust arrives sooner than this, so
          // this is the kind estimate.
          const closing = Math.max(
            1,
            vectorDistance(missile.velocity, args.ship.velocity)
          );
          return {
            name: missile.name,
            source: missile.source,
            distance,
            rounds: Math.max(1, Math.ceil(distance / (closing * TURN_IN_SECONDS))),
          };
        }),
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
    <div className="incoming-board">
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
                {rounds <= 1 ? "this round" : `${rounds} rounds`}
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

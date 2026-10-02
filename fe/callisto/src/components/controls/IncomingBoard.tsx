import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {vectorDistance} from "lib/Util";
import {TURN_IN_SECONDS} from "lib/universal";
import {useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";
import {shipWeapons} from "lib/shipDesignTemplates";
import {weaponGuns} from "lib/weapon";

/**
 * What is coming at this ship, and what can be done about it.
 *
 * The gunner works point defence and sand but had no way to see the thing
 * they are defending against: how many missiles are inbound, how soon, and
 * whether the guns and barrels aboard can plausibly stop them. "Five
 * inbound, we can take three" is the decision, and nothing on screen said it.
 */
export function IncomingBoard(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);

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

  const defences = useMemo(() => {
    const weapons = shipWeapons(args.ship, templates);
    let pointDefence = 0;
    let sandcasters = 0;
    weapons.forEach((weapon, index) => {
      const kinds = weaponGuns(weapon).map((gun) => gun.kind);
      // Lasers only for point defence (High Guard p. 30), and a mount nobody
      // is at does nothing at all.
      const manned = (args.ship.crew?.gunnery?.length ?? 0) > index;
      if (!manned) {
        return;
      }
      if (kinds.some((kind) => kind === "Beam" || kind === "Pulse" || kind === "PointDefense")) {
        pointDefence += 1;
      }
      if (kinds.includes("Sand")) {
        sandcasters += 1;
      }
    });
    return {pointDefence, sandcasters};
  }, [args.ship, templates]);

  const barrels = args.ship.magazine?.sand ?? 0;
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
      {/* What is available to meet it. Point defence intercepts; sand only
          softens a hit, and each cloud costs a barrel. */}
      <div className="incoming-defences">
        <span title="Laser mounts that could be put on point defence, with someone at them">
          {defences.pointDefence} PD mount{defences.pointDefence === 1 ? "" : "s"}
        </span>
        <span title="Sandcasters crewed, and barrels left to throw">
          {defences.sandcasters} caster{defences.sandcasters === 1 ? "" : "s"} · {barrels} barrel
          {barrels === 1 ? "" : "s"}
        </span>
      </div>
    </div>
  );
}

export default IncomingBoard;

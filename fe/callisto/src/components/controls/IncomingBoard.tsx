import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {isUndetected} from "lib/contacts";
import {Band, RANGE_MOD, WEAPON_HIT_MOD, reaches} from "lib/gunnery";
import {bandName, rangeBetween} from "lib/range";
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

  // What we are doing about being shot at, which applies to every attacker.
  const ourDodge = args.ship.dodge_thrust > 0 ? -(args.ship.crew?.pilot ?? 0) : 0;
  const ourEvade = -(args.ship.software_running?.find((s) => s.kind === "Evade")?.level ?? 0);

  /**
   * What the other side can throw at us from where it is standing.
   *
   * Missiles in flight are only half the threat, and the quieter half: a
   * ship at Short range with three pulse turrets is the thing that actually
   * kills you this round. Counts only mounts that reach us at the band they
   * are at now, since a laser out of its range is not a threat at all.
   */
  const threats = useMemo(
    () =>
      entities.ships
        .filter((other) => other.name !== args.ship.name)
        .filter((other) => other.team == null || other.team !== args.ship.team)
        .filter((other) => !isUndetected(args.ship, other))
        .map((other) => {
          const band = bandName(rangeBetween(args.ship, other).now) as Band;
          const counts = new Map<string, number>();
          for (const weapon of shipWeapons(other, templates)) {
            if (!reaches(weapon, band)) {
              continue;
            }
            for (const gun of weaponGuns(weapon)) {
              // Sand is defensive; it is not pointed at us.
              if (gun.kind === "Sand") {
                continue;
              }
              counts.set(gun.kind, (counts.get(gun.kind) ?? 0) + 1);
            }
          }

          // Everything about their shot that we can know: the weapon, the
          // range, whether they hold a lock on us, and what we are doing
          // about it. Their gunner's skill is theirs, and is not in here.
          const theirLock = other.sensor_locks?.includes(args.ship.name) ?? false;
          const guns = [...counts.entries()].map(([kind, count]) => {
            const salvo = kind === "Missile" || kind === "Torpedo";
            const terms: [string, number][] = [
              ["weapon", WEAPON_HIT_MOD[kind] ?? 0],
              [salvo ? "range (salvo, so none)" : `range (${band})`, salvo ? 0 : RANGE_MOD[band]],
              ["their sensor lock", theirLock ? 2 : 0],
              ["our pilot evading", ourDodge],
              ["our Evade software", ourEvade],
            ];
            const dm = terms.reduce((total, [, value]) => total + value, 0);
            const named = terms.filter(([, value]) => value !== 0);
            return {
              kind,
              count,
              dm,
              why: `${
                named.length === 0
                  ? "No modifiers"
                  : named.map(([name, value]) => `${name} ${value >= 0 ? "+" : ""}${value}`).join(", ")
              } — their gunner's own skill is not known to us.`,
            };
          });

          return {name: other.name, band, guns};
        })
        .filter((threat) => threat.guns.length > 0),
    [entities.ships, args.ship, templates, ourDodge, ourEvade]
  );

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
      {threats.length > 0 && <h3 className="incoming-heading">Guns that reach us</h3>}
      {threats.length > 0 && (
        <ul className="threat-rows">
          {threats.map((threat) => (
            <li key={threat.name} className="threat-row">
              <span className="threat-name">{threat.name}</span>
              <span className="threat-band">{threat.band}</span>
              <span className="threat-guns">
                {threat.guns.map((gun, index) => (
                  <span key={gun.kind} title={gun.why}>
                    {index > 0 ? ", " : ""}
                    {gun.count}× {gun.kind.replace(/([a-z])([A-Z])/g, "$1 $2")}{" "}
                    <span className="threat-dm">
                      {gun.dm >= 0 ? "+" : ""}
                      {gun.dm}
                    </span>
                  </span>
                ))}
              </span>
            </li>
          ))}
        </ul>
      )}
      {/* What is available to meet it. Point defence intercepts; sand only
          softens a hit, and each cloud costs a barrel. */}
      <h3 className="incoming-heading">Ours to answer with</h3>
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

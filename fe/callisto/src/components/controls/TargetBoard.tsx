import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {isUndetected} from "lib/contacts";
import {Band, RANGE_MOD, reaches} from "lib/gunnery";
import {bandName, rangeBetween} from "lib/range";
import {shipWeapons} from "lib/shipDesignTemplates";
import {weaponToString} from "lib/weapon";
import {WeaponGlyph} from "components/controls/WeaponGlyph";
import {useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";

/**
 * What the gunner needs before pressing a button.
 *
 * Everything here existed somewhere else: the range in the 3D view, the
 * target's armour in its dossier, whether a mount even reaches in the rules
 * tables, the lock on the sensop's card. A gunner choosing a shot had to
 * assemble it from four places, so this assembles it in one -- per target,
 * how far, which way it is going, what the range does to the roll, and which
 * of our mounts can touch it.
 */
export function TargetBoard(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const proposedPlan = useAppSelector((state) => state.ui.proposedPlan);

  const weapons = useMemo(() => shipWeapons(args.ship, templates), [args.ship, templates]);

  const rows = useMemo(
    () =>
      entities.ships
        .filter((other) => other.name !== args.ship.name)
        // Nothing can be fired at what has not been found.
        .filter((other) => !isUndetected(args.ship, other))
        .map((other) => {
          const range = rangeBetween(args.ship, other, proposedPlan?.plan);
          const band = bandName(range.now) as Band;
          const endBand = bandName(range.next) as Band;
          const design = templates[other.design];
          return {
            name: other.name,
            team: other.team,
            metres: range.now,
            band,
            endBand,
            closing: range.next < range.now,
            locked: args.ship.sensor_locks?.includes(other.name) ?? false,
            armour: other.current_armor,
            screens: design?.screens?.length ?? 0,
          };
        })
        .sort((a, b) => a.metres - b.metres),
    [entities.ships, args.ship, templates, proposedPlan]
  );

  if (rows.length === 0) {
    return <p className="sensor-empty">No contacts.</p>;
  }

  return (
    <ul className="target-rows">
      {rows.map((row) => (
        <li key={row.name} className="target-row">
          <div className="target-head">
            <span className="target-name">{row.name}</span>
            {row.locked && (
              <span className="target-lock" title="Sensor lock: DM+2 to attacks on this ship">
                lock
              </span>
            )}
            <span className="target-range" title={`${Math.round(row.metres).toLocaleString("en-US")} m`}>
              {Math.round(row.metres / 1000).toLocaleString("en-US")} km
            </span>
          </div>
          <div className="target-detail">
            <span className={`target-band target-band-${row.band.replace(" ", "-").toLowerCase()}`}>
              {row.band}
            </span>
            <span className="target-dm" title="What the range does to an attack roll">
              DM {RANGE_MOD[row.band] >= 0 ? "+" : ""}
              {RANGE_MOD[row.band]}
            </span>
            {/* Where it will be when the round ends: a shot is ordered now
                and resolved then, and a target that is opening may be out of
                a mount's reach by the time it fires. */}
            <span
              className="target-trend"
              title={row.closing ? "Closing" : "Opening"}>
              {row.closing ? "▼" : "▲"} {row.endBand}
            </span>
            <span className="target-defence" title="Armour, and screens fitted">
              armour {row.armour}
              {row.screens > 0 ? ` · ${row.screens} screen${row.screens === 1 ? "" : "s"}` : ""}
            </span>
          </div>
          {/* Which of our mounts can touch it, at the band it is in now.
              A launcher always reaches: the salvo flies to the target. */}
          <div className="target-reach">
            {weapons.map((weapon, index) => {
              const can = reaches(weapon, row.band);
              return (
                <span
                  key={index}
                  className={can ? "target-reach-yes" : "target-reach-no"}
                  title={`${weaponToString(weapon)}: ${can ? "in reach" : `out of reach at ${row.band}`}`}>
                  <WeaponGlyph weapon={weapon} />
                </span>
              );
            })}
          </div>
        </li>
      ))}
    </ul>
  );
}

export default TargetBoard;

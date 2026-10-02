import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {isUndetected} from "lib/contacts";
import {Band, RANGE_MOD, WEAPON_HIT_MOD, reaches} from "lib/gunnery";
import {bandName, rangeBetween} from "lib/range";
import {shipWeapons} from "lib/shipDesignTemplates";
import {weaponGuns, weaponToString} from "lib/weapon";
import {WeaponGlyph} from "components/controls/WeaponGlyph";
import {useAppDispatch, useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";
import {setShowAlliesOnTargets} from "state/uiSlice";

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
  // Our own squadron is not what a gunner is looking at, so it is off by
  // default and one tick away when a referee wants the whole picture.
  const showAllies = useAppSelector((state) => state.ui.showAlliesOnTargets ?? false);
  const dispatch = useAppDispatch();

  const weapons = useMemo(() => shipWeapons(args.ship, templates), [args.ship, templates]);

  // What we are doing about being shot at, which applies to every attacker.
  const ourDodge = args.ship.dodge_thrust > 0 ? -(args.ship.crew?.pilot ?? 0) : 0;
  const ourEvade = -(args.ship.software_running?.find((s) => s.kind === "Evade")?.level ?? 0);

  const rows = useMemo(
    () =>
      entities.ships
        .filter((other) => other.name !== args.ship.name)
        .filter(
          (other) =>
            showAllies || args.ship.team == null || other.team !== args.ship.team
        )
        // Nothing can be fired at what has not been found.
        .filter((other) => !isUndetected(args.ship, other))
        .map((other) => {
          const range = rangeBetween(args.ship, other, proposedPlan?.plan);
          const band = bandName(range.now) as Band;
          const endBand = bandName(range.next) as Band;
          const design = templates[other.design];

          // Their armament, and what each kind of gun would roll against us
          // from where they are. Their gunner's own skill is theirs.
          const theirLock = other.sensor_locks?.includes(args.ship.name) ?? false;
          const theirGuns = shipWeapons(other, templates)
            .filter((weapon) => reaches(weapon, band))
            .map((weapon) => {
              const kinds = weaponGuns(weapon).map((gun) => gun.kind).filter((kind) => kind !== "Sand");
              const kind = kinds[0] ?? "";
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
                weapon,
                empty: kinds.length === 0,
                dm,
                why: `${weaponToString(weapon)} against us: ${
                  named.length === 0
                    ? "no modifiers"
                    : named.map(([name, value]) => `${name} ${value >= 0 ? "+" : ""}${value}`).join(", ")
                } — their gunner's own skill is not known to us.`,
              };
            })
            .filter((gun) => !gun.empty);

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
            theirGuns,
          };
        })
        .sort((a, b) => a.metres - b.metres),
    [entities.ships, args.ship, templates, proposedPlan, showAllies, ourDodge, ourEvade]
  );

  const allyToggle = (
    <label className="target-allies" title="Show the ships on our own side as well">
      <input
        type="checkbox"
        checked={showAllies}
        onChange={(event) => dispatch(setShowAlliesOnTargets(event.target.checked))}
      />
      allies
    </label>
  );

  if (rows.length === 0) {
    return (
      <>
        <p className="sensor-empty">{showAllies ? "No contacts." : "No hostile contacts."}</p>
        {allyToggle}
      </>
    );
  }

  return (
    <>
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
            {/* Ours, at the end of the line: which mounts can touch it at
                the band it is in now. A launcher always reaches, since the
                salvo flies to the target. */}
            <span className="target-reach">
              {weapons.map((weapon, index) => {
                const can = reaches(weapon, row.band);
                return (
                  <span
                    key={index}
                    className={can ? "target-reach-yes" : "target-reach-no"}
                    title={`Ours: ${weaponToString(weapon)} — ${
                      can ? "in reach" : `out of reach at ${row.band}`
                    }`}>
                    <WeaponGlyph weapon={weapon} />
                  </span>
                );
              })}
            </span>
          </div>
          {/* Theirs: what can reach us from there, and what it would roll.
              Drawn the way Contact Detail draws armament, so a turret means
              the same thing on both cards. */}
          {row.theirGuns.length > 0 && (
            <div className="target-theirs" title="What they can shoot back with from this range">
              {row.theirGuns.map((gun, index) => (
                <span key={index} className="target-their-gun" title={gun.why}>
                  <WeaponGlyph weapon={gun.weapon} />
                  <span className="target-their-dm">
                    {gun.dm >= 0 ? "+" : ""}
                    {gun.dm}
                  </span>
                </span>
              ))}
            </div>
          )}
        </li>
      ))}
    </ul>
    {allyToggle}
    </>
  );
}

export default TargetBoard;

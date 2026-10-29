import * as React from "react";
import {useMemo, useState} from "react";

import {Ship} from "lib/entities";
import {isUndetected} from "lib/contacts";
import {Band, RANGE_MOD, WEAPON_HIT_MOD, reaches} from "lib/gunnery";
import {bandName, rangeBetween} from "lib/range";
import {shipWeapons} from "lib/shipDesignTemplates";
import {spinalOf, weaponToString, weaponGuns} from "lib/weapon";
import {shortBandOf, spinalRangeDm, spinalSizeDm} from "lib/spinal";
import {useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";

/** Chance that 2D plus `dm` reaches 8, as a percentage. */
const chanceOf = (dm: number): number => {
  // 2D outcomes, 2..12, out of 36.
  const ways = [0, 0, 1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];
  let hits = 0;
  for (let roll = 2; roll <= 12; roll++) {
    if (roll + dm >= 8) {
      hits += ways[roll];
    }
  }
  return Math.round((hits / 36) * 100);
};


/**
 * What each mount would roll against a chosen target, and how often that
 * lands.
 *
 * Assembles the same terms the server does -- gunner, weapon, range, lock,
 * the target's evasion -- so a gunner can tell a hopeless shot from a good
 * one before spending the mount on it. Hidden by default: some tables would
 * rather the dice stayed mysterious.
 */
export function ShotOdds(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const [shown, setShown] = useState(false);
  const [targetName, setTargetName] = useState<string | null>(null);

  const targets = useMemo(
    () =>
      entities.ships.filter(
        (other) => other.name !== args.ship.name && !isUndetected(args.ship, other)
      ),
    [entities.ships, args.ship]
  );

  const target = useMemo(
    () => targets.find((other) => other.name === targetName) ?? targets[0] ?? null,
    [targets, targetName]
  );

  const rows = useMemo(() => {
    if (target == null) {
      return [];
    }
    const distance = rangeBetween(args.ship, target).now;
    const band = bandName(distance) as Band;
    const ourTons = templates[args.ship.design]?.displacement ?? 0;
    const theirTons = templates[target.design]?.displacement ?? 0;
    const locked = args.ship.sensor_locks?.includes(target.name) ?? false;
    // The defender's evasion: the pilot's dodge order, and Evade software,
    // which both subtract from every attack.
    const dodge = target.dodge_thrust > 0 ? -(target.crew?.pilot ?? 0) : 0;
    const evadeSoftware = -(target.software_running?.find((s) => s.kind === "Evade")?.level ?? 0);
    const advanced = args.ship.software_running?.find((s) => s.kind === "AdvancedFireControl")?.level ?? 0;
    // "For now assume all missiles are smart missiles": the guidance bonus is
    // our TL over theirs, and never less than 1 nor more than 6.
    const ourTl = templates[args.ship.design]?.tl ?? 0;
    const theirTl = templates[target.design]?.tl ?? 0;
    const smartMissile = Math.min(6, Math.max(1, ourTl - theirTl));
    const launchSolution = args.ship.software_running?.find((s) => s.kind === "LaunchSolution")?.level ?? 0;

    return shipWeapons(args.ship, templates).map((weapon, index) => {
      const kinds = weaponGuns(weapon).map((gun) => gun.kind);
      const kind = kinds[0] ?? "";
      const salvo = kind === "Missile" || kind === "Torpedo";
      const gunner = args.ship.crew?.gunnery?.[index] ?? 0;
      const manned = (args.ship.crew?.gunnery?.length ?? 0) > index;
      const inReach = reaches(weapon, band);
      // A spinal's size and close-range DMs, and the ships it cannot attack
      // at all (High Guard pp. 34-35); a bay's small-target DM (p. 31).
      const spinal = spinalOf(weapon.mount);
      const sizeDm = spinalSizeDm(theirTons);
      const tooSmall = spinal != null && sizeDm == null && !target.held_still;
      const bay = typeof weapon.mount === "object" && "Bay" in weapon.mount;
      const smallTarget =
        spinal != null ? (sizeDm ?? -8) : bay ? (theirTons <= 100 ? -4 : theirTons <= 2_000 ? -2 : 0) : 0;
      const closeRange =
        spinal != null && band === "Short" ? spinalRangeDm(ourTons, shortBandOf(distance)) : 0;
      const bombardment = kind === "MassDriver" && (target.current_maneuver ?? 0) > 0 ? -12 : 0;
      // Itemised the way the results log itemises an attack: a bare -1 is a
      // riddle, and the gunner's next question is always "why".
      // A salvo is not the gunner's shot. Launching makes no attack roll at
      // all: the missile rolls when it arrives, with its own guidance and
      // none of the gunner's skill, the mount's own accuracy or the range it
      // was thrown from. Showing the gunner's skill on that row said a rack
      // would hit like a gun, which it does not.
      const terms: [string, number][] = salvo
        ? [
            ["smart missile (our TL over theirs)", smartMissile],
            ["sensor lock", locked ? 2 : 0],
            ["their pilot evading", dodge],
            ["their Evade software", evadeSoftware],
            ["Advanced Fire Control", advanced],
            ["Launch Solution", launchSolution],
          ]
        : [
            ["gunner", gunner],
            ["weapon", WEAPON_HIT_MOD[kind] ?? 0],
            [`range (${band})`, RANGE_MOD[band]],
            ["target size", smallTarget],
            [`spinal at ${band === "Short" ? shortBandOf(distance) : band}`, closeRange],
            ["orbital bombardment", bombardment],
            ["sensor lock", locked ? 2 : 0],
            ["their pilot evading", dodge],
            ["their Evade software", evadeSoftware],
            ["Advanced Fire Control", advanced],
          ];
      const dm = terms.reduce((total, [, value]) => total + value, 0);
      const named = terms.filter(([, value]) => value !== 0);
      const why =
        named.length === 0
          ? "No modifiers: a plain 2D against 8."
          : named.map(([name, value]) => `${name} ${value >= 0 ? "+" : ""}${value}`).join(", ");

      return {
        label: weaponToString(weapon),
        manned,
        inReach: inReach && !tooSmall,
        tooSmall,
        dm,
        why,
        chance: chanceOf(dm),
      };
    });
  }, [args.ship, target, templates]);

  // Nothing to weigh with nothing to shoot at, and an offer to work out the
  // odds against no one reads as a fault.
  if (target == null) {
    return null;
  }

  if (!shown) {
    return (
      <button type="button" className="odds-toggle" data-tutorial="shot-odds" onClick={() => setShown(true)}>
        Show shot odds
      </button>
    );
  }

  return (
    <div className="shot-odds" data-tutorial="shot-odds">
      <div className="odds-head">
        <select
          className="salvo-select"
          value={target?.name ?? ""}
          onChange={(event) => setTargetName(event.target.value)}>
          {targets.map((other) => (
            <option key={other.name} value={other.name}>
              {other.name}
            </option>
          ))}
        </select>
        <button type="button" className="odds-toggle" onClick={() => setShown(false)}>
          hide
        </button>
      </div>
      <ul className="odds-rows">
        {rows.map((row, index) => (
          <li key={index} className={row.manned && row.inReach ? "odds-row" : "odds-row odds-row-dead"}>
            <span className="odds-weapon">{row.label}</span>
            <span className="odds-dm" title={row.why}>
              {row.dm >= 0 ? "+" : ""}
              {row.dm}
            </span>
            <span
              className="odds-chance"
              title={
                row.manned && row.inReach
                  ? `2D ${row.dm >= 0 ? "+" : ""}${row.dm} against 8 lands ${row.chance} times in 100`
                  : undefined
              }>
              {!row.manned
                ? "unmanned"
                : row.tooSmall
                  ? "too small; stationary targets only"
                  : row.inReach
                    ? `${row.chance}%`
                    : "out of reach"}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

export default ShotOdds;

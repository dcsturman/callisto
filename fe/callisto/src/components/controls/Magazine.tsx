import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {shipWeapons} from "lib/shipDesignTemplates";
import {weaponGuns} from "lib/weapon";
import {useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";

/**
 * What is left to shoot, and what throws it.
 *
 * Missiles and torpedoes come out of the magazine as they are launched, and
 * a barrel goes with every sand cloud. The sandcaster count rides with the
 * barrels because the two are one question -- how many clouds can we put up
 * this round, and for how many rounds -- while the gunner's own mounts are
 * already in front of them on the firing card.
 */
export function Magazine(args: {ship: Ship}) {
  const templates = useAppSelector(templatesSelector);
  const magazine = args.ship.magazine;

  // What this hull can actually throw. A ship with no launcher has nothing
  // to say about missiles, and saying "missiles 0" about it reads as a ship
  // that has run dry rather than one that never carried any.
  const {casters, carries} = useMemo(() => {
    let crewed = 0;
    let total = 0;
    const carries = {missiles: false, torpedoes: false, sand: false};
    shipWeapons(args.ship, templates).forEach((weapon, index) => {
      const kinds = weaponGuns(weapon).map((gun) => gun.kind);
      carries.missiles ||= kinds.includes("Missile");
      carries.torpedoes ||= kinds.includes("Torpedo");
      const sand = kinds.filter((kind) => kind === "Sand").length;
      if (sand > 0) {
        carries.sand = true;
      }
      total += sand;
      if ((args.ship.crew?.gunnery?.length ?? 0) > index) {
        crewed += sand;
      }
    });
    return {casters: {crewed, total}, carries};
  }, [args.ship, templates]);

  if (magazine == null) {
    return null;
  }

  const rows = [
    {label: "missiles", count: magazine.missiles, suffix: "", carried: carries.missiles},
    {label: "torpedoes", count: magazine.torpedoes, suffix: "", carried: carries.torpedoes},
    {
      label: "sand",
      carried: carries.sand,
      count: magazine.sand,
      // An unmanned caster cannot throw, so say how many are actually
      // crewed when that is fewer than the ship carries.
      suffix:
        casters.total === 0
          ? ""
          : casters.crewed === casters.total
            ? ` · ${casters.total} caster${casters.total === 1 ? "" : "s"}`
            : ` · ${casters.crewed} of ${casters.total} casters crewed`,
    },
  ].filter((row) => row.carried);

  // Nothing to carry, nothing to say.
  if (rows.length === 0) {
    return null;
  }

  return (
    <ul className="magazine-rows">
      {rows.map((row) => (
        <li key={row.label} className={row.count === 0 ? "magazine-row magazine-out" : "magazine-row"}>
          <span className="magazine-label">{row.label}</span>
          <span className="magazine-count">
            {row.count}
            {row.suffix}
          </span>
        </li>
      ))}
    </ul>
  );
}

export default Magazine;

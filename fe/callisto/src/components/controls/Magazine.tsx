import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {EMPTY_MAGAZINE, describeLoad, mountCapacity} from "lib/magazine";
import {shipWeapons} from "lib/shipDesignTemplates";
import {weaponGuns, weaponToString} from "lib/weapon";
import {useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";

/**
 * What is left to shoot, and what throws it.
 *
 * Each launcher and sandcaster fires from its own load: a missile or torpedo
 * leaves its mount as it is launched, and a canister goes with every sand
 * cloud. What the mounts cannot hold is in stores, and a Reload refills a
 * mount from there. The stores come first, one row per munition; then each
 * mount's load, which is what decides whether it can fire this round. The
 * sandcaster count rides with the canisters because the two are one question
 * -- how many clouds can we put up this round, and for how many rounds.
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

  const loads = useMemo(() => {
    const displacement = templates[args.ship.design]?.displacement ?? 0;
    return shipWeapons(args.ship, templates).flatMap((weapon, index) => {
      const capacity = mountCapacity(weapon, displacement);
      if (capacity.missiles + capacity.torpedoes + capacity.sand === 0) {
        return [];
      }
      const loaded = args.ship.loaded?.[index] ?? EMPTY_MAGAZINE;
      const empty =
        (capacity.missiles > 0 && loaded.missiles === 0) ||
        (capacity.torpedoes > 0 && loaded.torpedoes === 0) ||
        (capacity.sand > 0 && loaded.sand === 0);
      return [
        {
          key: index,
          label: `Mount ${index + 1}: ${weaponToString(weapon)}`,
          load: describeLoad(capacity, loaded),
          empty,
        },
      ];
    });
  }, [args.ship, templates]);

  if (magazine == null) {
    return null;
  }

  const rows = [
    {label: "missiles in stores", count: magazine.missiles, suffix: "", carried: carries.missiles},
    {label: "torpedoes in stores", count: magazine.torpedoes, suffix: "", carried: carries.torpedoes},
    {
      label: "canisters in stores",
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
    <>
      <ul className="magazine-rows" data-tutorial="magazine">
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
      <ul className="magazine-loads" data-tutorial="mount-loads">
        {loads.map((mount) => (
          <li key={mount.key} className={mount.empty ? "magazine-row magazine-out" : "magazine-row"}>
            <span className="magazine-label">{mount.label}</span>
            <span className="magazine-count">{mount.load}</span>
          </li>
        ))}
      </ul>
    </>
  );
}

export default Magazine;

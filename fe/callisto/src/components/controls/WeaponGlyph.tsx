import * as React from "react";
import {useId} from "react";
import {Tooltip} from "react-tooltip";

import {Weapon, WEAPON_COLORS, mountToString, weaponGuns, weaponToString} from "lib/weapon";

import Turret1 from "assets/icons/turret1.svg?react";
import Turret2 from "assets/icons/turret2.svg?react";
import Turret3 from "assets/icons/turret3.svg?react";
import Barbette from "assets/icons/barbette.svg?react";
import FixedMount from "assets/icons/fixed-mount.svg?react";
import SmallBay from "assets/icons/bay-s.svg?react";
import MediumBay from "assets/icons/bay-m.svg?react";
import LargeBay from "assets/icons/bay-l.svg?react";

/**
 * One weapon mount, drawn the way the gunner's own buttons draw it: the mount
 * as the shape, the weapon as the colour.
 *
 * Shared so a reading of another ship's armament uses the same vocabulary as
 * firing your own -- a green triple turret means the same thing in both
 * places. The full name is on hover, since a glyph cannot say "long range" or
 * name the second gun in a mixed turret.
 */
export function WeaponGlyph(args: {weapon: Weapon}) {
  const guns = weaponGuns(args.weapon);
  const kind = guns[0]?.kind ?? args.weapon.kind ?? "";
  const Icon = mountIcon(args.weapon);
  // A mixed mount has no single colour, so the glyph takes its first gun's
  // and the hover text names the rest.
  const mixed = new Set(guns.map((gun) => gun.kind)).size > 1;
  const tipId = useId();

  // What the glyph cannot say: the mount in words, and -- for the bar under a
  // mixed mount -- what the bar means. A native `title` was there before and
  // was easy to miss; this is the same tooltip the gunner's own buttons use.
  const tip = [
    weaponToString(args.weapon),
    mountToString(args.weapon.mount),
    mixed ? "mixed mount (marked by the bar)" : "",
  ]
    .filter((part) => part !== "")
    .join(" — ");

  return (
    <>
      <span
        className={mixed ? "weapon-glyph weapon-glyph-mixed" : "weapon-glyph"}
        data-tooltip-id={tipId}
        data-tooltip-content={tip}
        data-tooltip-delay-show={300}>
        <Icon className="weapon-glyph-icon" style={{fill: WEAPON_COLORS[kind]}} />
      </span>
      <Tooltip id={tipId} className="tooltip-body weapon-button-tooltip" />
    </>
  );
}

/** The shape for this mount: a turret by its size, or the mount's own icon. */
function mountIcon(weapon: Weapon): React.FunctionComponent<React.SVGProps<SVGSVGElement>> {
  const mount = weapon.mount;
  if (mount === "FixedMount") {
    return FixedMount;
  }
  if (typeof mount === "string") {
    return Barbette;
  }
  if ("Turret" in mount) {
    return mount.Turret >= 3 ? Turret3 : mount.Turret === 2 ? Turret2 : Turret1;
  }
  if ("Bay" in mount) {
    return mount.Bay === "Large" ? LargeBay : mount.Bay === "Medium" ? MediumBay : SmallBay;
  }
  // A point-defence battery has no icon of its own; a turret is close enough.
  return Turret1;
}

export default WeaponGlyph;

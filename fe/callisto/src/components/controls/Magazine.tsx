import * as React from "react";

import {Ship} from "lib/entities";

/**
 * What is left to shoot.
 *
 * Missiles and torpedoes come out of the magazine as they are launched, and
 * a barrel goes with every sand cloud. A gunner planning a salvo wants to
 * know what that leaves, and a captain deciding whether to press an
 * engagement wants to know when the racks run dry.
 */
export function Magazine(args: {ship: Ship}) {
  const magazine = args.ship.magazine;
  if (magazine == null) {
    return null;
  }

  const rows = [
    {label: "missiles", count: magazine.missiles},
    {label: "torpedoes", count: magazine.torpedoes},
    {label: "sand", count: magazine.sand},
  ].filter((row) => row.count > 0 || row.label === "missiles");

  if (rows.every((row) => row.count === 0)) {
    return <p className="magazine-empty">Magazine empty.</p>;
  }

  return (
    <ul className="magazine-rows">
      {rows.map((row) => (
        <li key={row.label} className={row.count === 0 ? "magazine-row magazine-out" : "magazine-row"}>
          <span className="magazine-label">{row.label}</span>
          <span className="magazine-count">{row.count}</span>
        </li>
      ))}
    </ul>
  );
}

export default Magazine;

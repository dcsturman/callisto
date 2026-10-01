import * as React from "react";

/**
 * What a player's console shows once their ship has left the scenario.
 *
 * A ship that jumps out or is destroyed is simply no longer in the entity
 * list, and every panel keyed to it rendered nothing -- the console went
 * blank, and the pilot's course request raced the removal and surfaced as a
 * browser alert about an "unknown ship". The stations are gone either way;
 * saying so is better than an empty column and an error box.
 */
export function ShipGone(args: {shipName: string}) {
  return (
    <div className="ship-gone">
      <h2 className="ship-gone-title">{args.shipName} is out of the action</h2>
      <p className="ship-gone-note">
        Jumped out, destroyed, or removed by the referee. Its stations are closed; the rest of the
        battle carries on without you.
      </p>
    </div>
  );
}

export default ShipGone;

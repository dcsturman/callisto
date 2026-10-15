import * as React from "react";
import { useEffect, useState } from "react";
import { Ship } from "lib/entities";
import { setOrders, setShipController } from "lib/serverManager";

/**
 * Who is giving this ship its orders, and what they are.
 *
 * Belongs to the ship rather than to any station, so it sits in the Ship's
 * Computer card where every role sees it. Anyone at the ship's console may
 * hand it to the AI or take it back; that is the whole permission model for
 * now.
 *
 * The orders are plain English. An AI-crewed ship plans against them; a human
 * crew reads them as their brief. They are sent when the box loses focus
 * rather than on every keystroke, because each send rebroadcasts the world.
 */
export function ShipCommand(args: { ship: Ship }) {
  const ai = args.ship.controller === "AI";
  const [draft, setDraft] = useState(args.ship.orders ?? "");

  // Someone else changed the orders: show theirs.
  useEffect(() => setDraft(args.ship.orders ?? ""), [args.ship.orders, args.ship.name]);

  const commit = () => {
    if (draft.trim() !== (args.ship.orders ?? "")) {
      setOrders(args.ship.name, draft);
    }
  };

  return (
    <div className="ship-command">
      <div className="ship-command-crew">
        <h2>Crew</h2>
        <span className={ai ? "controller-chip controller-chip-ai" : "controller-chip"}>
          {ai ? "AI" : "People"}
        </span>
        <button
          className="control-button ship-command-switch"
          title={
            ai
              ? "Take this ship back: its stations go to the people at its consoles."
              : "Hand this ship to the AI: it will plan and act every round on its own, against its orders."
          }
          onClick={() => setShipController(args.ship.name, ai ? "Human" : "AI")}
        >
          {ai ? "Take back" : "Switch to AI"}
        </button>
      </div>
      <label className="ship-command-orders">
        <h2>Orders</h2>
        <textarea
          className="ship-command-orders-text"
          rows={2}
          placeholder="None. For example: close to Medium and cripple her drive; do not destroy her."
          value={draft}
          onChange={(event) => setDraft(event.target.value)}
          onBlur={commit}
        />
      </label>
    </div>
  );
}

export default ShipCommand;

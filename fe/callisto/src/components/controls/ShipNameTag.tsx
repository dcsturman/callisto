import * as React from "react";
import { Team, teamLabelColor } from "lib/teams";

/** A ship named in a message: her name, and her side when known. */
export interface ShipRef {
  name: string;
  team?: Team | null;
}

/**
 * One ship's name inside a line of text, in its own team's colour.
 *
 * Bold always: the eye should find who a line is about before reading the
 * line. Grey for a ship on no side, which is the same grey the ship list
 * gives it. Shared by the results and Fleet Comms so a ship looks the same in
 * both.
 */
export function ShipNameTag(args: { ship: ShipRef }) {
  return (
    <span className="result-ship" style={{ color: teamLabelColor(args.ship.team ?? null) }}>
      {args.ship.name}
    </span>
  );
}

/**
 * Free text with every known ship's name drawn as a {@link ShipNameTag}: what an
 * AI captain writes names ships in passing ("close on HMS Executor"), and they
 * should read the way they do in the results. Longest names first, so "Executor"
 * does not match inside "HMS Executor".
 */
export function WithShipNames(args: { text: string; ships: ShipRef[] }) {
  const names = [...args.ships].sort((a, b) => b.name.length - a.name.length);
  if (names.length === 0) {
    return <>{args.text}</>;
  }
  const escape = (name: string) => name.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const pattern = new RegExp(`(${names.map((ship) => escape(ship.name)).join("|")})`, "g");
  return (
    <>
      {args.text.split(pattern).map((piece, index) => {
        const ship = names.find((candidate) => candidate.name === piece);
        return ship == null ? (
          <React.Fragment key={index}>{piece}</React.Fragment>
        ) : (
          <ShipNameTag key={index} ship={ship} />
        );
      })}
    </>
  );
}

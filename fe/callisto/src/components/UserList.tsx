import * as React from "react";
import { ViewMode, rolesToString } from "lib/view";
import type { Ship } from "lib/entities";
import { TEAM_CSS, Team } from "lib/teams";

// Peer entries on the wire carry `display_name` (server strips the email's
// local part). The current player's own email is still tracked separately in
// `userSlice.email`.
export type UserContext = {
    display_name: string;
    roles: ViewMode[],
    ship: string | null;
    /** Whether this player has said their orders are in for this round. */
    ready?: boolean;
    /**
     * Further ships this player answers for beyond `ship`: a team's
     * commander, or the AI crewing a side. Absent for one console.
     */
    ships?: string[];
}

export type UserList = UserContext[];

/**
 * The one side every ship in `fleet` is on, if they are all on the same one:
 * what to call a player answering for a whole side, such as the AI.
 */
export const sideOf = (fleet: Set<string>, ships: Ship[]): Team | null => {
    const teams = new Set(ships.filter((ship) => fleet.has(ship.name)).map((ship) => ship.team ?? null));
    const [team] = [...teams];
    return teams.size === 1 && team != null ? team : null;
};

export function Users(args: {users: UserList, email: string | null, ships?: Ship[]}) {
    // Derive the current player's display name client-side from their email's
    // local part to match the server's `display_name` for filtering.
    // Same-prefix collisions (alice@gmail.com vs alice@example.com) result in
    // the current user appearing in the peer list — cosmetic only, no
    // security implication.
    const ownDisplayName = args.email ? args.email.split("@")[0] : null;

    return (
        args.users.length > 1 ? <div className="user-list">
            <h4>Users</h4>
            <ul className="user-list-list">
                {args.users.filter(user => user.display_name !== ownDisplayName).map((user, index) => {
                    // General on a ship reads as just the ship: it is every
                    // station, and naming them all says less than the name of
                    // the ship. Anything narrower is listed, comma-separated,
                    // so "Captain, Gunner on Executor" says exactly what that
                    // player is doing.
                    const general = user.roles.includes(ViewMode.General);
                    let role_text = "";
                    // A whole side on one connection reads as how many ships it
                    // has: "Pirates (AI)" with three names after it would not fit.
                    const fleet = new Set([...(user.ship == null ? [] : [user.ship]), ...(user.ships ?? [])]);
                    // Several players can share a name -- the AI crews each
                    // side on a connection of its own -- so the side they
                    // answer for is what tells them apart.
                    const side = (user.ships ?? []).length > 0 ? sideOf(fleet, args.ships ?? []) : null;
                    const commander = user.roles.includes(ViewMode.FleetCommand);
                    if (side != null) {
                        role_text = "";
                    } else if (fleet.size > 1) {
                        role_text = ` (${fleet.size} ships)`;
                    } else if (general && user.ship != null) {
                        role_text = ` (On ${user.ship})`;
                    } else if (!general && user.ship == null) {
                        role_text = ` (${rolesToString(user.roles)})`;
                    } else if (!general && user.ship != null) {
                        role_text = ` (${rolesToString(user.roles)} on ${user.ship})`;
                    }
                    return (
                    <li key={`${user.display_name}-${index}`}>
                        {/* A tick for a player who is done this round, with the
                            same width held for one who is not, so names do not
                            shift sideways as the table readies up. */}
                        <span className="user-ready-mark">{user.ready ? "\u2713" : "\u00a0"}</span>
                        {user.display_name}{role_text}
                        {side != null && commander && (
                            <>
                                {" (Fleet Command, "}
                                <span style={{ color: TEAM_CSS[side] }}>{side}</span>
                                {")"}
                            </>
                        )}
                        {side != null && !commander && (
                            <>
                                {" ("}
                                <span style={{ color: TEAM_CSS[side] }}>{side} side</span>
                                {/* One ship is named: its name says more than a count. */}
                                {fleet.size === 1 ? `, ${[...fleet][0]})` : `, ${fleet.size} ships)`}
                            </>
                        )}
                    </li>
                )})}
            </ul>
        </div> : <></>
    );
};

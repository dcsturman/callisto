import * as React from "react";
import { ViewMode, rolesToString } from "lib/view";

// Peer entries on the wire carry `display_name` (server strips the email's
// local part). The current player's own email is still tracked separately in
// `userSlice.email`.
export type UserContext = {
    display_name: string;
    roles: ViewMode[],
    ship: string | null;
}

export type UserList = UserContext[];

export function Users(args: {users: UserList, email: string | null}) {
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
                {args.users.filter(user => user.display_name !== ownDisplayName).map((user) => {
                    // General on a ship reads as just the ship: it is every
                    // station, and naming them all says less than the name of
                    // the ship. Anything narrower is listed, comma-separated,
                    // so "Captain, Gunner on Executor" says exactly what that
                    // player is doing.
                    const general = user.roles.includes(ViewMode.General);
                    let role_text = "";
                    if (general && user.ship != null) {
                        role_text = ` (On ${user.ship})`;
                    } else if (!general && user.ship == null) {
                        role_text = ` (${rolesToString(user.roles)})`;
                    } else if (!general && user.ship != null) {
                        role_text = ` (${rolesToString(user.roles)} on ${user.ship})`;
                    }
                    return (
                    <li key={user.display_name}>{user.display_name}{role_text}</li>
                )})}
            </ul>
        </div> : <></>
    );
};

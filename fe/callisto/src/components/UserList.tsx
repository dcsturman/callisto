import * as React from "react";
import { ViewMode } from "lib/view";

// Peer entries on the wire carry `display_name` (server strips the email's
// local part). The current player's own email is still tracked separately in
// `userSlice.email`.
export type UserContext = {
    display_name: string;
    role: ViewMode,
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
                    let role_text = "";
                    if (user.role as ViewMode === ViewMode.General && user.ship != null) {
                        role_text = ` (On ${user.ship})`;
                    } else if (user.role as ViewMode !== ViewMode.General && user.ship == null) {
                        role_text = ` (${ViewMode[user.role]})`;
                    } else if (user.role as ViewMode !== ViewMode.General && user.ship != null) {
                        role_text = ` (${ViewMode[user.role]} on ${user.ship})`;
                    };
                    return (
                    <li key={user.display_name}>{user.display_name}{role_text}</li>
                )})}
            </ul>
        </div> : <></>
    );
};

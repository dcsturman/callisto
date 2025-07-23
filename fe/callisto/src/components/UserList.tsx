import * as React from "react";
import { ViewMode } from "lib/view";

export type UserContext = {
    email: string;
    role: ViewMode,
    ship: string | null;
}

export type UserList = UserContext[];

export function Users(args: {users: UserList, email: string | null}) {
    return (
        args.users.length > 1 ? <div className="user-list">
            <h4>Users</h4>
            <ul className="user-list-list">
                {args.users.filter(user => user.email !== args.email).map((user) => {
                    let role_text = "";
                    if (user.role as ViewMode === ViewMode.General && user.ship != null) {
                        role_text = ` (On ${user.ship})`;
                    } else if (user.role as ViewMode !== ViewMode.General && user.ship == null) {
                        role_text = ` (${ViewMode[user.role]})`;
                    } else if (user.role as ViewMode !== ViewMode.General && user.ship != null) {
                        role_text = ` (${ViewMode[user.role]} on ${user.ship})`;
                    };
                    return (
                    <li key={user.email}>{user.email}{role_text}</li>
                )})}
            </ul>
        </div> : <></>
    );
};
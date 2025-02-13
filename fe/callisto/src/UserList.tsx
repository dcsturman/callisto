import * as React from "react";

export type UserList = [string];

export function Users(args: {users: UserList, email: string | null}) {
    console.log("Length of list = " + args.users.length);
    return (
        args.users.length > 1 ? <div className="user-list">
            <h4>Users</h4>
            <ul className="user-list-list">
                {args.users.filter(user => user !== args.email).map((user) => (
                    <li key={user}>{user}</li>
                ))}
            </ul>
        </div> : <></>
    );
};
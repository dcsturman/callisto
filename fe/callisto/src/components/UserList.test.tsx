// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";

// Tell React this is an `act()`-aware environment so it doesn't warn on
// every state update during tests.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { Users, UserList } from "components/UserList";
import { ViewMode } from "lib/view";

// We avoid @testing-library/react here because @testing-library/dom is not
// installed as a dependency (it's a peer dep of testing-library/react that
// would require touching package.json — out of scope for this agent).
// Plain createRoot + document queries get the job done.

let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
});

afterEach(() => {
  act(() => {
    root.unmount();
  });
  container.remove();
});

function renderUsers(users: UserList, email: string | null) {
  act(() => {
    root.render(<Users users={users} email={email} />);
  });
}

describe("Users (peer list)", () => {
  it("renders all peers when more than one user is present", () => {
    renderUsers(
      [
        { display_name: "alice", role: ViewMode.General, ship: "Buccaneer" },
        { display_name: "bob", role: ViewMode.Pilot, ship: "Buccaneer" },
      ],
      null,
    );
    expect(container.textContent).toContain("alice");
    expect(container.textContent).toContain("bob");
  });

  it("filters out the current user by display_name (derived from email local-part)", () => {
    renderUsers(
      [
        { display_name: "alice", role: ViewMode.General, ship: null },
        { display_name: "bob", role: ViewMode.General, ship: null },
      ],
      "alice@example.com",
    );
    // bob renders, alice does not.
    expect(container.textContent).toContain("bob");
    expect(container.textContent).not.toContain("alice");
  });

  it("renders nothing when only the current user is in the list", () => {
    renderUsers(
      [{ display_name: "alice", role: ViewMode.General, ship: null }],
      "alice@example.com",
    );
    expect(container.querySelector(".user-list")).toBeNull();
  });

  it("includes role/ship suffix in the rendered label", () => {
    renderUsers(
      [
        { display_name: "alice", role: ViewMode.Pilot, ship: "Buccaneer" },
        { display_name: "bob", role: ViewMode.General, ship: "Buccaneer" },
      ],
      null,
    );
    const items = Array.from(container.querySelectorAll("li")).map(
      (li) => li.textContent ?? "",
    );
    expect(items.some((t) => /alice.*Pilot.*Buccaneer/.test(t))).toBe(true);
    expect(items.some((t) => /bob.*On Buccaneer/.test(t))).toBe(true);
  });
});

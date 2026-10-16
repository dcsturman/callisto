// @vitest-environment jsdom
import { describe, it, expect, beforeEach, afterEach } from "vitest";
import * as React from "react";
import { createRoot, Root } from "react-dom/client";
import { act } from "react";

// Tell React this is an `act()`-aware environment so it doesn't warn on
// every state update during tests.
(globalThis as unknown as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

import { Users, UserList, sideOf } from "components/UserList";
import type { Ship } from "lib/entities";
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

function renderUsers(users: UserList, email: string | null, ships?: Ship[]) {
  act(() => {
    root.render(<Users users={users} email={email} ships={ships} />);
  });
}

// Only the fields the user list reads from a ship.
const ship = (name: string, team?: string): Ship => ({ name, team }) as unknown as Ship;

describe("Users (peer list)", () => {
  it("renders all peers when more than one user is present", () => {
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.General], ship: "Buccaneer" },
        { display_name: "bob", roles: [ViewMode.Pilot], ship: "Buccaneer" },
      ],
      null,
    );
    expect(container.textContent).toContain("alice");
    expect(container.textContent).toContain("bob");
  });

  it("shows a connection answering for a side by its ship count", () => {
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.General], ship: "Executor" },
        {
          display_name: "ai-ships",
          roles: [ViewMode.General],
          ship: null,
          ships: ["Invigilator", "Cicada", "Death's Actuary"],
        },
      ],
      null,
    );
    expect(container.textContent).toContain("ai-ships (3 ships)");
    expect(container.textContent).toContain("alice (On Executor)");
  });

  it("ticks players who are ready and leaves the rest unticked", () => {
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.Pilot], ship: "Buccaneer", ready: true },
        { display_name: "bob", roles: [ViewMode.Gunner], ship: "Buccaneer" },
      ],
      null,
    );
    const rows = Array.from(container.querySelectorAll("li"));
    const alice = rows.find((row) => row.textContent?.includes("alice"));
    const bob = rows.find((row) => row.textContent?.includes("bob"));
    expect(alice?.textContent).toContain("\u2713");
    expect(bob?.textContent).not.toContain("\u2713");
  });

  it("filters out the current user by display_name (derived from email local-part)", () => {
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.General], ship: null },
        { display_name: "bob", roles: [ViewMode.General], ship: null },
      ],
      "alice@example.com",
    );
    // bob renders, alice does not.
    expect(container.textContent).toContain("bob");
    expect(container.textContent).not.toContain("alice");
  });

  it("renders nothing when only the current user is in the list", () => {
    renderUsers(
      [{ display_name: "alice", roles: [ViewMode.General], ship: null }],
      "alice@example.com",
    );
    expect(container.querySelector(".user-list")).toBeNull();
  });

  it("includes role/ship suffix in the rendered label", () => {
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.Pilot], ship: "Buccaneer" },
        { display_name: "bob", roles: [ViewMode.General], ship: "Buccaneer" },
      ],
      null,
    );
    const items = Array.from(container.querySelectorAll("li")).map(
      (li) => li.textContent ?? "",
    );
    expect(items.some((t) => /alice.*Pilot.*Buccaneer/.test(t))).toBe(true);
    expect(items.some((t) => /bob.*On Buccaneer/.test(t))).toBe(true);
  });

  it("lists every station a player is working, comma-separated", () => {
    // A small crew doubling up: "Captain, Gunner on Executor" says exactly what
    // that player is doing, where General would have said only the ship.
    renderUsers(
      [
        { display_name: "alice", roles: [ViewMode.Captain, ViewMode.Gunner], ship: "Executor" },
        { display_name: "bob", roles: [ViewMode.Pilot, ViewMode.Sensors], ship: null },
      ],
      null,
    );
    const items = Array.from(container.querySelectorAll("li")).map(
      (li) => li.textContent ?? "",
    );
    expect(items.some((t) => t.includes("alice (Captain, Gunner on Executor)"))).toBe(true);
    expect(items.some((t) => t.includes("bob (Pilot, Sensors)"))).toBe(true);
  });
});

describe("an AI player's side", () => {
  const ships = [ship("AF1", "Red"), ship("Ailreaui", "Red"), ship("INS Stag", "Blue"), ship("Drifter")];

  it("is named when every ship it answers for is on one side", () => {
    expect(sideOf(new Set(["AF1", "Ailreaui"]), ships)).toBe("Red");
  });

  it("is not named for a mixed fleet, or one with no team", () => {
    expect(sideOf(new Set(["AF1", "INS Stag"]), ships)).toBeNull();
    expect(sideOf(new Set(["Drifter"]), ships)).toBeNull();
  });

  it("tells two AI connections apart by the side each crews", () => {
    renderUsers(
      [
        { display_name: "ai-ships", roles: [ViewMode.General], ship: null, ships: ["AF1", "Ailreaui"] },
        { display_name: "ai-ships", roles: [ViewMode.General], ship: null, ships: ["INS Stag"] },
      ],
      null,
      ships,
    );
    expect(container.textContent).toContain("ai-ships (Red side, 2 ships)");
    expect(container.textContent).toContain("ai-ships (Blue side, 1 ship)");
  });
});

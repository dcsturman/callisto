import { describe, expect, it } from "vitest";
import type { Ship } from "lib/entities";
import { shipsTakingTeamOrders, teamOrders } from "lib/teamOrders";

// Only the fields team orders read; the rest of a ship does not matter here.
const ship = (name: string, orders?: string): Ship => ({ name, team: "Blue", orders }) as unknown as Ship;

describe("teamOrders", () => {
  it("reads the orders most ships share as the team's, and the rest as their own", () => {
    const ships = [
      ship("Kestrel", "Destroy the corsairs."),
      ship("Bulwark", "Destroy the corsairs."),
      ship("Diligent"),
      ship("Peregrine", "Protect Kestrel."),
    ];
    expect(teamOrders(ships)).toEqual({
      orders: "Destroy the corsairs.",
      following: ["Kestrel", "Bulwark", "Diligent"],
      own: ["Peregrine"],
    });
    expect(shipsTakingTeamOrders(ships)).toEqual(["Kestrel", "Bulwark", "Diligent"]);
  });

  it("has no orders for a team none of whose ships has any", () => {
    expect(teamOrders([ship("A"), ship("B")])).toEqual({ orders: "", following: ["A", "B"], own: [] });
  });
});

import { describe, expect, it } from "vitest";
import { sizeAfterMove, squadronShips, teamSquadrons } from "lib/squadrons";
import { Ship } from "lib/entities";

const ship = (name: string, team?: string, squadron?: string) => ({ name, team, squadron }) as unknown as Ship;

describe("squadrons", () => {
  const ships = [
    ship("Lance 2", "Blue", "Lance"),
    ship("Lance 1", "Blue", "Lance"),
    ship("Hesperus", "Blue"),
    ship("Spear 1", "Red", "Lance"),
    ship("Drifter", undefined, "Lance"),
  ];

  it("lists a team's squadrons with their ships in order", () => {
    expect(teamSquadrons(ships, "Blue")).toEqual([{ name: "Lance", ships: ["Lance 1", "Lance 2"] }]);
    expect(teamSquadrons(ships, "Red")).toEqual([{ name: "Lance", ships: ["Spear 1"] }]);
  });

  it("keeps same-named squadrons on different teams apart", () => {
    expect(squadronShips(ships, "Blue", "Lance")).toEqual(["Lance 1", "Lance 2"]);
    expect(squadronShips(ships, null, "Lance")).toEqual(["Drifter"]);
  });
});

describe("sizeAfterMove", () => {
  const ships = [ship("Lance 1", "Blue", "Lance"), ship("Lance 2", "Blue", "Lance"), ship("Hesperus", "Blue")];

  it("counts ships already in the squadron once", () => {
    expect(sizeAfterMove(ships, "Blue", "Lance", ["Lance 1", "Hesperus"])).toBe(3);
    expect(sizeAfterMove(ships, "Blue", "New", ["Hesperus"])).toBe(1);
  });
});

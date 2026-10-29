import { describe, expect, it } from "vitest";
import {
  batchNames,
  centroid,
  gridPositions,
  hullShare,
  sizeAfterMove,
  squadronShips,
  squadronStep,
  teamSquadrons,
} from "lib/squadrons";
import { ShipDesignTemplates } from "lib/shipDesignTemplates";
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

describe("hullShare", () => {
  const templates = { Fighter: { hull: 10 }, Cruiser: { hull: 100 } } as unknown as ShipDesignTemplates;
  const hulled = (design: string, current_hull: number) => ({ design, current_hull }) as unknown as Ship;

  it("weighs each ship by her design's hull", () => {
    expect(hullShare([hulled("Fighter", 0), hulled("Cruiser", 55)], templates)).toBeCloseTo(0.5);
  });

  it("is null when no design is known", () => {
    expect(hullShare([hulled("Mystery", 3)], templates)).toBeNull();
  });
});

describe("centroid", () => {
  it("is the middle of the ships", () => {
    const at = (position: number[]) => ({ position }) as unknown as Ship;
    expect(centroid([at([0, 0, 0]), at([10, 20, -4])])).toEqual([5, 10, -2]);
  });
});

describe("squadronStep", () => {
  const reported = (rationale: string | undefined, status = "Ready") =>
    ({ name: "x", ai_report: { round: 1, status, rationale } }) as unknown as Ship;

  it("takes the squadron's step from the front of a ship's report", () => {
    expect(
      squadronStep([reported("Lance, step 1 of 2: engage Light Fighter; until round 6. Closing on the cruiser.")], "Lance"),
    ).toBe("step 1 of 2: engage Light Fighter; until round 6");
    expect(squadronStep([reported("Lance: hold. Waiting.")], "Lance")).toBe("hold");
  });

  it("says when the AI is still deciding, and nothing for ships with no AI", () => {
    expect(squadronStep([reported("Lance: hold."), reported(undefined, "Deciding")], "Lance")).toBe("deciding…");
    expect(squadronStep([{ name: "y" } as unknown as Ship], "Lance")).toBeNull();
  });
});

describe("batchNames", () => {
  it("numbers from the base name, skipping names taken", () => {
    expect(batchNames("Lance", 3, ["Lance 02"])).toEqual(["Lance 01", "Lance 03", "Lance 04"]);
  });

  it("drops a number already on the base", () => {
    expect(batchNames("Lance 1", 2, [])).toEqual(["Lance 01", "Lance 02"]);
  });
});

describe("gridPositions", () => {
  it("lays ships five abreast, centred on the point", () => {
    const at = gridPositions([1000, 0, 7], 7, 100);
    expect(at).toHaveLength(7);
    // Two rows, 100 apart, either side of x = 1000.
    expect(at[0]).toEqual([950, -200, 7]);
    expect(at[4]).toEqual([950, 200, 7]);
    // The second row has two, centred too.
    expect(at[5]).toEqual([1050, -50, 7]);
    expect(at[6]).toEqual([1050, 50, 7]);
  });

  it("puts one ship on the point", () => {
    expect(gridPositions([1, 2, 3], 1, 100)).toEqual([[1, 2, 3]]);
  });
});

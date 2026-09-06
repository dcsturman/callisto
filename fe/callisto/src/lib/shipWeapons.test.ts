import { describe, it, expect } from "vitest";
import type { Ship } from "lib/entities";
import {
  ShipDesignTemplates,
  defaultShipDesignTemplate,
  shipWeapons,
} from "lib/shipDesignTemplates";
import { Weapon, createWeapon } from "lib/weapon";

const designWeapons: Weapon[] = [
  createWeapon("Pulse", { Turret: 2 }),
  createWeapon("Sand", { Turret: 2 }),
];

const templates: ShipDesignTemplates = {
  "Free Trader": {
    ...defaultShipDesignTemplate(),
    name: "Free Trader",
    displacement: 200,
    weapons: designWeapons,
  },
};

const shipNamed = (design: string, weapons?: Weapon[]) =>
  ({ name: "Test", design, weapons }) as Ship;

describe("shipWeapons", () => {
  // This is the whole reason the per-ship armament editor and the read path had
  // to land together: without it a ship with custom weapons shows its design's.
  it("prefers the ship's own armament over its design's", () => {
    const own = [createWeapon("Particle", "Barbette")];
    expect(shipWeapons(shipNamed("Free Trader", own), templates)).toEqual(own);
  });

  it("falls back to the design when the ship has no armament of its own", () => {
    expect(shipWeapons(shipNamed("Free Trader"), templates)).toEqual(designWeapons);
  });

  it("treats an explicitly empty armament as an unarmed ship, not as inherit", () => {
    expect(shipWeapons(shipNamed("Free Trader", []), templates)).toEqual([]);
  });

  it("returns nothing for an unknown design rather than throwing", () => {
    expect(shipWeapons(shipNamed("No Such Design"), templates)).toEqual([]);
  });

  it("returns nothing for a null ship", () => {
    expect(shipWeapons(null, templates)).toEqual([]);
  });
});

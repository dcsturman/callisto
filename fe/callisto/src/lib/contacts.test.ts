import { describe, it, expect } from "vitest";
import { hasContact, isUndetected, isUndetectedBy, isViewersOwn, undetectedTest } from "lib/contacts";
import { Ship } from "lib/entities";
import { Team } from "lib/teams";

const ship = (name: string, contacts?: string[], team?: Team): Ship =>
  ({ name, contacts, team }) as unknown as Ship;

describe("hasContact", () => {
  it("finds a ship on the contact list", () => {
    expect(hasContact(ship("Seeker", ["Quarry"]), ship("Quarry"))).toBe(true);
  });

  it("does not find one that is absent", () => {
    expect(hasContact(ship("Seeker", ["Other"]), ship("Quarry"))).toBe(false);
  });

  it("treats a missing list as detecting nothing, not as unknown", () => {
    // The server omits `contacts` when empty, so absent means blind. Treating
    // it as "unknown, show everything" would make a fully blind ship look
    // omniscient.
    expect(hasContact(ship("Seeker"), ship("Quarry"))).toBe(false);
  });

  it("always detects itself", () => {
    expect(hasContact(ship("Seeker"), ship("Seeker"))).toBe(true);
  });

  it("detects everything when there is no observer at all", () => {
    expect(hasContact(null, ship("Quarry"))).toBe(true);
  });

  it("always detects a team-mate, contact list or not", () => {
    // A squadron shares a plot as a matter of course. Mirrors Ship::detects on
    // the server; if these disagree the display and the rules diverge.
    const seeker = ship("Seeker", [], "Green");
    expect(hasContact(seeker, ship("Wingman", [], "Green"))).toBe(true);
  });

  it("does not detect the other side just for having a team", () => {
    const seeker = ship("Seeker", [], "Green");
    expect(hasContact(seeker, ship("Enemy", [], "Red"))).toBe(false);
    expect(hasContact(seeker, ship("Neutral", []))).toBe(false);
  });
});

describe("isUndetected", () => {
  it("hides nothing in the all-ships view, where no ship is doing the looking", () => {
    expect(isUndetected(null, ship("Quarry"))).toBe(false);
    expect(isUndetected(undefined, ship("Quarry"))).toBe(false);
  });

  it("hides a ship the observer has no contact on", () => {
    expect(isUndetected(ship("Seeker", []), ship("Quarry"))).toBe(true);
  });

  it("does not hide one it can see", () => {
    expect(isUndetected(ship("Seeker", ["Quarry"]), ship("Quarry"))).toBe(false);
  });

  it("never hides a team-mate", () => {
    expect(
      isUndetected(ship("Seeker", [], "Gold"), ship("Wingman", [], "Gold")),
    ).toBe(false);
  });

  it("never hides the observer from itself", () => {
    expect(isUndetected(ship("Seeker", []), ship("Seeker"))).toBe(false);
  });
});

describe("a team as the viewer", () => {
  const scout = ship("Scout", ["Raider"], "Blue");
  const escort = ship("Escort", [], "Blue");
  const raider = ship("Raider", [], "Red");
  const lurker = ship("Lurker", [], "Red");
  const fleet = [scout, escort, raider, lurker];
  const blue = { team: "Blue" };

  it("sees whatever any of its ships has found", () => {
    expect(isUndetectedBy(blue, raider, fleet)).toBe(false);
  });

  it("does not see what none of them has", () => {
    expect(isUndetectedBy(blue, lurker, fleet)).toBe(true);
  });

  it("always sees its own ships", () => {
    expect(isUndetectedBy(blue, escort, fleet)).toBe(false);
    expect(isViewersOwn(blue, escort)).toBe(true);
    expect(isViewersOwn(blue, raider)).toBe(false);
  });

  it("is nobody in particular for the referee, who sees everything", () => {
    expect(isUndetectedBy(null, lurker, fleet)).toBe(false);
  });
});

describe("undetectedTest", () => {
  const ships = [
    ship("Blue 1", ["Red 1"], "Blue"),
    ship("Blue 2", ["Red 2"], "Blue"),
    ship("Red 1", [], "Red"),
    ship("Red 2", ["Blue 1"], "Red"),
    ship("Red 3", [], "Red"),
  ];

  it("agrees with isUndetectedBy for a team, a ship and the referee", () => {
    for (const viewer of [{ team: "Blue" }, { team: "Red" }, { ship: ships[3] }, null]) {
      const test = undetectedTest(viewer, ships);
      ships.forEach((target) => expect(test(target)).toBe(isUndetectedBy(viewer, target, ships)));
    }
  });

  it("pools the team's contacts", () => {
    const test = undetectedTest({ team: "Blue" }, ships);
    expect(ships.filter(test).map((s) => s.name)).toEqual(["Red 3"]);
  });
});

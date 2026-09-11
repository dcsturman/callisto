import { describe, it, expect } from "vitest";
import { hasContact, isUndetected } from "lib/contacts";
import { Ship } from "lib/entities";

const ship = (name: string, contacts?: string[]): Ship =>
  ({ name, contacts }) as unknown as Ship;

describe("hasContact", () => {
  it("finds a ship on the contact list", () => {
    expect(hasContact(ship("Seeker", ["Quarry"]), "Quarry")).toBe(true);
  });

  it("does not find one that is absent", () => {
    expect(hasContact(ship("Seeker", ["Other"]), "Quarry")).toBe(false);
  });

  it("treats a missing list as detecting nothing, not as unknown", () => {
    // The server omits `contacts` when empty, so absent means blind. Treating
    // it as "unknown, show everything" would make a fully blind ship look
    // omniscient.
    expect(hasContact(ship("Seeker"), "Quarry")).toBe(false);
  });

  it("always detects itself", () => {
    expect(hasContact(ship("Seeker"), "Seeker")).toBe(true);
  });

  it("detects everything when there is no observer at all", () => {
    expect(hasContact(null, "Quarry")).toBe(true);
  });
});

describe("isUndetected", () => {
  it("hides nothing in the all-ships view, where no ship is doing the looking", () => {
    expect(isUndetected(null, "Quarry")).toBe(false);
    expect(isUndetected(undefined, "Quarry")).toBe(false);
  });

  it("hides a ship the observer has no contact on", () => {
    expect(isUndetected(ship("Seeker", []), "Quarry")).toBe(true);
  });

  it("does not hide one it can see", () => {
    expect(isUndetected(ship("Seeker", ["Quarry"]), "Quarry")).toBe(false);
  });

  it("never hides the observer from itself", () => {
    expect(isUndetected(ship("Seeker", []), "Seeker")).toBe(false);
  });
});

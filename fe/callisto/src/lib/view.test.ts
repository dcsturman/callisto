import {describe, expect, test} from "vitest";
import {CUSTOMISABLE_ROLES, ViewMode, hasRole, isReferee, parseRoles, rolesToString} from "lib/view";

describe("hasRole", () => {
  test("a single role sees only its own station", () => {
    expect(hasRole([ViewMode.Pilot], ViewMode.Pilot)).toBe(true);
    expect(hasRole([ViewMode.Pilot], ViewMode.Gunner)).toBe(false);
  });

  test("two roles see both stations", () => {
    const both = [ViewMode.Captain, ViewMode.Gunner];
    expect(hasRole(both, ViewMode.Captain)).toBe(true);
    expect(hasRole(both, ViewMode.Gunner)).toBe(true);
    expect(hasRole(both, ViewMode.Engineer)).toBe(false);
  });

  test("General is every station, as it always was", () => {
    for (const station of CUSTOMISABLE_ROLES) {
      expect(hasRole([ViewMode.General], station)).toBe(true);
    }
  });

  test("but General is not an Observer", () => {
    // The Observer check hides the controls entirely; General must never trip it.
    expect(hasRole([ViewMode.General], ViewMode.Observer)).toBe(false);
    expect(hasRole([ViewMode.Observer], ViewMode.Observer)).toBe(true);
  });
});

describe("isReferee", () => {
  test("the ship decides, not the role", () => {
    expect(isReferee([ViewMode.General], null)).toBe(true);
    expect(isReferee([ViewMode.General], "Executor")).toBe(false);
    // A referee who sits at one station to simplify their screen is still
    // the referee.
    expect(isReferee([ViewMode.Pilot], null)).toBe(true);
    expect(isReferee([ViewMode.Pilot], "Executor")).toBe(false);
    // Watching is not refereeing.
    expect(isReferee([ViewMode.Observer], null)).toBe(false);
  });

  test("a fleet commander says Ready rather than running the round", () => {
    expect(isReferee([ViewMode.FleetCommand], null)).toBe(false);
  });
});

describe("Fleet Command", () => {
  test("works every station of the ship it has open, as the referee does", () => {
    expect(hasRole([ViewMode.FleetCommand], ViewMode.Gunner)).toBe(true);
    expect(hasRole([ViewMode.FleetCommand], ViewMode.Observer)).toBe(false);
  });

  test("reads from the wire and names itself", () => {
    expect(parseRoles(["FleetCommand"])).toEqual([ViewMode.FleetCommand]);
    expect(rolesToString([ViewMode.FleetCommand])).toBe("Fleet Command");
  });
});

describe("wire shape", () => {
  test("names join in chosen order", () => {
    expect(rolesToString([ViewMode.Captain, ViewMode.Gunner])).toBe("Captain, Gunner");
  });

  test("a list, a bare name from an older server, or nothing at all", () => {
    expect(parseRoles(["Captain", "Gunner"])).toEqual([ViewMode.Captain, ViewMode.Gunner]);
    expect(parseRoles("Pilot")).toEqual([ViewMode.Pilot]);
    expect(parseRoles(null)).toEqual([ViewMode.General]);
    expect(parseRoles(["not a role"])).toEqual([ViewMode.General]);
  });
});

describe("Squadron Command", () => {
  test("says Ready for its squadron, works its ships' stations, and names itself", () => {
    expect(isReferee([ViewMode.SquadronCommand], null)).toBe(false);
    expect(hasRole([ViewMode.SquadronCommand], ViewMode.Pilot)).toBe(true);
    expect(parseRoles(["SquadronCommand"])).toEqual([ViewMode.SquadronCommand]);
    expect(rolesToString([ViewMode.SquadronCommand])).toBe("Squadron Command");
  });
});

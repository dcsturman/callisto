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
  test("General with no ship runs the board; anything else does not", () => {
    expect(isReferee([ViewMode.General], null)).toBe(true);
    expect(isReferee([ViewMode.General], "Executor")).toBe(false);
    expect(isReferee([ViewMode.Pilot], null)).toBe(false);
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

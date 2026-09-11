import { describe, it, expect } from "vitest";
import { TEAMS, TEAM_CSS, teamBodyColor } from "lib/teams";

describe("teams", () => {
  it("caps at four", () => {
    expect(TEAMS).toHaveLength(4);
    expect(new Set(TEAMS).size).toBe(4);
  });

  it("gives every team a display colour", () => {
    TEAMS.forEach((t) => expect(TEAM_CSS[t]).toMatch(/^#[0-9a-f]{6}$/i));
  });

  it("falls back to the unaligned colour with no team", () => {
    expect(teamBodyColor(null, 1)).toEqual([10, 10, 24]);
    expect(teamBodyColor(undefined, 1)).toEqual([10, 10, 24]);
  });

  it("scales for detection state without changing hue", () => {
    const full = teamBodyColor("Red", 1);
    const dim = teamBodyColor("Red", 0.2);
    // Same hue: every channel scaled by the same factor, so the ratios hold.
    expect(dim[0] / full[0]).toBeCloseTo(0.2);
    expect(dim[1] / full[1]).toBeCloseTo(0.2);
    expect(dim[2] / full[2]).toBeCloseTo(0.2);
  });

  it("keeps teams distinguishable from one another", () => {
    const seen = TEAMS.map((t) => teamBodyColor(t, 1).join(","));
    expect(new Set(seen).size).toBe(TEAMS.length);
  });
});

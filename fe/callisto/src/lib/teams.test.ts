import { describe, it, expect } from "vitest";
import {
  TEAMS,
  TEAM_CSS,
  teamBodyColor,
  teamLabelColor,
  NO_CONTACT_LABEL,
} from "lib/teams";

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

describe("teamLabelColor", () => {
  it("shows the team colour for a detected ship", () => {
    expect(teamLabelColor("Red", {})).toBe(TEAM_CSS.Red);
  });

  it("greys a ship with no sensor contact, whatever side it is on", () => {
    // Detection wins over team: "can I act on this" is the more urgent
    // question than "whose is it".
    expect(teamLabelColor("Red", {undetected: true})).toBe(NO_CONTACT_LABEL);
    expect(teamLabelColor(null, {undetected: true})).toBe(NO_CONTACT_LABEL);
  });

  it("dims a detected ship that is not the one being flown", () => {
    const full = teamLabelColor("Blue", {});
    const dimmed = teamLabelColor("Blue", {dim: true});
    expect(dimmed).not.toBe(full);
    expect(dimmed).toMatch(/^#[0-9a-f]{6}$/i);
    // Same hue, lower value: every channel pulled towards black.
    const chan = (h: string) =>
      [1, 3, 5].map((i) => parseInt(h.slice(i, i + 2), 16));
    chan(dimmed).forEach((c, i) => expect(c).toBeLessThanOrEqual(chan(full)[i]));
  });

  it("uses white for an unaligned ship, not a green that clashes with team Green", () => {
    const unaligned = teamLabelColor(null, {});
    expect(unaligned).toBe(teamLabelColor(undefined, {}));
    expect(unaligned).not.toBe(TEAM_CSS.Green);
    // Neutral: all three channels equal, so it reads as the absence of a team
    // colour rather than as one of them.
    const [r, g, b] = [1, 3, 5].map((i) => parseInt(unaligned.slice(i, i + 2), 16));
    expect(r).toBe(g);
    expect(g).toBe(b);
  });
});

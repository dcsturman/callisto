/**
 * Which side a ship is on.
 *
 * Capped at four and named for colours rather than numbers, so a referee
 * reading "Red" on a dropdown and seeing a red ship in the view needs no
 * translation step. Values match the Rust `Team` enum on the wire.
 */
export type Team = "Red" | "Blue" | "Green" | "Gold";

export const TEAMS: Team[] = ["Red", "Blue", "Green", "Gold"];

/**
 * Display colours, as HDR triples for the 3D view.
 *
 * The scene runs a bloom pass over a half-float buffer, so values above 1 set
 * how hard a ship blooms rather than just how bright it looks. These are scaled
 * to bloom about as hard as the unaligned blue-white default did, so adding a
 * team does not make a ship shout.
 *
 * Hue carries the team; brightness and label colour carry detection state. The
 * two have to compose, which is why these are returned as a base to be scaled
 * rather than as finished colours.
 */
const TEAM_HDR: Record<Team, [number, number, number]> = {
  Red: [24, 6, 6],
  Blue: [6, 10, 24],
  Green: [6, 22, 8],
  Gold: [24, 18, 5],
};

/** The unaligned default: the blue-white every ship used before teams. */
const UNALIGNED_HDR: [number, number, number] = [10, 10, 24];

/** Flat CSS colours for 2D chrome -- dropdowns, the roster, labels. */
export const TEAM_CSS: Record<Team, string> = {
  Red: "#ff6b6b",
  Blue: "#6ba3ff",
  Green: "#5ddc7a",
  Gold: "#ffc94d",
};

/**
 * The 3D body colour for a ship, scaled for how well it is currently seen.
 *
 * `scale` is the detection treatment: 1 for your own ship and for the
 * all-ships view, lower for a ship that is merely detected, lower still for one
 * that is not. Scaling rather than substituting keeps the team readable at
 * every brightness.
 */
export const teamBodyColor = (
  team: Team | null | undefined,
  scale: number,
): [number, number, number] => {
  const base = team == null ? UNALIGNED_HDR : TEAM_HDR[team];
  return [base[0] * scale, base[1] * scale, base[2] * scale];
};

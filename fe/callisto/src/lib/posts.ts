import { EntityList } from "lib/entities";

/** A Hold side's post as drawn: where it is now, how far it reaches, whose. */
export interface DrawnPost {
  key: string;
  position: [number, number, number];
  /** Metres. */
  radius: number;
  team: string | null;
  label: string;
}

/** Every post a Hold ship names, once each, resolved to where it is now. */
export const postsToDraw = (entities: EntityList): DrawnPost[] => {
  const posts = new Map<string, DrawnPost>();
  for (const ship of entities.ships) {
    if (ship.goal !== "hold" || ship.post == null) {
      continue;
    }
    const position =
      typeof ship.post === "string"
        ? entities.planets.find((planet) => planet.name === ship.post)?.position
        : ship.post;
    if (position == null) {
      continue;
    }
    const key = `${ship.team ?? ship.name}:${JSON.stringify(ship.post)}`;
    if (!posts.has(key)) {
      const where = typeof ship.post === "string" ? ship.post : "post";
      posts.set(key, {
        key,
        position,
        radius: (ship.post_radius ?? 25_000) * 1000,
        team: ship.team ?? null,
        label: ship.team != null ? `${ship.team} holds ${where}` : `${ship.name} holds ${where}`,
      });
    }
  }
  return [...posts.values()];
};

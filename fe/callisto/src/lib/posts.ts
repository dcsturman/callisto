import { EntityList, Ship } from "lib/entities";

/** The colour of a Hold post, on the map and in the ship list. */
export const POST_COLOUR = "#b57cff";

/** Where a ship's post is now, and its radius in metres; null if it names
 *  none, or a body the scenario does not have. */
export const postOf = (
  entities: EntityList,
  ship: Ship,
): { position: [number, number, number]; radius: number; where: string } | null => {
  if (ship.post == null) {
    return null;
  }
  const position =
    typeof ship.post === "string"
      ? entities.planets.find((planet) => planet.name === ship.post)?.position
      : ship.post;
  if (position == null) {
    return null;
  }
  return {
    position,
    radius: (ship.post_radius ?? 25_000) * 1000,
    where: typeof ship.post === "string" ? ship.post : "its post",
  };
};

/** How far a Hold ship is from her post, in metres, and whether that is
 *  within its radius; null for a ship with no post. */
export const fromPost = (
  entities: EntityList,
  ship: Ship,
): { distance: number; inside: boolean } | null => {
  const post = ship.goal === "hold" ? postOf(entities, ship) : null;
  if (post == null) {
    return null;
  }
  const [ax, ay, az] = ship.position;
  const [bx, by, bz] = post.position;
  const distance = Math.hypot(ax - bx, ay - by, az - bz);
  return { distance, inside: distance <= post.radius };
};

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
    const post = postOf(entities, ship);
    if (post == null) {
      continue;
    }
    const key = `${ship.team ?? ship.name}:${JSON.stringify(ship.post)}`;
    if (!posts.has(key)) {
      const where = typeof ship.post === "string" ? ship.post : "";
      const whose = ship.team ?? ship.name;
      posts.set(key, {
        key,
        position: post.position,
        radius: post.radius,
        team: ship.team ?? null,
        label: where ? `${whose}'s post: ${where}` : `${whose}'s post`,
      });
    }
  }
  return [...posts.values()];
};

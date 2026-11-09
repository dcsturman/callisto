import { Missile } from "lib/entities";
import { SCALE } from "lib/universal";

/** Every missile's place in the scene, three numbers each, for drawing a
 *  swarm of them as one instanced mesh. */
export const missilePositions = (missiles: Missile[]): Float32Array => {
  const at = new Float32Array(missiles.length * 3);
  missiles.forEach((missile, i) => {
    at[i * 3] = missile.position[0] * SCALE;
    at[i * 3 + 1] = missile.position[1] * SCALE;
    at[i * 3 + 2] = missile.position[2] * SCALE;
  });
  return at;
};

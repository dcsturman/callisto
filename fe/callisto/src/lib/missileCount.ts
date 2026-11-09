/**
 * A launch from one mount flies as one salvo carrying its count, where every
 * missile used to be its own object. Everything that tells a crew how many
 * are coming counts missiles, through these, not the objects they fly in.
 *
 * Kept apart from lib/entities so the components that count missiles do not
 * pull the whole of it in at load.
 */

/** How many missiles a salvo carries: one when the server says nothing. */
export const missileCount = (missile: { count?: number }): number => missile.count ?? 1;

/** Every missile in these salvoes. */
export const missilesIn = (salvoes: { count?: number }[]): number =>
  salvoes.reduce((sum, salvo) => sum + missileCount(salvo), 0);

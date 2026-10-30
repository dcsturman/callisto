/** Chance that 2D plus `dm` reaches `target` (8 by default), as a percentage. */
export const chanceOf = (dm: number, target: number = 8): number => {
  // 2D outcomes, 2..12, out of 36.
  const ways = [0, 0, 1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];
  let hits = 0;
  for (let roll = 2; roll <= 12; roll++) {
    if (roll + dm >= target) {
      hits += ways[roll];
    }
  }
  return Math.round((hits / 36) * 100);
};

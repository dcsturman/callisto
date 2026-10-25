import { describe, it, expect } from "vitest";
import { Ship } from "lib/entities";
import { outcome, rememberSides, timeUp } from "lib/victory";
import { EntityList } from "lib/entities";

const ship = (name: string, team?: string, goal?: string): Ship => ({ name, team, goal }) as unknown as Ship;

const start = [ship("Executor", "Red"), ship("Cicada", "Blue"), ship("Invigilator", "Blue")];
const seen = rememberSides(new Map(), start);

describe("outcome", () => {
  it("is nothing while two sides have ships", () => {
    expect(outcome(seen, start)).toBeNull();
  });

  it("names the side left standing", () => {
    const ended = outcome(seen, [ship("Executor", "Red")]);
    expect(ended?.headline).toBe("Red Team Victorious!");
    expect(ended?.gone.map((side) => side.label)).toEqual(["Blue Team"]);
  });

  it("calls it mutual destruction when nobody is left", () => {
    expect(outcome(seen, [])?.headline).toBe("Mutual Destruction");
  });

  it("calls a side that set out to escape and is gone an escape, not a defeat", () => {
    const convoy = rememberSides(new Map(), [ship("Trader", "Blue", "escape"), ship("Corsair", "Red")]);
    expect(outcome(convoy, [ship("Corsair", "Red")])?.headline).toBe("Blue Team escaped!");
  });

  it("needs two sides to have fought: a scenario of one side has no winner", () => {
    const alone = rememberSides(new Map(), [ship("Executor", "Red")]);
    expect(outcome(alone, [ship("Executor", "Red")])).toBeNull();
  });

  it("counts each unaligned ship as a side of its own", () => {
    const free = rememberSides(new Map(), [ship("Hulk"), ship("Trader")]);
    expect(outcome(free, [ship("Trader")])?.headline).toBe("Trader Victorious!");
  });
});

describe("timeUp", () => {
  const aslan = (z: number) => ({ name: "Ailreaui", team: "Red", goal: "hold", post: "Europa", position: [0, 0, z] });
  const gazelle = { name: "Unicorn", team: "Blue", goal: "destroy", position: [0, 0, 5e8] };
  const world = (round: number, z: number): EntityList =>
    ({
      ships: [aslan(z), gazelle],
      planets: [{ name: "Europa", position: [0, 0, 6.71e8] }],
      missiles: [],
      metadata: { name: "Jupiter", description: "", rounds: 30 },
      round,
    }) as unknown as EntityList;
  const sides = (w: EntityList) => rememberSides(new Map(), w.ships);

  it("is nothing while rounds remain", () => {
    const w = world(30, 6.71e8);
    expect(timeUp(w, sides(w))).toBeNull();
  });

  it("gives the game to a holder still at its post when the rounds are up", () => {
    const w = world(31, 6.71e8 - 5e6);
    expect(timeUp(w, sides(w))?.headline).toBe("Red Team held Europa!");
  });

  it("gives it to the attacker if the post has fallen", () => {
    const w = world(31, 6.71e8 - 9e7);
    expect(timeUp(w, sides(w))?.headline).toBe("Blue Team Victorious!");
  });
});

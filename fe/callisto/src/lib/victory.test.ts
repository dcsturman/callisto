import { describe, it, expect } from "vitest";
import { Ship } from "lib/entities";
import { escapeNeeded, outcome, rememberSides, timeUp } from "lib/victory";
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

  it("calls a side that set out to escape and got away an escape", () => {
    const convoy = rememberSides(new Map(), [ship("Trader", "Blue", "escape"), ship("Corsair", "Red")]);
    const ended = outcome(convoy, [ship("Corsair", "Red")], {
      departed: [{ name: "Trader", team: "Blue", safe: true }],
    });
    expect(ended?.headline).toBe("Blue Team escaped!");
    expect(ended?.detail).toBe("1 of 1 Blue Team ship got away.");
  });

  it("does not call an escaping side that was destroyed an escape", () => {
    const convoy = rememberSides(new Map(), [ship("Trader", "Blue", "escape"), ship("Corsair", "Red")]);
    const ended = outcome(convoy, [ship("Corsair", "Red")]);
    expect(ended?.headline).toBe("Red Team Victorious!");
    expect(ended?.detail).toBe("Only 0 of 1 Blue Team ship got away, of the 1 needed.");
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

describe("escapes", () => {
  const convoy = (n: number) => [...Array(n).keys()].map((i) => ship(`T${i}`, "Blue", "escape"));
  const jumped = (names: string[], safe = true) => names.map((name) => ({ name, team: "Blue", safe }));
  const fourAndRaider = rememberSides(new Map(), [...convoy(4), ship("Corsair", "Red")]);

  it("needs more than two-thirds of the side to get away", () => {
    expect(escapeNeeded(3)).toBe(3);
    expect(escapeNeeded(4)).toBe(3);
    expect(escapeNeeded(6)).toBe(5);
    expect(escapeNeeded(1)).toBe(1);
  });

  it("takes the scenario's own number when it sets one", () => {
    expect(escapeNeeded(6, { metadata: { escape_needed: 2 } })).toBe(2);
    expect(escapeNeeded(2, { metadata: { escape_needed: 5 } })).toBe(2);
  });

  it("is an escape when enough got away, and a loss when too few did", () => {
    const remains = [ship("Corsair", "Red")];
    expect(outcome(fourAndRaider, remains, { departed: jumped(["T0", "T1", "T2"]) })?.headline).toBe(
      "Blue Team escaped!",
    );
    expect(outcome(fourAndRaider, remains, { departed: jumped(["T0", "T1"]) })?.headline).toBe(
      "Red Team Victorious!",
    );
  });

  it("does not count a misjump as getting away", () => {
    const departed = [...jumped(["T0", "T1"]), ...jumped(["T2"], false)];
    expect(outcome(fourAndRaider, [ship("Corsair", "Red")], { departed })?.headline).toBe("Red Team Victorious!");
  });

  const atTimeUp = (inFight: string[], away: string[]): EntityList =>
    ({
      ships: [...inFight.map((name) => ship(name, "Blue", "escape")), ship("Corsair", "Red")],
      planets: [],
      missiles: [],
      metadata: { name: "", description: "", rounds: 10 },
      filename: "",
      round: 11,
      departed: jumped(away),
    }) as unknown as EntityList;

  it("on time, is an escape when enough have already got away", () => {
    expect(timeUp(atTimeUp(["T3"], ["T0", "T1", "T2"]), fourAndRaider)?.headline).toBe("Blue Team escaped!");
  });

  it("on time, is a draw when enough are alive but not enough got away", () => {
    const ended = timeUp(atTimeUp(["T2", "T3"], ["T0"]), fourAndRaider);
    expect(ended?.headline).toBe("A draw");
    expect(ended?.winner).toBeNull();
  });

  it("on time, is the attackers' when too few are left to make it", () => {
    expect(timeUp(atTimeUp(["T3"], ["T0"]), fourAndRaider)?.headline).toBe("Red Team Victorious!");
  });
});

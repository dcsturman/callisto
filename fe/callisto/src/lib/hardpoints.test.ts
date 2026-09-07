import { describe, it, expect } from "vitest";
import {
  MOUNT_OPTIONS,
  WeaponGroup,
  allowanceForDisplacement,
  checkAllowance,
  commonGunnery,
  emptyGroup,
  expandGroups,
  groupWeapons,
  mountCost,
  mountForOptionId,
  mountOptionId,
  mountOptionsFor,
  setAllGunnery,
  totalMounts,
} from "lib/hardpoints";
import { Weapon, WeaponMount, createWeapon } from "lib/weapon";

// Editor rows.
const turret = (size: number, kind = "Beam", count = 1): WeaponGroup => ({
  count,
  mount: { Turret: size },
  kind,
  gunnery: 0,
});
const bay = (
  size: "Small" | "Medium" | "Large",
  kind = "Missile",
  count = 1,
): WeaponGroup => ({ count, mount: { Bay: size }, kind, gunnery: 0 });
const barbette = (kind = "Particle", count = 1): WeaponGroup => ({
  count,
  mount: "Barbette",
  kind,
  gunnery: 0,
});
const fixed = (kind = "Missile", count = 1): WeaponGroup => ({
  count,
  mount: "FixedMount",
  kind,
  gunnery: 0,
});

// Flat weapons, as they arrive from a design or an existing ship.
const wTurret = (size: number, kind = "Beam"): Weapon =>
  createWeapon(kind, { Turret: size });
const wBay = (size: "Small" | "Medium" | "Large", kind = "Missile"): Weapon =>
  createWeapon(kind, { Bay: size });

describe("allowanceForDisplacement", () => {
  it("gives ships of 100 tons or more one hardpoint per 100 tons", () => {
    expect(allowanceForDisplacement(100)).toEqual({
      kind: "hardpoints",
      total: 1,
    });
    expect(allowanceForDisplacement(200)).toEqual({
      kind: "hardpoints",
      total: 2,
    });
    expect(allowanceForDisplacement(5000)).toEqual({
      kind: "hardpoints",
      total: 50,
    });
  });

  it("rounds partial hundreds down", () => {
    expect(allowanceForDisplacement(199).total).toBe(1);
    expect(allowanceForDisplacement(1800).total).toBe(18);
  });

  it("gives craft under 100 tons firmpoints on the three small-craft bands", () => {
    expect(allowanceForDisplacement(10)).toEqual({
      kind: "firmpoints",
      total: 1,
    });
    expect(allowanceForDisplacement(34).total).toBe(1);
    expect(allowanceForDisplacement(35).total).toBe(2);
    expect(allowanceForDisplacement(69).total).toBe(2);
    expect(allowanceForDisplacement(70).total).toBe(3);
    expect(allowanceForDisplacement(99).total).toBe(3);
  });
});

describe("mountCost", () => {
  it("charges one hardpoint per turret regardless of size", () => {
    expect(mountCost({ Turret: 1 }, "hardpoints")).toBe(1);
    expect(mountCost({ Turret: 2 }, "hardpoints")).toBe(1);
    expect(mountCost({ Turret: 3 }, "hardpoints")).toBe(1);
  });

  it("charges one for a fixed mount, a barbette and the smaller bays", () => {
    expect(mountCost("FixedMount", "hardpoints")).toBe(1);
    expect(mountCost("Barbette", "hardpoints")).toBe(1);
    expect(mountCost({ Bay: "Small" }, "hardpoints")).toBe(1);
    expect(mountCost({ Bay: "Medium" }, "hardpoints")).toBe(1);
  });

  it("charges five for a large bay", () => {
    expect(mountCost({ Bay: "Large" }, "hardpoints")).toBe(5);
    expect(mountCost({ Bay: "Large" }, "firmpoints")).toBe(5);
  });

  it("charges three firmpoints for a barbette on a small craft", () => {
    expect(mountCost("Barbette", "firmpoints")).toBe(3);
  });
});

describe("checkAllowance on ships of 100 tons or more", () => {
  it("accepts a Free Trader: 200 tons, two turrets, two hardpoints", () => {
    const report = checkAllowance([turret(2, "Pulse"), turret(2, "Sand")], 200);
    expect(report.used).toBe(2);
    expect(report.overAllowance).toBe(false);
    expect(report.problems).toEqual([]);
    expect(report.rowProblems).toEqual([null, null]);
  });

  it("charges a group once per mount in it", () => {
    // Two rows, but eight turrets between them.
    const report = checkAllowance(
      [turret(3, "Beam", 6), turret(3, "Sand", 2)],
      1000,
    );
    expect(report.used).toBe(8);
    expect(report.overAllowance).toBe(false);
  });

  it("scores the P.F. Sloan: 34 mounts in three rows on 50 hardpoints", () => {
    const report = checkAllowance(
      [bay("Small", "Missile", 2), turret(3, "Beam", 30), turret(3, "Pulse", 2)],
      5000,
    );
    expect(report.used).toBe(34);
    expect(report.overAllowance).toBe(false);
    expect(report.problems).toEqual([]);
  });

  it("counts empty and zero-count rows as costing nothing", () => {
    const report = checkAllowance(
      [turret(3), emptyGroup(), turret(1, "Sand", 0)],
      300,
    );
    expect(report.used).toBe(1);
    expect(report.overAllowance).toBe(false);
  });

  it("flags a large bay that eats more hardpoints than the hull has", () => {
    // 200 tons is 2 hardpoints; a Large Bay alone costs 5.
    const report = checkAllowance([bay("Large")], 200);
    expect(report.used).toBe(5);
    expect(report.overAllowance).toBe(true);
    expect(report.rowProblems[0]).toContain("2 hardpoints");
  });

  it("fits a large bay on a hull big enough for it", () => {
    // 600 tons is 6 hardpoints: a Large Bay (5) plus one turret.
    const report = checkAllowance([bay("Large"), turret(3)], 600);
    expect(report.used).toBe(6);
    expect(report.overAllowance).toBe(false);
  });

  // `excelsior` is really a barbette plus one mixed triple turret (2
  // hardpoints), but WeaponMount cannot express a mixed turret so it is stored
  // as three mounts. It must render and report, not crash or lose a mount.
  it("reports the excelsior over-allowance without dropping any mount", () => {
    const groups = [barbette(), turret(2, "Missile"), turret(1, "Sand")];
    const report = checkAllowance(groups, 200);
    expect(report.used).toBe(3);
    expect(report.overAllowance).toBe(true);
    expect(report.rowProblems).toHaveLength(3);
    // The overrun is charged to the last row, so the legal ones stay clean.
    expect(report.rowProblems[0]).toBeNull();
    expect(report.rowProblems[1]).toBeNull();
    expect(report.rowProblems[2]).not.toBeNull();
  });
});

describe("checkAllowance on small craft", () => {
  it("accepts a Heavy Fighter: 50 tons, a single turret and a fixed mount", () => {
    const report = checkAllowance([turret(1, "Beam"), fixed()], 50);
    expect(report.allowance).toEqual({ kind: "firmpoints", total: 2 });
    expect(report.used).toBe(2);
    expect(report.overAllowance).toBe(false);
    expect(report.problems).toEqual([]);
  });

  it("rejects a second turret", () => {
    const report = checkAllowance([turret(1, "Beam"), turret(1, "Sand")], 70);
    expect(report.overAllowance).toBe(false);
    expect(report.rowProblems[0]).toBeNull();
    expect(report.rowProblems[1]).toContain("one Firmpoint");
    expect(report.problems.join(" ")).toContain(
      "only one Firmpoint may be a turret",
    );
  });

  // The turret limit counts mounts, not rows: two turrets in one group are
  // still two turrets.
  it("rejects a group of two turrets on one small craft", () => {
    const report = checkAllowance([turret(1, "Beam", 2)], 70);
    expect(report.rowProblems[0]).toContain("one Firmpoint");
    expect(report.problems.join(" ")).toContain("2 turrets");
  });

  it("rejects a double or triple turret", () => {
    const double = checkAllowance([turret(2)], 50);
    expect(double.rowProblems[0]).toContain("single turret");

    const triple = checkAllowance([turret(3)], 50);
    expect(triple.rowProblems[0]).toContain("single turret");
  });

  it("charges a barbette three firmpoints", () => {
    // A 50-ton craft has 2 firmpoints, so a barbette alone overruns it.
    const small = checkAllowance([barbette()], 50);
    expect(small.used).toBe(3);
    expect(small.overAllowance).toBe(true);

    // A 70-ton craft has exactly 3, so a barbette alone fits and fills it.
    const exact = checkAllowance([barbette()], 70);
    expect(exact.used).toBe(3);
    expect(exact.overAllowance).toBe(false);

    // ...and leaves no room for anything else.
    const overfull = checkAllowance([barbette(), fixed()], 70);
    expect(overfull.used).toBe(4);
    expect(overfull.overAllowance).toBe(true);
  });

  it("allows any number of fixed mounts up to the allowance", () => {
    const report = checkAllowance([fixed("Missile", 3)], 70);
    expect(report.used).toBe(3);
    expect(report.overAllowance).toBe(false);
    expect(report.problems).toEqual([]);
  });
});

describe("groupWeapons", () => {
  it("collapses the P.F. Sloan's 34 weapons into three rows", () => {
    const weapons = [
      wBay("Small"),
      wBay("Small"),
      ...Array(30).fill(wTurret(3, "Beam")),
      wTurret(3, "Pulse"),
      wTurret(3, "Pulse"),
    ];
    const groups = groupWeapons(weapons);
    expect(groups).toHaveLength(3);
    expect(groups.map((group) => group.count)).toEqual([2, 30, 2]);
    expect(totalMounts(groups)).toBe(34);
  });

  it("defaults gunnery to zero where the crew list is short or absent", () => {
    const groups = groupWeapons([wTurret(3), wTurret(3)], [2]);
    // Different skills, so the two turrets do not merge.
    expect(groups).toHaveLength(2);
    expect(groups.map((group) => group.gunnery)).toEqual([2, 0]);
  });

  it("keeps a differently-skilled gunner in a row of their own", () => {
    const weapons = [wTurret(3), wTurret(3), wTurret(3)];
    const groups = groupWeapons(weapons, [1, 3, 1]);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toMatchObject({ count: 2, gunnery: 1 });
    expect(groups[1]).toMatchObject({ count: 1, gunnery: 3 });
  });

  it("merges identical weapons that are not adjacent", () => {
    const groups = groupWeapons([wTurret(3, "Beam"), wTurret(3, "Sand"), wTurret(3, "Beam")]);
    expect(groups).toHaveLength(2);
    expect(groups[0]).toMatchObject({ kind: "Beam", count: 2 });
    expect(groups[1]).toMatchObject({ kind: "Sand", count: 1 });
  });

  it("returns nothing for an unarmed ship", () => {
    expect(groupWeapons([])).toEqual([]);
    expect(totalMounts([])).toBe(0);
  });
});

describe("expandGroups", () => {
  it("produces weapons and gunnery index-aligned, which is what weapon_id needs", () => {
    const { weapons, gunnery } = expandGroups([
      { count: 2, mount: { Bay: "Small" }, kind: "Missile", gunnery: 3 },
      { count: 3, mount: { Turret: 3 }, kind: "Beam", gunnery: 1 },
    ]);
    expect(weapons).toHaveLength(5);
    expect(gunnery).toEqual([3, 3, 1, 1, 1]);
    expect(weapons[0]).toEqual(createWeapon("Missile", { Bay: "Small" }));
    expect(weapons[4]).toEqual(createWeapon("Beam", { Turret: 3 }));
  });

  it("drops empty and zero-count rows", () => {
    const { weapons, gunnery } = expandGroups([
      emptyGroup(),
      turret(3, "Beam", 0),
      turret(1, "Sand", 1),
    ]);
    expect(weapons).toHaveLength(1);
    expect(gunnery).toHaveLength(1);
  });

  it("round-trips a grouped armament back to the list it came from", () => {
    const weapons = [
      wTurret(3, "Beam"),
      wTurret(3, "Beam"),
      wBay("Medium", "Missile"),
    ];
    const skills = [2, 2, 4];
    const expanded = expandGroups(groupWeapons(weapons, skills));
    expect(expanded.weapons).toEqual(weapons);
    expect(expanded.gunnery).toEqual(skills);
  });
});

describe("bulk gunner skill", () => {
  it("reports the shared skill when every mount agrees", () => {
    const groups = [
      { ...turret(3, "Beam", 4), gunnery: 2 },
      { ...turret(3, "Sand", 2), gunnery: 2 },
    ];
    expect(commonGunnery(groups)).toBe(2);
  });

  it("reports nothing once a row is overridden", () => {
    const groups = [
      { ...turret(3, "Beam", 4), gunnery: 2 },
      { ...turret(3, "Sand", 2), gunnery: 3 },
    ];
    expect(commonGunnery(groups)).toBeNull();
  });

  it("ignores empty rows when deciding whether the crew agrees", () => {
    const groups = [{ ...turret(3, "Beam"), gunnery: 2 }, emptyGroup(0)];
    expect(commonGunnery(groups)).toBe(2);
  });

  // With nothing armed the field still has a job: it shows the skill the next
  // row will inherit.  Returning null there would render it empty and make
  // every keystroke compute back to empty.
  it("falls back to the pending rows when nothing is armed yet", () => {
    expect(commonGunnery([emptyGroup(2)])).toBe(2);
    expect(commonGunnery([])).toBeNull();
  });

  it("still answers when a row is mid-edit at a count of zero", () => {
    const groups = [{ ...turret(3, "Beam", 0), gunnery: 4 }, emptyGroup(4)];
    expect(commonGunnery(groups)).toBe(4);
  });

  it("writes one skill through to every row", () => {
    const groups = setAllGunnery([turret(3), bay("Large"), emptyGroup()], 3);
    expect(groups.map((group) => group.gunnery)).toEqual([3, 3, 3]);
  });
});

describe("mountOptionsFor", () => {
  it("offers every mount on a hull with hardpoints", () => {
    expect(mountOptionsFor("hardpoints")).toEqual(MOUNT_OPTIONS);
  });

  it("offers small craft only what a firmpoint can carry", () => {
    expect(mountOptionsFor("firmpoints").map((option) => option.label)).toEqual([
      "None",
      "Fixed Mount",
      "Single Turret",
      "Barbette",
    ]);
  });

  it("keeps doubles, triples and bays off small craft entirely", () => {
    const ids = mountOptionsFor("firmpoints").map((option) => option.id);
    expect(ids).not.toContain("turret-2");
    expect(ids).not.toContain("turret-3");
    expect(ids.some((id) => id.startsWith("bay-"))).toBe(false);
  });
});

describe("mount dropdown options", () => {
  it("offers every mount the editor must support", () => {
    expect(MOUNT_OPTIONS.map((option) => option.label)).toEqual([
      "None",
      "Fixed Mount",
      "Single Turret",
      "Double Turret",
      "Triple Turret",
      "Barbette",
      "Small Bay",
      "Medium Bay",
      "Large Bay",
    ]);
  });

  it("round-trips every option between id and mount", () => {
    MOUNT_OPTIONS.forEach((option) => {
      expect(mountForOptionId(option.id)).toEqual(option.mount);
      expect(mountOptionId(option.mount)).toBe(option.id);
    });
  });

  it("maps an empty row to the None option", () => {
    expect(mountOptionId(null)).toBe("none");
    expect(mountForOptionId("none")).toBeNull();
  });

  it("returns null for a mount it cannot represent", () => {
    expect(mountOptionId({ Turret: 4 } as WeaponMount)).toBeNull();
    expect(mountOptionId("Spinal" as WeaponMount)).toBeNull();
  });
});

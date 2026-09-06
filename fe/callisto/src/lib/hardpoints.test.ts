import { describe, it, expect } from "vitest";
import {
  MOUNT_OPTIONS,
  allowanceForDisplacement,
  checkAllowance,
  compactWeaponRows,
  mountCost,
  mountForOptionId,
  mountOptionId,
  padWeaponRows,
  rowCountForDesign,
} from "lib/hardpoints";
import { Weapon, WeaponMount, createWeapon } from "lib/weapon";

const turret = (size: number, kind = "Beam"): Weapon =>
  createWeapon(kind, { Turret: size });
const bay = (size: "Small" | "Medium" | "Large", kind = "Missile"): Weapon =>
  createWeapon(kind, { Bay: size });
const barbette = (kind = "Particle"): Weapon => createWeapon(kind, "Barbette");
const fixed = (kind = "Missile"): Weapon => createWeapon(kind, "FixedMount");

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
    expect(allowanceForDisplacement(300)).toEqual({
      kind: "hardpoints",
      total: 3,
    });
    expect(allowanceForDisplacement(2000)).toEqual({
      kind: "hardpoints",
      total: 20,
    });
  });

  it("rounds partial hundreds down", () => {
    expect(allowanceForDisplacement(199)).toEqual({
      kind: "hardpoints",
      total: 1,
    });
    expect(allowanceForDisplacement(450)).toEqual({
      kind: "hardpoints",
      total: 4,
    });
  });

  // The small-craft bands are the part that is NOT floor(tons / 100): every
  // one of these hulls would get zero mounts under that formula.
  it("gives craft under 100 tons firmpoints on the three small-craft bands", () => {
    expect(allowanceForDisplacement(6)).toEqual({ kind: "firmpoints", total: 1 });
    expect(allowanceForDisplacement(34)).toEqual({ kind: "firmpoints", total: 1 });
    expect(allowanceForDisplacement(35)).toEqual({ kind: "firmpoints", total: 2 });
    expect(allowanceForDisplacement(50)).toEqual({ kind: "firmpoints", total: 2 });
    expect(allowanceForDisplacement(69)).toEqual({ kind: "firmpoints", total: 2 });
    expect(allowanceForDisplacement(70)).toEqual({ kind: "firmpoints", total: 3 });
    expect(allowanceForDisplacement(99)).toEqual({ kind: "firmpoints", total: 3 });
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

  it("counts empty rows as costing nothing", () => {
    const report = checkAllowance([turret(3), null, null], 300);
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
    const weapons = [barbette(), turret(2, "Missile"), turret(1, "Sand")];
    const report = checkAllowance(weapons, 200);
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
    expect(report.problems.join(" ")).toContain("only one Firmpoint may be a turret");
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
    const report = checkAllowance([fixed(), fixed(), fixed()], 70);
    expect(report.used).toBe(3);
    expect(report.overAllowance).toBe(false);
    expect(report.problems).toEqual([]);
  });
});

describe("row helpers", () => {
  it("gives one row per point of allowance", () => {
    expect(rowCountForDesign(200, 2)).toBe(2);
    expect(rowCountForDesign(400, 0)).toBe(4);
    expect(rowCountForDesign(6, 1)).toBe(1);
    expect(rowCountForDesign(50, 0)).toBe(2);
  });

  it("gives extra rows to a design that already exceeds its allowance", () => {
    // excelsior: 200 tons (2 hardpoints) but 3 stored mounts.
    expect(rowCountForDesign(200, 3)).toBe(3);
  });

  it("pads short armaments and never truncates long ones", () => {
    expect(padWeaponRows([turret(1)], 3)).toEqual([turret(1), null, null]);
    expect(padWeaponRows([turret(1), turret(2), turret(3)], 2)).toHaveLength(3);
    expect(padWeaponRows([], 2)).toEqual([null, null]);
  });

  it("compacts rows to a dense list, preserving order", () => {
    const a = turret(1, "Beam");
    const b = turret(3, "Missile");
    expect(compactWeaponRows([null, a, null, b, null])).toEqual([a, b]);
    expect(compactWeaponRows([null, null])).toEqual([]);
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
    for (const option of MOUNT_OPTIONS) {
      expect(mountForOptionId(option.id)).toEqual(option.mount);
      expect(mountOptionId(option.mount)).toBe(option.id);
    }
  });

  it("maps an empty row to the None option", () => {
    expect(mountOptionId(null)).toBe("none");
  });

  // A mount no option covers must be reported rather than silently snapped to
  // some other mount, which would rewrite a design behind the user's back.
  it("returns null for a mount it cannot represent", () => {
    expect(mountOptionId({ Turret: 4 } as WeaponMount)).toBeNull();
    expect(mountOptionId("SpinalMount")).toBeNull();
  });
});

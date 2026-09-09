import { describe, it, expect } from "vitest";
import { describeScreens } from "lib/shipDesignTemplates";
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
  describeGroupGuns,
  gunCapacity,
  setAllGunnery,
  totalMounts,
  isLegalPairing,
  weaponKindsForMount,
} from "lib/hardpoints";
import {
  Weapon,
  WeaponMount,
  createWeapon,
  describeModifiers,
  isActionableWeapon,
  isPassiveWeapon,
  weaponToString,
  weaponKindLabel,
  weaponGuns,
  weaponKinds,
  countOfKind,
  isUniformWeapon,
} from "lib/weapon";

// Editor rows.
const turret = (size: number, kind = "Beam", count = 1): WeaponGroup => ({
  count,
  mount: { Turret: size },
  kind,
  gunnery: 0,
  modifiers: [],
});
const bay = (
  size: "Small" | "Medium" | "Large",
  kind = "Missile",
  count = 1,
): WeaponGroup => ({ count, mount: { Bay: size }, kind, gunnery: 0, modifiers: [] });
const barbette = (kind = "Particle", count = 1): WeaponGroup => ({
  count,
  mount: "Barbette",
  kind,
  gunnery: 0,
  modifiers: [],
});
const fixed = (kind = "Missile", count = 1): WeaponGroup => ({
  count,
  mount: "FixedMount",
  kind,
  gunnery: 0,
  modifiers: [],
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
      { count: 2, mount: { Bay: "Small" }, kind: "Missile", gunnery: 3, modifiers: [] },
      { count: 3, mount: { Turret: 3 }, kind: "Beam", gunnery: 1, modifiers: [] },
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
      "PD Battery (Type I)",
      "PD Battery (Type II)",
      "PD Battery (Type III)",
    ]);
  });

  it("does not offer a battery on a firmpoint hull", () => {
    // A point-defence battery is 20 tons and consumes a Hardpoint, which a hull
    // under 100 tons does not have.
    const ids = mountOptionsFor("firmpoints").map((option) => option.id);
    expect(ids.some((id) => id.startsWith("battery-"))).toBe(false);
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

describe("point defence batteries", () => {
  it("only allows point defence in a battery mount", () => {
    expect(isLegalPairing("PointDefense", { Battery: 3 })).toBe(true);
    expect(isLegalPairing("PointDefense", { Turret: 3 })).toBe(false);
    expect(isLegalPairing("PointDefense", "Barbette")).toBe(false);
  });

  it("only allows a battery mount to hold point defence", () => {
    expect(isLegalPairing("Beam", { Battery: 2 })).toBe(false);
    expect(isLegalPairing("Missile", { Battery: 2 })).toBe(false);
    expect(weaponKindsForMount({ Battery: 3 })).toEqual(["PointDefense"]);
  });

  it("names a battery by its grade rather than its weapon kind", () => {
    expect(weaponToString({ kind: "PointDefense", mount: { Battery: 3 } })).toBe(
      "Point Defence Battery (Type III)",
    );
    expect(weaponToString({ kind: "PointDefense", mount: { Battery: 1 } })).toBe(
      "Point Defence Battery (Type I)",
    );
  });

  it("gives a battery no action button", () => {
    // It intercepts automatically, so there is nothing for the crew to order.
    expect(
      isActionableWeapon({ kind: "PointDefense", mount: { Battery: 3 } }),
    ).toBe(false);
    // Sandcasters are excluded by kind, not by their display name -- so a
    // weapon merely containing "Sand" in its name keeps its button.
    expect(isActionableWeapon({ kind: "Sand", mount: { Turret: 3 } })).toBe(
      false,
    );
    expect(isActionableWeapon({ kind: "Beam", mount: { Turret: 3 } })).toBe(
      true,
    );
  });

  it("charges a battery one hardpoint", () => {
    expect(mountCost({ Battery: 3 }, "hardpoints")).toBe(1);
  });
});

describe("weapon kind labels", () => {
  it("shows readable names instead of wire identifiers", () => {
    // These travel the wire as Rust enum variant names.
    expect(weaponKindLabel("PointDefense")).toBe("Point Defence");
    expect(weaponKindLabel("MassDriver")).toBe("Mass Driver");
  });

  it("leaves kinds that are already words alone", () => {
    expect(weaponKindLabel("Beam")).toBe("Beam");
    expect(weaponKindLabel("Torpedo")).toBe("Torpedo");
    // Including one this build has never heard of.
    expect(weaponKindLabel("Antimatter")).toBe("Antimatter");
  });

  it("uses the readable name when naming a mounted weapon", () => {
    expect(
      weaponToString({ kind: "MassDriver", mount: { Bay: "Large" } }),
    ).toBe("Large Mass Driver Bay");
  });
});

describe("automatic defences get no fire-control button", () => {
  // Repulsors deflect incoming missiles rather than attacking, so like
  // sandcasters and point-defence batteries there is no target to pick.
  it("treats repulsors as automatic", () => {
    const repulsor: Weapon = { kind: "Repulsor", mount: { Bay: "Small" } };
    expect(isActionableWeapon(repulsor)).toBe(false);
    expect(isPassiveWeapon(repulsor)).toBe(true);
  });

  it("treats sandcasters and point defence batteries as automatic", () => {
    expect(isPassiveWeapon({ kind: "Sand", mount: { Turret: 3 } })).toBe(true);
    expect(
      isPassiveWeapon({ kind: "PointDefense", mount: { Battery: 3 } }),
    ).toBe(true);
  });

  // Everything that actually shoots at a target keeps its button, including the
  // weapon types added most recently.
  it("leaves real weapons actionable", () => {
    const armed: Weapon[] = [
      { kind: "Beam", mount: { Turret: 3 } },
      { kind: "Torpedo", mount: "Barbette" },
      { kind: "Meson", mount: { Bay: "Medium" } },
      { kind: "Ion", mount: "Barbette" },
      { kind: "MassDriver", mount: { Bay: "Large" } },
    ];
    armed.forEach((weapon) => {
      expect(isActionableWeapon(weapon)).toBe(true);
      expect(isPassiveWeapon(weapon)).toBe(false);
    });
  });
});

describe("describing a design's screens", () => {
  it("names screens readably and counts repeats", () => {
    // These travel the wire as Rust enum variant names.
    expect(describeScreens(["Meson", "Meson", "NuclearDamper"])).toEqual([
      "Meson Screen x2",
      "Nuclear Damper",
    ]);
  });

  it("says nothing for a design with no screens", () => {
    expect(describeScreens(undefined)).toEqual([]);
    expect(describeScreens([])).toEqual([]);
  });

  it("passes through a screen type this build does not know", () => {
    expect(describeScreens(["Antimatter"])).toEqual(["Antimatter"]);
  });
});

describe("weapon modifiers", () => {
  const modified = (kind: string, mods: string[]): Weapon => ({
    kind,
    mount: { Turret: 3 },
    modifiers: mods,
  });

  // The hazard: a referee opening a modified design in the editor and saving it
  // must not silently strip the modifiers off its weapons.
  it("survives a group/expand round trip", () => {
    const original: Weapon[] = [
      modified("Pulse", ["LongRange", "HighYield"]),
      modified("Pulse", ["LongRange", "HighYield"]),
      createWeapon("Sand", { Turret: 3 }),
    ];
    const { weapons } = expandGroups(groupWeapons(original));
    expect(weapons).toEqual(original);
  });

  // The MK Mora's case: same kind, same mount, different modifiers. Merging
  // them would give the plain weapon modifications it never had.
  it("does not merge weapons that differ only by modifier", () => {
    const groups = groupWeapons([
      modified("Pulse", ["HighYield"]),
      createWeapon("Pulse", { Turret: 3 }),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups[0].modifiers).toEqual(["HighYield"]);
    expect(groups[1].modifiers).toEqual([]);
  });

  it("still merges weapons that match in every respect", () => {
    const groups = groupWeapons([
      modified("Pulse", ["HighYield"]),
      modified("Pulse", ["HighYield"]),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].count).toBe(2);
  });

  it("names modifiers readably, collapsing repeats as the book writes them", () => {
    expect(describeModifiers(["EnergyEfficient", "EnergyEfficient", "EnergyEfficient"])).toBe(
      "energy efficient x3",
    );
    expect(describeModifiers(["LongRange", "HighYield"])).toBe("long range, high yield");
    expect(describeModifiers([])).toBe("");
    expect(describeModifiers(undefined)).toBe("");
  });

  it("shows modifiers when naming a weapon", () => {
    expect(weaponToString(modified("Pulse", ["LongRange", "HighYield"]))).toBe(
      "Triple Pulse Turret (long range, high yield)",
    );
    // An unmodified weapon reads exactly as before.
    expect(weaponToString(createWeapon("Pulse", { Turret: 3 }))).toBe("Triple Pulse Turret");
  });
});

describe("mixed turrets", () => {
  // The MK Mora's turret: two long-range high-yield pulse lasers and a plain
  // sandcaster, in the wire shape the server sends for a mixed mount.
  const moraTurret: Weapon = {
    mount: { Turret: 3 },
    guns: [
      { kind: "Pulse", modifiers: ["LongRange", "HighYield"] },
      { kind: "Pulse", modifiers: ["LongRange", "HighYield"] },
      { kind: "Sand" },
    ],
  };

  it("reads the guns out of a mixed mount", () => {
    expect(weaponGuns(moraTurret)).toHaveLength(3);
    expect(weaponKinds(moraTurret)).toEqual(["Pulse", "Sand"]);
    expect(countOfKind(moraTurret, "Pulse")).toBe(2);
    expect(countOfKind(moraTurret, "Sand")).toBe(1);
    expect(isUniformWeapon(moraTurret)).toBe(false);
  });

  // A uniform mount still arrives in the old shape, with a kind and a turret
  // size rather than a gun list.
  it("expands a uniform mount from its turret size", () => {
    const triple: Weapon = { kind: "Beam", mount: { Turret: 3 } };
    expect(weaponGuns(triple)).toHaveLength(3);
    expect(weaponKinds(triple)).toEqual(["Beam"]);
    expect(isUniformWeapon(triple)).toBe(true);
    // And a mount that holds one weapon expands to one gun.
    expect(weaponGuns({ kind: "Torpedo", mount: "Barbette" })).toHaveLength(1);
  });

  it("names a mixed mount by its contents", () => {
    expect(weaponToString(moraTurret)).toBe("Triple Turret (Pulse x2, Sand)");
    // A uniform mount is unchanged.
    expect(weaponToString({ kind: "Beam", mount: { Turret: 3 } })).toBe(
      "Triple Beam Turret",
    );
  });

  // A turret of lasers and sand is still orderable -- the lasers can fire even
  // though the sandcaster cannot be ordered.
  it("keeps a mixed turret actionable if any gun in it can be ordered", () => {
    expect(isActionableWeapon(moraTurret)).toBe(true);
    // But a mount whose every gun is automatic is not.
    expect(
      isActionableWeapon({
        mount: { Turret: 2 },
        guns: [{ kind: "Sand" }, { kind: "Sand" }],
      }),
    ).toBe(false);
  });
});

describe("the editor must not destroy a mixed-turret ship", () => {
  // The MK Mora as the server sends it: six turrets of two long-range
  // high-yield pulse lasers plus a sandcaster, and four of two missile racks
  // plus an accurate high-yield beam laser.
  const laserSand = (): Weapon => ({
    mount: { Turret: 3 },
    guns: [
      { kind: "Pulse", modifiers: ["LongRange", "HighYield"] },
      { kind: "Pulse", modifiers: ["LongRange", "HighYield"] },
      { kind: "Sand" },
    ],
  });
  const missileBeam = (): Weapon => ({
    mount: { Turret: 3 },
    guns: [
      { kind: "Missile" },
      { kind: "Missile" },
      { kind: "Beam", modifiers: ["Accurate", "HighYield"] },
    ],
  });
  const mkMora = (): Weapon[] => [
    ...Array.from({ length: 6 }, laserSand),
    ...Array.from({ length: 4 }, missileBeam),
  ];

  // The bug this guards: `kind` and `modifiers` are undefined on a mixed mount
  // because they live on the guns, so keying the editor rows on them collapsed
  // all ten of the MK Mora's turrets into a single group -- and saving wrote
  // them back as ten uniform pulse turrets, destroying the ship's armament.
  it("keeps mixed turrets in separate groups", () => {
    const groups = groupWeapons(mkMora());
    expect(groups).toHaveLength(2);
    expect(groups[0].count).toBe(6);
    expect(groups[1].count).toBe(4);
  });

  it("writes a mixed-turret ship back exactly as it came in", () => {
    const original = mkMora();
    const { weapons } = expandGroups(groupWeapons(original));
    expect(weapons).toEqual(original);
  });

  it("still merges mixed turrets that are genuinely identical", () => {
    const groups = groupWeapons([laserSand(), laserSand()]);
    expect(groups).toHaveLength(1);
    expect(groups[0].count).toBe(2);
  });

  // Two mounts of the same size and the same kinds but different arrangements
  // are different ships and must not merge.
  it("does not merge mounts whose guns differ in proportion", () => {
    const twoLasers = laserSand();
    const oneLaser: Weapon = {
      mount: { Turret: 3 },
      guns: [
        { kind: "Pulse", modifiers: ["LongRange", "HighYield"] },
        { kind: "Sand" },
        { kind: "Sand" },
      ],
    };
    expect(groupWeapons([twoLasers, oneLaser])).toHaveLength(2);
  });

  it("leaves uniform mounts round-tripping as before", () => {
    const uniform: Weapon[] = [
      createWeapon("Beam", { Turret: 3 }),
      createWeapon("Beam", { Turret: 3 }),
      createWeapon("Torpedo", "Barbette"),
    ];
    const { weapons } = expandGroups(groupWeapons(uniform));
    expect(weapons).toEqual(uniform);
  });
});

describe("editing a mixed mount", () => {
  const mixedGroup = (): WeaponGroup => ({
    count: 6,
    mount: { Turret: 3 },
    kind: "Pulse",
    gunnery: 0,
    modifiers: [],
    guns: [{ kind: "Pulse" }, { kind: "Pulse" }, { kind: "Sand" }],
  });

  it("names a mixed group by what is in the mount", () => {
    expect(describeGroupGuns(mixedGroup())).toBe("Pulse x2, Sand");
    // A uniform group is still named by its kind.
    expect(
      describeGroupGuns({
        count: 1,
        mount: { Turret: 3 },
        kind: "Beam",
        gunnery: 0,
        modifiers: [],
      }),
    ).toBe("Beam");
  });

  it("knows how many guns a mount holds", () => {
    expect(gunCapacity({ Turret: 3 })).toBe(3);
    expect(gunCapacity({ Turret: 1 })).toBe(1);
    // Everything that is not a turret holds exactly one.
    expect(gunCapacity("Barbette")).toBe(1);
    expect(gunCapacity({ Bay: "Large" })).toBe(1);
    expect(gunCapacity(null)).toBe(1);
  });

  it("expands a mixed group into the guns it lists", () => {
    const { weapons } = expandGroups([mixedGroup()]);
    expect(weapons).toHaveLength(6);
    // Every mount carries the same three guns, in order.
    weapons.forEach((weapon) => {
      expect(weapon.guns).toEqual([
        { kind: "Pulse" },
        { kind: "Pulse" },
        { kind: "Sand" },
      ]);
    });
  });
});

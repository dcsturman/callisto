import {describe, expect, test} from "vitest";

import {createWeapon, fullSalvo, salvoChoices} from "lib/weapon";

describe("full salvo", () => {
  test("a missile turret throws one per rack", () => {
    expect(fullSalvo(createWeapon("Missile", {Turret: 3}), "Missile")).toBe(3);
  });

  test("every other mount throws a fixed number", () => {
    expect(fullSalvo(createWeapon("Missile", "Barbette"), "Missile")).toBe(5);
    expect(fullSalvo(createWeapon("Missile", {Bay: "Small"}), "Missile")).toBe(12);
    expect(fullSalvo(createWeapon("Missile", {Bay: "Large"}), "Missile")).toBe(120);
    expect(fullSalvo(createWeapon("Torpedo", "Barbette"), "Torpedo")).toBe(1);
    expect(fullSalvo(createWeapon("Torpedo", {Bay: "Medium"}), "Torpedo")).toBe(6);
  });

  test("direct fire has no salvo", () => {
    expect(fullSalvo(createWeapon("Beam", {Turret: 2}), "Beam")).toBeNull();
  });
});

describe("salvo choices", () => {
  test("a small rack offers every number", () => {
    expect(salvoChoices(3)).toEqual([1, 2, 3]);
  });

  test("a large bay steps instead, and still ends at the whole salvo", () => {
    const choices = salvoChoices(120);
    expect(choices[0]).toBe(1);
    expect(choices[choices.length - 1]).toBe(120);
    expect(choices.length).toBeLessThanOrEqual(6);
    expect([...choices].sort((a, b) => a - b)).toEqual(choices);
  });
});

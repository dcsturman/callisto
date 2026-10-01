import {describe, expect, test} from "vitest";

import {PowerBudgetShip, powerDemand, powerLines, weaponMountPower} from "lib/power";
import {createWeapon} from "lib/weapon";

// Executor's shape: 200 tons, Thrust 6, jump 2, Advanced sensors, a particle
// barbette and a mixed missile/sand turret.
const design = {displacement: 200, maneuver: 6, jump: 2};
const weapons = [
  createWeapon("Particle", "Barbette"),
  {mount: {Turret: 3}, guns: [{kind: "Missile"}, {kind: "Missile"}, {kind: "Sand"}]},
];
const ship = (over: Partial<PowerBudgetShip> = {}): PowerBudgetShip => ({
  current_power: 260,
  current_sensors: "Advanced",
  active_weapons: [true, true],
  ...over,
});

const drawOf = (lines: ReturnType<typeof powerLines>, label: string) =>
  lines.find((line) => line.label.startsWith(label))?.draw;

describe("the power budget", () => {
  // The client's figures have to match the server's, since the server decides
  // what the ship can actually do.
  test("matches High Guard's figures", () => {
    const lines = powerLines(ship(), design, weapons);
    expect(drawOf(lines, "Basic")).toBe(40);
    expect(drawOf(lines, "Sensors")).toBe(6);
    expect(drawOf(lines, "M-drive")).toBe(120);
    expect(drawOf(lines, "J-drive")).toBe(40);
    expect(weaponMountPower(weapons[0])).toBe(15);
    expect(weaponMountPower(weapons[1])).toBe(1);
  });

  test("the jump drive is shown but not counted until the ship jumps", () => {
    const lines = powerLines(ship(), design, weapons);
    expect(lines.some((line) => line.onDemand)).toBe(true);
    expect(powerDemand(lines)).toBe(40 + 6 + 120 + 15 + 1);
  });

  test("a system the engineer shut down stops drawing", () => {
    const lines = powerLines(ship({offline: [{Weapon: 0}]}), design, weapons);
    expect(powerDemand(lines)).toBe(40 + 6 + 120 + 1);
    expect(lines.find((line) => line.label.includes("Particle"))?.online).toBe(false);
  });

  test("basic systems can be halved but never switched off", () => {
    const lines = powerLines(ship({basic_power_halved: true}), design, weapons);
    expect(drawOf(lines, "Basic")).toBe(20);
    expect(lines.find((line) => line.system === "Basic")?.switchable).toBe(false);
  });

  test("a weapon knocked out by damage draws nothing", () => {
    const lines = powerLines(ship({active_weapons: [false, true]}), design, weapons);
    expect(lines.find((line) => line.label.includes("Particle"))?.online).toBe(false);
  });
});

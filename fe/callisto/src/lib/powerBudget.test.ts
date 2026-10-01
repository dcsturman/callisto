import {describe, expect, test} from "vitest";

import {
  PowerBudgetShip,
  isPowered,
  powerDemand,
  powerLines,
  powerSpare,
  weaponMountPower,
} from "lib/power";
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

describe("what the plant can actually feed", () => {
  // The order is fixed: life support, sensors, weapons, then the drive, which
  // is the one system a partial share still moves.
  test("a damaged plant starves what it cannot feed and the drive takes the rest", () => {
    const lines = powerLines(ship({current_power: 100}), design, weapons);
    const line = (label: string) => lines.find((l) => l.label.startsWith(label))!;

    expect(line("Basic").received).toBe(40);
    expect(line("Sensors").received).toBe(6);
    expect(line("Particle").received).toBe(15);
    // 100 less life support, sensors, the barbette and the turret's own 1.
    expect(line("M-drive").received).toBe(38);
    expect(isPowered(line("M-drive"))).toBe(true);
  });

  test("a system that cannot have its full draw gets nothing", () => {
    // 44 runs life support and nothing else: the sensors need 6 and there
    // are 4 left.
    const lines = powerLines(ship({current_power: 44}), design, weapons);
    const sensors = lines.find((l) => l.label.startsWith("Sensors"))!;
    expect(sensors.received).toBe(0);
    expect(isPowered(sensors)).toBe(false);
  });

  test("the jump drive is not counted against the running total", () => {
    const lines = powerLines(ship(), design, weapons);
    expect(powerSpare(ship(), lines)).toBe(260 - 182);
    expect(lines.find((l) => l.label.startsWith("J-drive"))!.received).toBe(0);
  });
});

describe("ship features that draw power", () => {
  const withHologram = {
    ...design,
    features: [{name: "Holographic hull", power: 100, default_on: false}],
  };

  test("a system that is switched off draws nothing and sits after the drive", () => {
    const lines = powerLines(ship({offline: [{Feature: 0}]}), withHologram, weapons);
    const hologram = lines.find((l) => l.label === "Holographic hull")!;
    expect(hologram.online).toBe(false);
    expect(hologram.received).toBe(0);
    // The drive is fed first: a luxury loses its share before the ship loses
    // Thrust.
    expect(lines.findIndex((l) => l.system === "Maneuver")).toBeLessThan(
      lines.findIndex((l) => l.label === "Holographic hull")
    );
  });

  test("switched on, it takes what is left and starves if that is not enough", () => {
    const lines = powerLines(ship(), withHologram, weapons);
    const hologram = lines.find((l) => l.label === "Holographic hull")!;
    // 260 less 40 life support, 6 sensors, 16 guns and 120 drive leaves 78.
    expect(hologram.received).toBe(0);
    expect(isPowered(hologram)).toBe(false);
  });
});

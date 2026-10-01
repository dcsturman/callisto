import {describe, expect, test} from "vitest";

import {Ship, Missile} from "lib/entities";
import {emissionLevel, emissionTerms, emissionTotal} from "lib/emissions";
import {incomingSalvoes, teamPicture, watchersOf} from "lib/sensorPicture";

const ship = (over: Partial<Ship> & {name: string}): Ship =>
  ({
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    plan: [[[0, 0, 0], 360], null],
    design: "Gazelle",
    current_hull: 100,
    current_armor: 0,
    current_power: 100,
    current_maneuver: 4,
    current_jump: 2,
    current_fuel: 50,
    current_crew: 10,
    current_sensors: "Military",
    active_weapons: [],
    dodge_thrust: 0,
    assist_gunners: false,
    can_jump: false,
    sensor_locks: [],
    crew: {} as Ship["crew"],
    ...over,
  }) as Ship;

const missile = (over: Partial<Missile> & {name: string}): Missile =>
  ({
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    acceleration: [0, 0, 0],
    source: "Thrasher",
    target: "Executor",
    target_locked: false,
    target_sensor_lock: false,
    target_jump: false,
    target_destroyed: false,
    target_out_of_range: false,
    fuse: 0,
    ...over,
  }) as Missile;

describe("emission profile", () => {
  test("adds up the rows the detection check uses", () => {
    const quiet = ship({name: "Ghost", active_sensors: false, current_power: 0});
    expect(emissionTotal(emissionTerms(quiet, false))).toBe(0);
    expect(emissionLevel(0)).toBe("quiet");

    // Active sensors +2, power plant +1, 2G of thrust +2, firing +2.
    const working = ship({
      name: "Executor",
      plan: [[[2, 0, 0], 360], null],
      crit_level: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
    });
    expect(emissionTotal(emissionTerms(working, true))).toBe(7);
    expect(emissionLevel(7)).toBe("loud");
  });

  test("a lit transponder is the loudest thing a ship can do", () => {
    const squawking = ship({name: "Trader", transmitting: true});
    // Transponder +6, active sensors +2, power plant +1.
    expect(emissionTotal(emissionTerms(squawking, false))).toBe(9);
  });

  test("damage shows up as heat", () => {
    const hurt = ship({name: "Hulk", crit_level: [0, 2, 0, 0, 0, 1, 0, 0, 0, 0, 0]});
    const heat = emissionTerms(hurt, false).find((term) => term.name === "damage heat");
    expect(heat?.value).toBe(3);
  });
});

describe("who is watching us", () => {
  const us = ship({name: "Executor", team: "Blue"});

  test("reports contacts and locks, locks first, and leaves our own side out", () => {
    const watchers = watchersOf(us, [
      us,
      ship({name: "Flayer", team: "Red", contacts: ["Executor"]}),
      ship({name: "Thrasher", team: "Red", contacts: ["Executor"], sensor_locks: ["Executor"]}),
      ship({name: "Mate", team: "Blue", contacts: ["Executor"], sensor_locks: ["Executor"]}),
    ]);
    expect(watchers.map((w) => w.name)).toEqual(["Thrasher", "Flayer"]);
    expect(watchers[0].lock).toBe(true);
    expect(watchers[1].lock).toBe(false);
  });

  test("says nothing when nobody has found us", () => {
    expect(watchersOf(us, [us, ship({name: "Flayer", team: "Red"})])).toEqual([]);
  });
});

describe("squadron picture", () => {
  test("marks which of ours hold each contact, and flags the gaps", () => {
    const us = ship({name: "Executor", team: "Blue", contacts: ["Flayer", "Thrasher"]});
    const mate = ship({name: "Mate", team: "Blue", contacts: ["Flayer"], sensor_locks: ["Flayer"]});
    const picture = teamPicture(us, [us, mate, ship({name: "Flayer", team: "Red"}), ship({name: "Thrasher", team: "Red"})]);

    expect(picture.ours.map((s) => s.name)).toEqual(["Executor", "Mate"]);
    const flayer = picture.rows.find((row) => row.contact === "Flayer");
    const thrasher = picture.rows.find((row) => row.contact === "Thrasher");
    expect(flayer?.gap).toBe(false);
    expect(flayer?.cells.find((c) => c.ship === "Mate")?.lock).toBe(true);
    // Only we hold Thrasher, so the squadron has a hole in its picture.
    expect(thrasher?.gap).toBe(true);
  });

  test("our own ships are not contacts to hand off", () => {
    const us = ship({name: "Executor", team: "Blue", contacts: ["Mate"]});
    const mate = ship({name: "Mate", team: "Blue"});
    expect(teamPicture(us, [us, mate]).rows).toEqual([]);
  });
});

describe("incoming salvoes", () => {
  const us = ship({name: "Executor"});

  test("groups by shooter, nearest first, with a count and an arrival", () => {
    const salvoes = incomingSalvoes(us, [
      missile({name: "m1", source: "Thrasher", position: [4_000_000, 0, 0], velocity: [-1000, 0, 0]}),
      missile({name: "m2", source: "Thrasher", position: [6_000_000, 0, 0], velocity: [-1000, 0, 0]}),
      missile({name: "m3", source: "Flayer", position: [1_000_000, 0, 0], velocity: [-2000, 0, 0]}),
      missile({name: "m4", source: "Flayer", target: "Mate", position: [10, 0, 0]}),
    ]);

    expect(salvoes.map((s) => s.source)).toEqual(["Flayer", "Thrasher"]);
    expect(salvoes[0].count).toBe(1);
    expect(salvoes[1].count).toBe(2);
    // 4,000 km at 1 km/s is 4,000s, which is eleven 360s rounds.
    expect(salvoes[1].roundsOut).toBe(12);
  });

  test("nothing inbound is no salvoes", () => {
    expect(incomingSalvoes(us, [])).toEqual([]);
  });
});

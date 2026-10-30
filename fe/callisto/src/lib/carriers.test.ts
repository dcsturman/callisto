import {describe, expect, it} from "vitest";
import {Ship} from "lib/entities";
import {ShipDesignTemplates} from "lib/shipDesignTemplates";
import {
  aboardSummary,
  berthTakes,
  carriersFor,
  describeFacilities,
  embarkedGroups,
  embarkedSquadrons,
  isCarrierDesign,
  landingCheck,
  landingVelocityLimit,
  needsRearm,
  splitLaunch,
  thrustInG,
} from "lib/carriers";

const templates = {
  "Strike Carrier": {
    name: "Strike Carrier",
    displacement: 75_000,
    hull: 30_000,
    crew: 1_000,
    features: [
      {name: "Docking space (10 t) x80", kind: {DockingSpace: {craft_tons: 10, count: 80}}},
      {name: "Launch tube (100 t)", kind: {LaunchTube: {craft_tons: 100}}, power: 100, default_on: true},
      {name: "Recovery deck (100 t)", kind: {RecoveryDeck: {craft_tons: 100}}, power: 100, default_on: true},
    ],
    weapons: [],
  },
  "Light Fighter": {
    name: "Light Fighter",
    displacement: 10,
    hull: 4,
    crew: 1,
    weapons: [{kind: "Missile", mount: "FixedMount"}],
  },
  Pinnace: {name: "Pinnace", displacement: 40, hull: 16, crew: 2, weapons: []},
} as unknown as ShipDesignTemplates;

const still: Ship["plan"] = [[[0, 0, 0], 0], null];

const ship = (name: string, extra: Partial<Ship> = {}): Ship =>
  ({
    name,
    design: "Light Fighter",
    team: "Blue",
    position: [0, 0, 0],
    velocity: [0, 0, 0],
    plan: still,
    current_hull: 4,
    current_maneuver: 9,
    current_crew: 1,
    crew: {pilot: 2, sensors: [], engineers: [], gunnery: [1], leadership: 0},
    ...extra,
  }) as Ship;

const deck = {berths: 80, berths_free: 60, launch_per_round: 10, recover_per_round: 10, tube_tons: 100, deck_tons: 100};

const carrier = (name: string, embarked: Ship[], extra: Partial<Ship> = {}): Ship =>
  ship(name, {design: "Strike Carrier", current_hull: 30_000, current_crew: 1_000, flight_deck: deck, embarked, ...extra});

describe("a design's facilities", () => {
  it("reads them off the features and writes them the HG way", () => {
    expect(describeFacilities(templates["Strike Carrier"])).toBe(
      "80 docking spaces (10 t), a launch tube (100 t), a recovery deck (100 t)",
    );
    expect(describeFacilities(templates["Light Fighter"])).toBe("");
  });

  it("knows a carrier by her berths, and what they take", () => {
    expect(isCarrierDesign(templates["Strike Carrier"])).toBe(true);
    expect(isCarrierDesign(templates["Light Fighter"])).toBe(false);
    expect(berthTakes(templates["Strike Carrier"], 10)).toBe(true);
    expect(berthTakes(templates["Strike Carrier"], 50)).toBe(false);
  });
});

describe("the craft aboard", () => {
  const wing = (prefix: string, n: number, squadron: string) =>
    Array.from({length: n}, (_, i) => ship(`${prefix} ${String(i + 1).padStart(2, "0")}`, {squadron, rounds_aboard: 3}));

  it("groups them by design and squadron, each sorted by name", () => {
    const sirocco = carrier("Sirocco", [
      ...wing("Gale", 3, "Wind Wing 2"),
      ...wing("Zephyr", 2, "Wind Wing 1"),
      ship("Launch 1", {design: "Pinnace", rounds_aboard: 1}),
    ]);
    const groups = embarkedGroups(sirocco, templates);
    expect(groups.map((g) => [g.design, g.squadron, g.craft.length])).toEqual([
      ["Light Fighter", "Wind Wing 1", 2],
      ["Light Fighter", "Wind Wing 2", 3],
      ["Pinnace", null, 1],
    ]);
    expect(groups[1].craft.map((c) => c.craft.name)).toEqual(["Gale 01", "Gale 02", "Gale 03"]);
  });

  it("says which are launching, rearming or damaged", () => {
    const sirocco = carrier(
      "Sirocco",
      [
        ship("Gale 01", {squadron: "Wind Wing 1", rounds_aboard: 2}),
        ship("Gale 02", {squadron: "Wind Wing 1", rounds_aboard: 0, loaded: [{missiles: 1, torpedoes: 0, sand: 0}]}),
        ship("Gale 03", {squadron: "Wind Wing 1", rounds_aboard: 2, current_hull: 2}),
        ship("Gale 04", {squadron: "Wind Wing 1", rounds_aboard: 2}),
        // Full racks on the round she came in: nothing to rearm.
        ship("Gale 05", {squadron: "Wind Wing 1", rounds_aboard: 0, loaded: [{missiles: 4, torpedoes: 0, sand: 0}]}),
      ],
      {flight_ops: [{craft: "Gale 04", kind: "BerthLaunch", rounds_left: 2}]},
    );
    const states = Object.fromEntries(embarkedGroups(sirocco, templates)[0].craft.map((c) => [c.craft.name, c.states]));
    expect(states).toEqual({
      "Gale 01": [],
      "Gale 02": ["rearming"],
      "Gale 03": ["damaged 2/4"],
      "Gale 04": ["launching (2 rounds)"],
      "Gale 05": [],
    });
    expect(needsRearm(ship("x", {loaded: [{missiles: 0, torpedoes: 0, sand: 0}]}), templates)).toBe(true);
    expect(needsRearm(ship("x"), templates)).toBe(false);
  });

  it("sums them up for a line", () => {
    expect(aboardSummary(carrier("Sirocco", wing("Gale", 20, "Wind Wing 1")))).toBe("20 Light Fighter aboard");
    expect(
      aboardSummary(carrier("Sirocco", [...wing("Gale", 20, "Wind Wing 1"), ship("Launch 1", {design: "Pinnace"})])),
    ).toBe("20 Light Fighter and 1 Pinnace aboard");
    expect(aboardSummary(ship("Gale 01"))).toBeNull();
  });

  it("lists the wings aboard for the roster, under their carrier", () => {
    const ships = [
      carrier("Sirocco", [...wing("Gale", 2, "Wind Wing 1"), ...wing("Zephyr", 2, "Wind Wing 2")]),
      carrier("Mistral", wing("Breeze", 3, "Wind Wing 1")),
      ship("Loose 1"),
    ];
    expect(embarkedSquadrons(ships).map((s) => [s.team, s.name, s.carrier, s.ships.length])).toEqual([
      ["Blue", "Wind Wing 1", "Mistral", 3],
      ["Blue", "Wind Wing 1", "Sirocco", 2],
      ["Blue", "Wind Wing 2", "Sirocco", 2],
    ]);
  });
});

describe("the launch", () => {
  it("sends the tubes' worth now and the rest wait", () => {
    const names = Array.from({length: 12}, (_, i) => `Gale ${i}`);
    expect(splitLaunch(names, 10)).toEqual({now: names.slice(0, 10), waits: names.slice(10)});
  });

  it("goes through the berths all at once with no tube", () => {
    expect(splitLaunch(["a", "b"], 0)).toEqual({now: ["a", "b"], waits: []});
  });
});

describe("the landing", () => {
  const home = () => carrier("Sirocco", []);
  const gale = (extra: Partial<Ship> = {}) =>
    ship("Gale 01", {carrier: "Sirocco", squadron: "Wind Wing 1", position: [100_000, 0, 0], ...extra});

  it("passes every gate for a craft at rest beside her carrier, and shows the check's terms", () => {
    const check = landingCheck(gale(), home(), templates);
    expect(check.gates.every((g) => g.ok)).toBe(true);
    expect(check.ok).toBe(true);
    expect(check.deck).toBe(true);
    // 2D + pilot 2 against 8: 72 in 100.
    expect(check.terms).toEqual([
      ["pilot", 2],
      ["crew", 0],
      ["craft's thrust", 0],
      ["carrier's thrust", 0],
      ["other craft landing", 0],
    ]);
    expect(check.dm).toBe(2);
    expect(check.chance).toBe(72);
  });

  it("counts the Thrust both ships spend, and other craft landing", () => {
    // 1 G for the round against the carrier's 2 G the same way: 535 km
    // apart at the end and 3.5 km/s relative, inside Short and inside a 9 G
    // craft's limit.
    const burning = gale({plan: [[[1, 0, 0], 360], null]});
    const sirocco = carrier("Sirocco", [], {plan: [[[2, 0, 0], 360], null]});
    const check = landingCheck(burning, sirocco, templates, burning.plan, 2);
    expect(check.ok).toBe(true);
    expect(check.terms).toEqual([
      ["pilot", 2],
      ["crew", 0],
      ["craft's thrust", -1],
      ["carrier's thrust", -2],
      ["other craft landing", -2],
    ]);
    expect(check.dm).toBe(-3);
  });

  it("refuses the engine's gates: side, range, velocity, berth, pilot", () => {
    const failing = (craft: Ship, who: Ship) =>
      landingCheck(craft, who, templates)
        .gates.filter((g) => !g.ok)
        .map((g) => g.what);
    expect(failing(gale({team: "Red"}), home())).toEqual(["her side's ship"]);
    expect(failing(gale({position: [2_000_000, 0, 0]}), home())).toEqual([
      "within Short at the end of the round (2,000 km)",
    ]);
    // 9 G for a minute is the limit; 6 km/s is past it. She ends the round
    // 60 km past the carrier, so only the velocity gate fails.
    expect(failing(gale({position: [-2_100_000, 0, 0], velocity: [6_000, 0, 0]}), home())).toEqual([
      `closing at no more than ${(landingVelocityLimit(9) / 1000).toFixed(1)} km/s (6.0 km/s)`,
    ]);
    expect(failing(gale(), carrier("Sirocco", [], {flight_deck: {...deck, berths_free: 0}}))).toEqual([
      "a berth free (0 of 80)",
    ]);
    expect(failing(gale({design: "Pinnace"}), home())).toEqual(["a berth that takes 40 tons"]);
    expect(
      failing(gale({bridge_stations: ["Working", "Working", "Working", "Working", "Working", {Disabled: 1}]}), home()),
    ).toEqual(["pilot's station working"]);
  });

  it("knows a berth docking from a deck landing, and a pinned carrier", () => {
    const noDeck = carrier("Sirocco", [], {flight_deck: {...deck, recover_per_round: 0}, pinned: true, plan: [[[0, 3, 0], 360], null]});
    const check = landingCheck(gale(), noDeck, templates);
    expect(check.deck).toBe(false);
    expect(check.terms).toEqual([]);
    expect(check.chance).toBe(100);
    expect(check.notes).toEqual([
      "Sirocco is held still this round by a berth operation",
      "no recovery deck for her: she docks through a berth, D3 rounds, both ships held still, no check",
    ]);
    // Pinned, so her plan is not flown: no Thrust to count, and she does not move.
    expect(thrustInG(noDeck)).toBe(0);
    expect(check.gates.find((g) => g.what.startsWith("within Short"))?.ok).toBe(true);
  });

  it("offers her own side's carriers, her home first", () => {
    const ships = [
      carrier("Mistral", []),
      carrier("Sirocco", []),
      carrier("Bastion", [], {team: "Red"}),
      ship("Gale 02"),
      gale(),
    ];
    expect(carriersFor(gale(), ships).map((s) => s.name)).toEqual(["Sirocco", "Mistral"]);
  });

  it("floors the Thrust to whole G, as the engine does", () => {
    expect(thrustInG(ship("x", {plan: [[[1.9, 0, 0], 360], [[0, 2.4, 0], 100]]}))).toBe(2);
    expect(thrustInG(ship("x"))).toBe(0);
  });
});

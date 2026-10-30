/**
 * Carriers and the craft aboard them, as the cards read them off the wire
 * (plan §4; FAQ, *Carriers and small craft*).
 *
 * A craft aboard is not an entity: she travels whole inside her carrier's
 * `embarked`, so everything here works from the carrier. The landing gates
 * and the check's terms mirror `flight.rs` so the helm can say what the
 * engine will say before the order is sent.
 */
import {FlightOp, Ship} from "lib/entities";
import {ShipDesignTemplate, ShipDesignTemplates, shipWeapons} from "lib/shipDesignTemplates";
import {MUNITIONS, mountCapacity} from "lib/magazine";
import {FeatureKind} from "lib/power";
import {crewDm} from "lib/crewStrength";
import {chanceOf} from "lib/dice";
import {projectPosition, projectVelocity} from "lib/range";
import {G, RANGE_BANDS, TURN_IN_SECONDS} from "lib/universal";
import {vectorDistance} from "lib/Util";

/** A minute's burn: the relative velocity a craft may come in at, per G. */
const LANDING_SECONDS_PER_G = 60;
/** What a deck or a tube handles a round (HG 2022 FAQ p. 3). */
export const CRAFT_PER_FACILITY_ROUND = 10;
/** The pilot's station, by its place in `bridge_stations`. */
const PILOT_STATION = 5;
/** What the landing check must reach: Routine, as every check here is 8+. */
const CHECK_TARGET = 8;

export type FacilityKind = "DockingSpace" | "FullHangar" | "LaunchTube" | "RecoveryDeck" | "DockingClamp";

/** One of a design's carrier fittings: what it is, how many, how big a craft. */
export interface Facility {
  kind: FacilityKind;
  tons: number;
  count: number;
}

const FACILITY_LABEL: Record<FacilityKind, [string, string]> = {
  DockingSpace: ["docking space", "docking spaces"],
  FullHangar: ["full hangar", "full hangars"],
  LaunchTube: ["launch tube", "launch tubes"],
  RecoveryDeck: ["recovery deck", "recovery decks"],
  DockingClamp: ["docking clamp", "docking clamps"],
};

const facilityOf = (kind: FeatureKind | undefined): Facility | null => {
  if (kind == null || typeof kind === "string") {
    return null;
  }
  const [name, body] = Object.entries(kind)[0] as [FacilityKind, {craft_tons?: number; max_tons?: number; count?: number}];
  if (!(name in FACILITY_LABEL)) {
    return null;
  }
  return {kind: name, tons: body.craft_tons ?? body.max_tons ?? 0, count: body.count ?? 1};
};

/** A design's carrier fittings, in the order the design lists them. */
export const facilities = (design: ShipDesignTemplate | undefined | null): Facility[] =>
  (design?.features ?? []).flatMap((feature) => {
    const facility = facilityOf(feature.kind);
    return facility == null ? [] : [facility];
  });

/** Whether a design has somewhere to keep a craft: a docking space or a hangar. */
export const isCarrierDesign = (design: ShipDesignTemplate | undefined | null): boolean =>
  facilities(design).some((f) => f.kind === "DockingSpace" || f.kind === "FullHangar");

/** Whether any berth of the design takes a craft of `tons`. */
export const berthTakes = (design: ShipDesignTemplate | undefined | null, tons: number): boolean =>
  facilities(design).some((f) => (f.kind === "DockingSpace" || f.kind === "FullHangar") && f.tons >= tons);

/**
 * "80 docking spaces (10 t), a launch tube (100 t), a recovery deck (100 t)",
 * or "" for a design with none.
 */
export const describeFacilities = (design: ShipDesignTemplate | undefined | null): string =>
  facilities(design)
    .map((f) => {
      const [one, many] = FACILITY_LABEL[f.kind];
      const tons = f.tons > 0 ? ` (${f.tons.toLocaleString()} t)` : "";
      return f.count === 1 ? `a ${one}${tons}` : `${f.count} ${many}${tons}`;
    })
    .join(", ");

/** Whether a craft has a mount below its load: what a round aboard will refill. */
export const needsRearm = (craft: Ship, templates: ShipDesignTemplates): boolean => {
  const loaded = craft.loaded;
  if (loaded == null) {
    return false;
  }
  const tons = templates[craft.design]?.displacement ?? 0;
  return shipWeapons(craft, templates).some((weapon, index) => {
    const capacity = mountCapacity(weapon, tons);
    const load = loaded[index];
    return load != null && MUNITIONS.some(({key}) => load[key] < capacity[key]);
  });
};

/** A craft in her berth, and what state she is in. */
export interface CraftAboard {
  craft: Ship;
  /** "launching (2 rounds)", "rearming", "damaged 3/4"; empty when ready. */
  states: string[];
  op?: FlightOp;
}

/** The craft aboard of one design in one squadron. */
export interface EmbarkedGroup {
  design: string;
  squadron: string | null;
  craft: CraftAboard[];
}

const describeOp = (op: FlightOp): string =>
  op.kind === "TubeLaunch"
    ? "launching this round"
    : `${op.kind === "BerthLaunch" ? "launching" : "docking"} (${op.rounds_left} round${op.rounds_left === 1 ? "" : "s"})`;

/** The states a craft aboard is in: launching, rearming, damaged. */
export const craftStates = (craft: Ship, carrier: Ship, templates: ShipDesignTemplates): CraftAboard => {
  const op = (carrier.flight_ops ?? []).find((o) => o.craft === craft.name);
  const states: string[] = [];
  if (op != null) {
    states.push(describeOp(op));
  }
  // Rearmed at the end of her first whole round aboard; one that came in
  // with her racks full has nothing to wait for.
  if ((craft.rounds_aboard ?? 0) === 0 && needsRearm(craft, templates)) {
    states.push("rearming");
  }
  const hull = templates[craft.design]?.hull;
  if (hull != null && craft.current_hull < hull) {
    states.push(`damaged ${craft.current_hull}/${hull}`);
  }
  return {craft, states, op};
};

/** The carrier's craft, grouped by design and then by squadron, each sorted by name. */
export const embarkedGroups = (carrier: Ship, templates: ShipDesignTemplates): EmbarkedGroup[] => {
  const byKey = new Map<string, EmbarkedGroup>();
  for (const craft of carrier.embarked ?? []) {
    const squadron = craft.squadron ?? null;
    const key = `${craft.design}\u0000${squadron ?? ""}`;
    const group = byKey.get(key) ?? {design: craft.design, squadron, craft: []};
    group.craft.push(craftStates(craft, carrier, templates));
    byKey.set(key, group);
  }
  return [...byKey.values()]
    .map((group) => ({...group, craft: group.craft.sort((a, b) => a.craft.name.localeCompare(b.craft.name))}))
    .sort(
      (a, b) =>
        a.design.localeCompare(b.design) ||
        (a.squadron ?? "￿").localeCompare(b.squadron ?? "￿"),
    );
};

/**
 * "80 Light Fighter aboard", "62 Light Fighter and 6 Pinnace aboard", or
 * null when nothing is.
 */
export const aboardSummary = (ship: Ship): string | null => {
  const counts = new Map<string, number>();
  for (const craft of ship.embarked ?? []) {
    counts.set(craft.design, (counts.get(craft.design) ?? 0) + 1);
  }
  if (counts.size === 0) {
    return null;
  }
  const parts = [...counts.entries()]
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .map(([design, count]) => `${count} ${design}`);
  const list = parts.length <= 1 ? parts.join("") : `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
  return `${list} aboard`;
};

/** How many craft are aboard. */
export const aboardCount = (ship: Ship): number => ship.embarked?.length ?? 0;

/**
 * Which of a launch order's craft go this round and which wait: the tubes
 * take `rate` a round and the rest are told to wait for next round's order.
 * A rate of 0 is a berth launch, which takes every craft at once with the
 * carrier pinned until the last is out.
 */
export const splitLaunch = (chosen: string[], rate: number): {now: string[]; waits: string[]} =>
  rate <= 0 ? {now: chosen, waits: []} : {now: chosen.slice(0, rate), waits: chosen.slice(rate)};

/** The Thrust a plan spends, in whole G, as the engine counts it. */
export const thrustInG = (ship: Ship, plan = ship.plan): number => {
  if (ship.pinned) {
    return 0;
  }
  const magnitude = (a: [number, number, number]) => Math.sqrt(a[0] * a[0] + a[1] * a[1] + a[2] * a[2]);
  const loudest = Math.max(magnitude(plan[0][0]), plan[1] == null ? 0 : magnitude(plan[1][0]));
  return Math.max(0, Math.floor(loudest));
};

/** The relative velocity a craft of `thrust` G may come in at, in m/s. */
export const landingVelocityLimit = (thrust: number): number => Math.max(1, thrust) * G * LANDING_SECONDS_PER_G;

/** One thing the engine checks before a landing, and whether it holds. */
export interface Gate {
  ok: boolean;
  what: string;
}

/** The helm's picture of a landing on one carrier. */
export interface LandingCheck {
  carrier: Ship;
  gates: Gate[];
  /** Every gate holds: the engine will try the landing. */
  ok: boolean;
  /** Through a deck, with the Pilot check; false means a berth docking. */
  deck: boolean;
  /** Craft a round the decks take; 0 for a berth docking. */
  recoverPerRound: number;
  /** The check's terms, as the results log itemises them. */
  terms: [string, number][];
  dm: number;
  chance: number;
  /** Something worth knowing that is not a gate. */
  notes: string[];
}

const km = (metres: number): string => Math.round(metres / 1000).toLocaleString("en-US");

/**
 * Whether `craft` can land on `carrier` as the round ends, and her odds on
 * the deck (HG p. 104): 2D + Pilot + crew DM - the craft's G - the
 * carrier's G - other craft landing on the carrier this round not of her
 * squadron, against 8.
 *
 * Positions and velocities are projected to the end of the round, which is
 * when the engine looks; `plan` is the burn the pilot is considering, in
 * place of the one committed. `othersLanding` is how many other craft,
 * not of her squadron, have Land orders on the carrier: the console knows
 * only its own ship's queue, so it is the caller's count.
 */
export const landingCheck = (
  craft: Ship,
  carrier: Ship,
  templates: ShipDesignTemplates,
  plan: Ship["plan"] = craft.plan,
  othersLanding: number = 0,
): LandingCheck => {
  const craftDesign = templates[craft.design];
  const carrierDesign = templates[carrier.design];
  const tons = craftDesign?.displacement ?? 0;
  const deckInfo = carrier.flight_deck;

  const at = (ship: Ship, p: Ship["plan"]) => projectPosition(ship.position, ship.velocity, p, TURN_IN_SECONDS);
  const going = (ship: Ship, p: Ship["plan"]) => projectVelocity(ship.velocity, p, TURN_IN_SECONDS);
  const carrierPlan: Ship["plan"] = carrier.pinned ? [[[0, 0, 0], 0], null] : carrier.plan;
  const craftPlan: Ship["plan"] = craft.pinned ? [[[0, 0, 0], 0], null] : plan;
  const distance = vectorDistance(at(craft, craftPlan), at(carrier, carrierPlan));
  const closing = vectorDistance(going(craft, craftPlan), going(carrier, carrierPlan));
  const limit = landingVelocityLimit(craft.current_maneuver ?? 0);
  const pilotStation = craft.bridge_stations?.[PILOT_STATION];
  const berthFits = berthTakes(carrierDesign, tons);

  const gates: Gate[] = [
    {ok: deckInfo != null, what: `${carrier.name} has berths`},
    {ok: craft.team != null && craft.team === carrier.team, what: "her side's ship"},
    {ok: pilotStation == null || pilotStation === "Working", what: "pilot's station working"},
    {ok: distance <= RANGE_BANDS[0], what: `within Short at the end of the round (${km(distance)} km)`},
    {
      ok: closing <= limit,
      what: `closing at no more than ${(limit / 1000).toFixed(1)} km/s (${(closing / 1000).toFixed(1)} km/s)`,
    },
    {
      ok: (deckInfo?.berths_free ?? 0) > 0 && berthFits,
      what: berthFits
        ? `a berth free (${deckInfo?.berths_free ?? 0} of ${deckInfo?.berths ?? 0})`
        : `a berth that takes ${tons} tons`,
    },
  ];
  const recoverPerRound = tons <= (deckInfo?.deck_tons ?? 0) ? (deckInfo?.recover_per_round ?? 0) : 0;
  const deck = recoverPerRound > 0;

  const craftG = thrustInG(craft, plan);
  const carrierG = thrustInG(carrier);
  // 0 - n rather than -n: a negated zero prints as "-0" in a hover.
  const terms: [string, number][] = deck
    ? [
        ["pilot", craft.crew?.pilot ?? 0],
        ["crew", crewDm(craft, craftDesign).total],
        ["craft's thrust", 0 - craftG],
        ["carrier's thrust", 0 - carrierG],
        ["other craft landing", 0 - othersLanding],
      ]
    : [];
  const dm = terms.reduce((total, [, value]) => total + value, 0);

  const notes: string[] = [];
  if (carrier.pinned) {
    notes.push(`${carrier.name} is held still this round by a berth operation`);
  }
  if (deckInfo != null && !deck) {
    notes.push("no recovery deck for her: she docks through a berth, D3 rounds, both ships held still, no check");
  }
  if (deck) {
    notes.push(`the decks take ${recoverPerRound} a round; the engine turns away the rest`);
  }

  return {
    carrier,
    gates,
    ok: gates.every((gate) => gate.ok),
    deck,
    recoverPerRound,
    terms,
    dm,
    chance: deck ? chanceOf(dm, CHECK_TARGET) : 100,
    notes,
  };
};

/**
 * The carriers `craft` could land on: her own side's ships with a flight
 * deck, her home carrier first.
 */
export const carriersFor = (craft: Ship, ships: Ship[]): Ship[] =>
  ships
    .filter((ship) => ship.name !== craft.name && ship.flight_deck != null && ship.team != null && ship.team === craft.team)
    .sort((a, b) => Number(b.name === craft.carrier) - Number(a.name === craft.carrier) || a.name.localeCompare(b.name));

/** A squadron, or part of one, in a carrier's berths. */
export interface EmbarkedSquadron {
  team: string | null;
  name: string | null;
  carrier: string;
  ships: Ship[];
}

/**
 * Every squadron with craft aboard a carrier, by team, squadron and carrier,
 * for the roster: "Wind Wing 1 (aboard Sirocco)". A wing split between two
 * carriers is two entries.
 */
export const embarkedSquadrons = (ships: Ship[]): EmbarkedSquadron[] => {
  const byKey = new Map<string, EmbarkedSquadron>();
  for (const carrier of ships) {
    for (const craft of carrier.embarked ?? []) {
      const team = craft.team ?? carrier.team ?? null;
      const name = craft.squadron ?? null;
      const key = `${team ?? ""}\u0000${name ?? ""}\u0000${carrier.name}`;
      const entry = byKey.get(key) ?? {team, name, carrier: carrier.name, ships: []};
      entry.ships.push(craft);
      byKey.set(key, entry);
    }
  }
  return [...byKey.values()]
    .map((entry) => ({...entry, ships: entry.ships.sort((a, b) => a.name.localeCompare(b.name))}))
    .sort((a, b) => (a.name ?? "￿").localeCompare(b.name ?? "￿") || a.carrier.localeCompare(b.carrier));
};

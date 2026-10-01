/**
 * Ship software and the Bandwidth it runs in.
 *
 * Mirrors the server's `software.rs`, which is the Core Rulebook p. 161 and
 * High Guard pp. 73-76. The two have to agree: the server decides what a ship
 * can actually do, and a board that disagreed with it would be worse than no
 * board.
 *
 * Note that computers cost no Power -- Bandwidth is their whole budget.
 */

/** A kind of software, spelled as the server serialises it. */
export type SoftwareKind =
  | "Manoeuvre"
  | "Intellect"
  | "Library"
  | "JumpControl"
  | "Evade"
  | "FireControl"
  | "AutoRepair"
  | "AdvancedFireControl"
  | "AntiHijack"
  | "BattleNetwork"
  | "BattleSystem"
  | "BroadSpectrumEw"
  | "ConsciousIntelligence"
  | "ElectronicWarfare"
  | "LaunchSolution"
  | "PointDefence"
  | "ScreenOptimiser"
  | "VirtualCrew"
  | "VirtualGunner";

/** One package: a kind at a level. Level 0 for the unlevelled ones. */
export interface Software {
  kind: SoftwareKind;
  level: number;
}

interface Entry {
  label: string;
  /** What it does, in the words a crew would use. */
  blurb: string;
  /** [level, bandwidth, TL] for each level the package comes in. */
  levels: [number, number, number][];
}

export const SOFTWARE: Record<SoftwareKind, Entry> = {
  Manoeuvre: {
    label: "Manoeuvre",
    blurb: "Basic control of the ship. Free, and always running.",
    levels: [[0, 0, 8]],
  },
  Intellect: {
    label: "Intellect",
    blurb: "Understands spoken orders. Free, and always running.",
    levels: [[0, 0, 11]],
  },
  Library: {
    label: "Library",
    blurb: "Reference data on most subjects. Free, and always running.",
    levels: [[0, 0, 8]],
  },
  JumpControl: {
    label: "Jump Control",
    blurb: "Jumps of up to the listed number. Without it the ship cannot jump at all.",
    levels: [
      [1, 5, 9],
      [2, 10, 11],
      [3, 15, 12],
      [4, 20, 13],
      [5, 25, 14],
      [6, 30, 15],
    ],
  },
  Evade: {
    label: "Evade",
    blurb: "The ship flies itself evasively: a negative DM to every attack made on it.",
    levels: [
      [1, 10, 9],
      [2, 15, 11],
      [3, 25, 13],
    ],
  },
  FireControl: {
    label: "Fire Control",
    blurb: "Automated attacks, a DM to a gunner's attack, or any mix of the two.",
    levels: [
      [1, 5, 9],
      [2, 10, 10],
      [3, 15, 11],
      [4, 20, 12],
      [5, 25, 13],
    ],
  },
  AutoRepair: {
    label: "Auto-Repair",
    blurb: "Repair attempts, or a DM to one. Needs repair drones aboard.",
    levels: [
      [1, 10, 10],
      [2, 20, 12],
    ],
  },
  AdvancedFireControl: {
    label: "Advanced Fire Control",
    blurb: "A DM to every attack the ship makes, with no automated fire.",
    levels: [
      [1, 15, 10],
      [2, 25, 12],
      [3, 30, 14],
    ],
  },
  AntiHijack: {
    label: "Anti-Hijack",
    blurb: "Hinders boarders and hackers. Nothing a space battle touches.",
    levels: [
      [1, 2, 11],
      [2, 10, 12],
      [3, 15, 13],
    ],
  },
  BattleNetwork: {
    label: "Battle Network",
    blurb: "Hands the sensor picture to friendly ships out to Medium (/1) or Long (/2) range.",
    levels: [
      [1, 5, 12],
      [2, 10, 14],
    ],
  },
  BattleSystem: {
    label: "Battle System",
    blurb: "A DM to Tactics (naval) checks, which are not rolled here.",
    levels: [
      [1, 5, 9],
      [2, 10, 12],
      [3, 15, 15],
    ],
  },
  BroadSpectrumEw: {
    label: "Broad Spectrum EW",
    blurb: "A free electronic warfare action against every salvo inside Long range.",
    levels: [[0, 12, 13]],
  },
  ConsciousIntelligence: {
    label: "Conscious Intelligence",
    blurb: "A sentient ship's mind. Flavour, in a gunnery duel.",
    levels: [
      [1, 40, 16],
      [2, 25, 17],
      [3, 10, 18],
    ],
  },
  ElectronicWarfare: {
    label: "Electronic Warfare",
    blurb: "A DM to the sensor operator's electronic warfare checks.",
    levels: [
      [1, 10, 10],
      [2, 15, 13],
      [3, 20, 15],
    ],
  },
  LaunchSolution: {
    label: "Launch Solution",
    blurb: "A DM to every missile and torpedo salvo the ship fires.",
    levels: [
      [1, 5, 8],
      [2, 10, 10],
      [3, 15, 12],
    ],
  },
  PointDefence: {
    label: "Point Defence",
    blurb: "Point-defends another ship within Close (/1) or Short (/2) range.",
    levels: [
      [1, 12, 9],
      [2, 15, 12],
    ],
  },
  ScreenOptimiser: {
    label: "Screen Optimiser",
    blurb: "Angles the screens automatically, at DM+0.",
    levels: [[0, 10, 10]],
  },
  VirtualCrew: {
    label: "Virtual Crew",
    blurb: "Stands in for pilots, gunners and sensor operators at the listed skill.",
    levels: [
      [0, 5, 10],
      [1, 10, 13],
      [2, 15, 15],
    ],
  },
  VirtualGunner: {
    label: "Virtual Gunner",
    blurb: "Stands in for gunners at the listed skill.",
    levels: [
      [0, 5, 9],
      [1, 10, 12],
      [2, 15, 15],
    ],
  },
};

/** Every kind, in the order a console should list them. */
export const SOFTWARE_KINDS = Object.keys(SOFTWARE) as SoftwareKind[];

/** Whether the package carries a level at all, or is just itself. */
export const isLevelled = (kind: SoftwareKind): boolean => {
  const levels = SOFTWARE[kind].levels;
  return levels.length > 1 || levels[0][0] > 0;
};

/** What one package costs to run. */
export const bandwidthOf = (software: Software): number => {
  const levels = SOFTWARE[software.kind]?.levels ?? [];
  const match = levels.find(([level]) => level === software.level) ?? levels[0];
  return match ? match[1] : 0;
};

/** Software that costs nothing to run is always running. */
export const alwaysRunning = (software: Software): boolean => bandwidthOf(software) === 0;

/** "Fire Control/2", or just "Library". */
export const softwareLabel = (software: Software): string =>
  isLevelled(software.kind)
    ? `${SOFTWARE[software.kind].label}/${software.level}`
    : SOFTWARE[software.kind].label;

export const sameSoftware = (a: Software, b: Software): boolean =>
  a.kind === b.kind && a.level === b.level;

/** Bandwidth the running list is using. */
export const bandwidthUsed = (running: Software[]): number =>
  running.reduce((total, software) => total + bandwidthOf(software), 0);

/**
 * Processing available to one package.
 *
 * A /bis computer is worth +5 for Jump Control alone (CRB p. 180): the Type-S
 * scout's Computer/5bis is how it runs Jump Control/2 on a Processing 5
 * machine.
 */
export const processingFor = (processing: number, bis: boolean, kind: SoftwareKind): number =>
  bis && kind === "JumpControl" ? processing + 5 : processing;

/**
 * The computer's capacity right now, after any ion suppression.
 *
 * Mirrors `Ship::processing`: an ion hit spills a tenth of its damage into
 * the computer (a house rule -- see FAQ.md), so anything asking what the ship
 * can run has to subtract it.
 */
export const availableProcessing = (ship: {current_computer: number; ion_bandwidth_loss?: number}): number =>
  Math.max(0, ship.current_computer - (ship.ion_bandwidth_loss ?? 0));

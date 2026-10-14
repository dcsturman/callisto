import * as React from "react";
import {useMemo} from "react";
import {Ship, ShipSystem} from "lib/entities";
import {ShipDesignTemplate} from "lib/shipDesignTemplates";

/**
 * The ship's damage, on a drawing of a ship.
 *
 * A list of damaged systems says what is wrong; it does not say how bad
 * things are. A schematic does: green all over is a whole ship, two red
 * blocks amidships is a ship that is not going anywhere. The engineer reads
 * it without reading it.
 *
 * One drawing serves every design. It is a schematic rather than a portrait --
 * a Harrier and a Vulture are drawn alike -- because the eleven systems are
 * the same eleven whatever the hull, and eighty designs' worth of bespoke art
 * would be eighty more things to keep in step with the rules.
 */

/** Whole, dented, being written off. */
const WHOLE = "#4f9d69";
const HURT = "#d2a33a";
const WRECKED = "#e05a46";

const colourFor = (level: number): string => (level === 0 ? WHOLE : level <= 2 ? HURT : WRECKED);

/**
 * How a hull reads: by its criticals and by what is left of it, whichever is
 * worse.
 *
 * A hull with no critical on it can still be two thirds gone, and a ship that
 * is two thirds gone is not a green ship. Hull points are the clock the fight
 * runs on, so the outline answers to them as well as to the crit table: amber
 * below two thirds, red below a third.
 */
const hullColour = (level: number, percent: number | null): string => {
  const fromCrits = colourFor(level);
  if (percent == null) {
    return fromCrits;
  }
  const fromPoints = percent < 33 ? WRECKED : percent < 66 ? HURT : WHOLE;
  const rank = (colour: string) => (colour === WRECKED ? 2 : colour === HURT ? 1 : 0);
  return rank(fromPoints) > rank(fromCrits) ? fromPoints : fromCrits;
};

type Point = {x: number; y: number};
type Anchor = "start" | "middle" | "end";

/**
 * A system's shapes on the drawing, and the slot its label sits in.
 *
 * Several systems are drawn twice -- two thruster blocks, two bunkers, two
 * weapon pods -- because that is how a ship carries them, and one block
 * labelled "weapon" would read as a single gun. Both halves take their colour
 * from the same crit level: one system, drawn where it lives.
 *
 * Every system is labelled, whole or not: a coloured block with no name
 * beside it is a colour, and the question "what is that amber one?" has to
 * be answerable without hovering. Labels live in fixed slots down either
 * margin rather than on top of what they name -- the hull is 200 units wide
 * and eleven names will not fit inside it -- and a leader line joins each to
 * its shape. What is whole is drawn quieter than what is not, so the damage
 * still comes forward.
 */
type Section = {
  system: ShipSystem;
  label: string;
  shapes: Point[][];
  labelAt: {x: number; y: number; anchor: Anchor};
};

/** A band across the hull, from its top edge to its bottom: the hull tapers,
 *  so a rectangle would sit inside it wrong. */
const band = (
  y1: number,
  left1: number,
  right1: number,
  y2: number,
  left2: number,
  right2: number
): Point[] => [
  {x: left1, y: y1},
  {x: right1, y: y1},
  {x: right2, y: y2},
  {x: left2, y: y2},
];

/** A plain block, for what hangs off the hull rather than sitting in it. */
const block = (x: number, y: number, w: number, h: number): Point[] => [
  {x, y},
  {x: x + w, y},
  {x: x + w, y: y + h},
  {x, y: y + h},
];

const SECTIONS: Section[] = [
  {
    system: ShipSystem.Sensors,
    label: "Sensors",
    shapes: [band(50, 94, 106, 74, 86, 114)],
    labelAt: {x: 238, y: 54, anchor: "end"},
  },
  {
    system: ShipSystem.Bridge,
    label: "Bridge",
    shapes: [band(78, 84.5, 115.5, 106, 75, 125)],
    labelAt: {x: 238, y: 90, anchor: "end"},
  },
  {
    system: ShipSystem.Crew,
    label: "Crew",
    shapes: [band(110, 73.7, 126.3, 140, 63.6, 136.4)],
    labelAt: {x: 238, y: 126, anchor: "end"},
  },
  {
    system: ShipSystem.Cargo,
    label: "Cargo",
    shapes: [band(144, 62, 137.8, 160, 56.8, 143.2)],
    labelAt: {x: -38, y: 120, anchor: "start"},
  },
  {
    system: ShipSystem.Weapon,
    label: "Weapon",
    shapes: [block(20, 148, 13, 40), block(167, 148, 13, 40)],
    labelAt: {x: -38, y: 168, anchor: "start"},
  },
  {
    system: ShipSystem.Fuel,
    label: "Fuel",
    shapes: [band(164, 55, 75.25, 188, 47, 71.25), band(164, 124.75, 145, 188, 128.75, 153)],
    labelAt: {x: -38, y: 216, anchor: "start"},
  },
  {
    system: ShipSystem.Jump,
    label: "J-drive",
    shapes: [band(164, 78.25, 98.5, 188, 74.25, 98.5)],
    labelAt: {x: 58, y: 242, anchor: "middle"},
  },
  {
    system: ShipSystem.Powerplant,
    label: "Power",
    shapes: [band(164, 101.5, 121.75, 188, 101.5, 125.75)],
    labelAt: {x: 142, y: 242, anchor: "middle"},
  },
  {
    system: ShipSystem.Maneuver,
    label: "M-drive",
    shapes: [block(58, 208, 32, 13), block(110, 208, 32, 13)],
    labelAt: {x: 238, y: 220, anchor: "end"},
  },
];

/** The outer line: armour is what the ship is wrapped in. */
const ARMOUR_PATH = "M100,26 C114,54 136,124 174,196 L166,208 L34,208 L26,196 C64,124 86,54 100,26 Z";
/** The line within it: the hull proper. */
const HULL_PATH = "M92,44 L108,44 L158,192 L42,192 Z";

const centreOf = (shape: Point[]): Point => ({
  x: shape.reduce((total, point) => total + point.x, 0) / shape.length,
  y: shape.reduce((total, point) => total + point.y, 0) / shape.length,
});

const pointsOf = (shape: Point[]): string => shape.map((point) => `${point.x},${point.y}`).join(" ");

/** A system's name over its severity, joined to its shape by a leader. */
function SectionLabel(args: {
  label: string;
  at: Section["labelAt"];
  colour: string;
  reading: string;
  leaderTo?: Point;
  /** A system with nothing wrong steps back; the damage is what should read. */
  quiet?: boolean;
}) {
  // The leader leaves from the label's inner edge, so it never crosses its
  // own text.
  const from = {
    x: args.at.x + (args.at.anchor === "start" ? 6 : args.at.anchor === "end" ? -6 : 0),
    y: args.at.y - 3,
  };
  return (
    <g fill={args.colour} textAnchor={args.at.anchor} opacity={args.quiet === true ? 0.6 : 1}>
      {args.leaderTo != null && (
        <line
          x1={from.x}
          y1={from.y}
          x2={args.leaderTo.x}
          y2={args.leaderTo.y}
          stroke={args.colour}
          strokeWidth={0.6}
          strokeOpacity={args.quiet === true ? 0.3 : 0.5}
        />
      )}
      <text className="damage-diagram-name" x={args.at.x} y={args.at.y}>
        {args.label}
      </text>
      <text className="damage-diagram-reading" x={args.at.x} y={args.at.y + 10}>
        {args.reading}
      </text>
    </g>
  );
}

export function DamageDiagram(args: {ship: Ship; design: ShipDesignTemplate | undefined}) {
  const levels = args.ship.crit_level ?? [];
  const levelOf = (system: ShipSystem): number => levels[system] ?? 0;

  // Hull is the one system with a reading as well as a severity: a ship can
  // be uncritted and still be most of the way to breaking up.
  const hullPercent = useMemo(() => {
    if (args.design == null || args.design.hull === 0) {
      return null;
    }
    return Math.round((args.ship.current_hull / args.design.hull) * 100);
  }, [args.ship.current_hull, args.design]);

  const armourLevel = levelOf(ShipSystem.Armor);
  const hullLevel = levelOf(ShipSystem.Hull);
  const hullShows = hullColour(hullLevel, hullPercent);

  return (
    <svg
      className="damage-diagram"
      viewBox="-40 0 280 258"
      role="img"
      aria-label="Ship damage by system">
      {/* Armour outside, hull within it, each taking its own colour. Drawn
          first, so the blocks sit on top of them.
          The hull is the heavy line and the armour the fine one, which is the
          other way round from how they sit on the ship: most fights end when
          the hull reaches nothing, so the hull is what the drawing should say
          loudest. */}
      <path d={ARMOUR_PATH} fill="none" stroke={colourFor(armourLevel)} strokeWidth={1.6} />
      <path d={HULL_PATH} fill="#070b10" stroke={hullShows} strokeWidth={3.5} />

      {SECTIONS.map((section) => {
        const level = levelOf(section.system);
        const colour = colourFor(level);
        // A system drawn in two places points at whichever half is nearer its
        // label, rather than at the empty middle between them.
        const anchor = {
          x: section.labelAt.x,
          y: section.labelAt.y,
        };
        const nearest = section.shapes
          .map(centreOf)
          .reduce((best, point) =>
            Math.hypot(point.x - anchor.x, point.y - anchor.y) <
            Math.hypot(best.x - anchor.x, best.y - anchor.y)
              ? point
              : best
          );
        return (
          <g key={section.system}>
            {section.shapes.map((shape, index) => (
              <polygon
                key={index}
                points={pointsOf(shape)}
                fill={colour}
                fillOpacity={level === 0 ? 0.2 : 0.5}
                stroke={colour}
                strokeWidth={1}>
                <title>
                  {`${section.label}: ${level === 0 ? "no damage" : `severity ${level}`}`}
                </title>
              </polygon>
            ))}
            <SectionLabel
              label={section.label}
              at={section.labelAt}
              colour={colour}
              reading={String(level)}
              leaderTo={nearest}
              quiet={level === 0}
            />
          </g>
        );
      })}

      <SectionLabel
        label="Armour"
        at={{x: 238, y: 14, anchor: "end"}}
        colour={colourFor(armourLevel)}
        reading={String(armourLevel)}
        quiet={armourLevel === 0}
      />

      {/* The hull reads whether or not it is critted: its points are the clock
          the whole fight runs on. */}
      <SectionLabel
        label="Hull"
        at={{x: -38, y: 14, anchor: "start"}}
        colour={hullShows}
        quiet={hullShows === WHOLE}
        reading={hullPercent == null ? String(hullLevel) : `${hullLevel} · ${hullPercent}%`}
      />
    </svg>
  );
}

export default DamageDiagram;

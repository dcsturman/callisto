import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {
  EMISSION_LEVEL_LABELS,
  EMISSION_SCALE_MAX,
  EmissionLevel,
  EmissionTerm,
  emissionLevel,
  emissionTerms,
  emissionTotal,
} from "lib/emissions";
import {useAppSelector} from "state/hooks";

/**
 * How loud this ship is, and why.
 *
 * The sensop's own instrument: every row here is a DM an enemy adds to find
 * us, so the panel answers "are we being obvious, and what would quiet us
 * down". The scale is the gauge; the rows beneath it are the itemised reading,
 * the same terms the detection check reports after the fact.
 */
export function EmissionProfile(args: {ship: Ship}) {
  // What this ship has queued to fire, since that is what it will give away
  // when the round resolves.
  const firing = useAppSelector(
    (state) => (state.actions[args.ship.name]?.fire.length ?? 0) > 0
  );

  const terms = useMemo(
    () => emissionTerms(args.ship, firing),
    [args.ship, firing]
  );
  const total = emissionTotal(terms);
  const level = emissionLevel(total);

  return (
    <div className="emission-profile">
      <div className="section-tag">Emissions</div>
      <EmissionGauge total={total} level={level} />
      <ul className="emission-terms">
        {terms.map((term) => (
          <EmissionRow key={term.name} term={term} />
        ))}
      </ul>
    </div>
  );
}

/**
 * The gauge: four bands of increasing loudness with a needle on the reading.
 *
 * Drawn rather than written because the question it answers -- how exposed are
 * we -- is a matter of degree, and a bar answers that before the eye has read
 * a number.
 */
function EmissionGauge(args: {total: number; level: EmissionLevel}) {
  const width = 240;
  const height = 34;
  const scale = (value: number) => (Math.min(value, EMISSION_SCALE_MAX) / EMISSION_SCALE_MAX) * width;
  // Upper edge of each step, matching `emissionLevel`.
  const steps: {level: EmissionLevel; upTo: number}[] = [
    {level: "quiet", upTo: 2},
    {level: "faint", upTo: 5},
    {level: "loud", upTo: 9},
    {level: "blazing", upTo: EMISSION_SCALE_MAX},
  ];
  const needle = scale(args.total);

  return (
    <div className={`emission-gauge emission-${args.level}`}>
      <svg
        viewBox={`0 0 ${width} ${height}`}
        className="emission-gauge-svg"
        role="img"
        aria-label={`Emissions ${args.total}: ${EMISSION_LEVEL_LABELS[args.level]}`}>
        {steps.map((step, index) => {
          const from = index === 0 ? 0 : scale(steps[index - 1].upTo);
          return (
            <rect
              key={step.level}
              x={from}
              y={8}
              width={scale(step.upTo) - from}
              height={12}
              className={`emission-step emission-step-${step.level}`}
            />
          );
        })}
        {/* Ticks at each step edge: the scale is a rules scale, so it should
            be readable as numbers and not only as colour. */}
        {steps.slice(0, -1).map((step) => (
          <line
            key={step.upTo}
            x1={scale(step.upTo)}
            x2={scale(step.upTo)}
            y1={6}
            y2={22}
            className="emission-tick"
          />
        ))}
        <polygon
          points={`${needle - 5},0 ${needle + 5},0 ${needle},8`}
          className="emission-needle"
        />
        <line x1={needle} x2={needle} y1={6} y2={22} className="emission-needle-line" />
      </svg>
      <div className="emission-reading">
        <span className="emission-total">DM+{args.total}</span>
        <span className="emission-level">{EMISSION_LEVEL_LABELS[args.level]}</span>
      </div>
    </div>
  );
}

/** One row of the reading: what it is, what it costs, and how to quiet it. */
function EmissionRow(args: {term: EmissionTerm}) {
  const {name, value, remedy} = args.term;
  const title =
    value === 0
      ? `${name}: nothing to see`
      : remedy == null
        ? `${name}: DM+${value}, and nothing to be done about it`
        : `${name}: DM+${value} -- ${remedy}`;
  return (
    <li className={value > 0 ? "emission-term" : "emission-term emission-term-quiet"} title={title}>
      <EmissionIcon name={name} lit={value > 0} />
      <span className="emission-term-name">{name}</span>
      <span className="emission-term-value">{value > 0 ? `+${value}` : "—"}</span>
    </li>
  );
}

/**
 * A glyph per row, so the profile reads at a glance: a dish for the sensors
 * being radiated, a plume for the drive, and so on. Drawn inline and in one
 * colour, inheriting from the row, so a lit row and a quiet one differ only in
 * the way everything else on the row does.
 */
function EmissionIcon(args: {name: string; lit: boolean}) {
  const paths: {[name: string]: React.JSX.Element} = {
    "active sensors": (
      <>
        <path d="M2 12 A6 6 0 0 1 14 12 Z" />
        <line x1="8" y1="12" x2="8" y2="15" />
        <path d="M11 4 A5 5 0 0 1 13.5 6" className="emission-icon-wave" />
      </>
    ),
    thrust: (
      <>
        <path d="M6 3 L10 3 L10 9 L6 9 Z" />
        <path d="M6 9 L8 15 L10 9 Z" className="emission-icon-wave" />
      </>
    ),
    "power plant": (
      <>
        <circle cx="8" cy="9" r="5" />
        <circle cx="8" cy="9" r="1.5" className="emission-icon-wave" />
      </>
    ),
    firing: (
      <>
        <line x1="2" y1="9" x2="11" y2="9" />
        <path d="M11 5 L15 9 L11 13 Z" />
      </>
    ),
    "damage heat": (
      <>
        <path d="M8 2 C10 6 12 7 12 10 A4 4 0 0 1 4 10 C4 7 6 6 8 2 Z" />
      </>
    ),
    transmitting: (
      <>
        <line x1="8" y1="4" x2="8" y2="15" />
        <path d="M4 7 A6 6 0 0 1 12 7" className="emission-icon-wave" />
        <path d="M2 4 A10 10 0 0 1 14 4" className="emission-icon-wave" />
      </>
    ),
  };
  return (
    <svg
      viewBox="0 0 16 16"
      className={args.lit ? "emission-icon emission-icon-lit" : "emission-icon"}
      aria-hidden="true">
      {paths[args.name]}
    </svg>
  );
}

export default EmissionProfile;

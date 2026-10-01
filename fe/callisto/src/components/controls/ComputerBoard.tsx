import * as React from "react";
import {useMemo} from "react";

import {Ship} from "lib/entities";
import {
  SOFTWARE,
  Software,
  alwaysRunning,
  bandwidthOf,
  bandwidthUsed,
  processingFor,
  sameSoftware,
  softwareLabel,
} from "lib/software";
import {setSoftwareRunning} from "lib/serverManager";
import {useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";

/**
 * The ship's computer: what it carries, what it is running, and how much
 * Bandwidth is left.
 *
 * A ship owns more software than it can run at once -- that is the whole of
 * the mechanic -- so the question this board answers is which of it to have
 * up when the shooting starts. Free packages cannot be stopped, since there
 * is no Bandwidth to free by stopping them, and anything that will not fit is
 * shown as unavailable rather than hidden.
 */
export function ComputerBoard(args: {ship: Ship}) {
  const templates = useAppSelector(templatesSelector);
  const design = templates[args.ship.design];

  const installed = useMemo(
    () => args.ship.software ?? design?.software ?? [],
    [args.ship.software, design]
  );
  const running = useMemo(() => args.ship.software_running ?? [], [args.ship.software_running]);

  if (design == null) {
    return null;
  }

  const processing = args.ship.current_computer;
  const bis = design.computer_bis === true;
  const used = bandwidthUsed(running);
  const isRunning = (software: Software) => running.some((on) => sameSoftware(on, software));

  return (
    <div className="computer-board">
      <div className="computer-summary">
        <span className="computer-processing">
          Computer/{processing}
          {bis ? "bis" : ""}
          {design.computer_fib ? "fib" : ""}
        </span>
        <span className={used > processing ? "computer-spare computer-over" : "computer-spare"}>
          {used} of {processing} bandwidth
        </span>
      </div>
      {/* One bar rather than a column per package: Bandwidth is a single pool,
          and what the engineer wants at a glance is how much of it is left. */}
      <div className="bandwidth-gauge" title={`${processing - used} Bandwidth free`}>
        <div
          className="bandwidth-gauge-fill"
          style={{width: `${Math.min(100, processing === 0 ? 0 : (used / processing) * 100)}%`}}
        />
      </div>
      <ul className="software-list">
        {installed.map((software) => {
          const on = isRunning(software);
          const fixed = alwaysRunning(software);
          const capacity = processingFor(processing, bis, software.kind);
          // Room for it, counting what stopping it would give back.
          const fits = used - (on ? bandwidthOf(software) : 0) + bandwidthOf(software) <= capacity;
          const entry = SOFTWARE[software.kind];
          return (
            <li
              key={`${software.kind}-${software.level}`}
              className={on ? "software-row software-on" : "software-row"}>
              <label
                className="software-toggle"
                title={
                  fixed
                    ? `${entry.blurb} Costs no Bandwidth, so it is always running.`
                    : !fits && !on
                      ? `${entry.blurb} Needs ${bandwidthOf(software)} Bandwidth; ${processing - used} free.`
                      : `${entry.blurb} ${bandwidthOf(software)} Bandwidth.`
                }>
                <input
                  type="checkbox"
                  checked={on || fixed}
                  disabled={fixed || (!on && !fits)}
                  onChange={(event) =>
                    setSoftwareRunning(args.ship.name, software, event.target.checked)
                  }
                />
                <span className="software-name">{softwareLabel(software)}</span>
              </label>
              <span className="software-bandwidth">{fixed ? "—" : bandwidthOf(software)}</span>
            </li>
          );
        })}
        {installed.length === 0 && <li className="software-row software-none">no software aboard</li>}
      </ul>
    </div>
  );
}

export default ComputerBoard;

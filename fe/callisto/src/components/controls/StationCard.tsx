import * as React from "react";
import {useState} from "react";

/**
 * The shell every crew station is drawn in.
 *
 * One shape for all of them: the station's name, who is working it, the
 * instruments and orders inside, and a fold for when it is in the way. Before
 * this, each station had grown its own frame -- the gunner an accordion in
 * one column, the pilot a bare heading in another -- so where a panel sat and
 * what it looked like depended on which role you held rather than on what it
 * was.
 */
export function StationCard(args: {
  /** The station: "Pilot", "Gunner", and so on. */
  title: string;
  /** A glyph for it, where one is unmistakable. */
  icon?: React.ReactNode;
  /** Who is working it, when the ship carries more than one of them. */
  crew?: string;
  /** Folded away to its heading. Stations start open. */
  initialOpen?: boolean;
  /**
   * Move this card one place earlier or later in the console.
   *
   * Absent when there is nowhere to move it -- the first card cannot go up
   * -- so the arrow is simply not offered rather than offered and refused.
   */
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  children: React.ReactNode;
}) {
  const [open, setOpen] = useState(args.initialOpen ?? true);

  return (
    <section className={open ? "station-card" : "station-card station-card-closed"}>
      {/* A row rather than one button, because the arrows are buttons too
          and a button cannot live inside a button. */}
      <div className="station-card-head-row">
        <button
          type="button"
          className="station-card-head"
          aria-expanded={open}
          onClick={() => setOpen(!open)}>
          {args.icon != null && (
            <span className="station-card-icon" aria-hidden="true">
              {args.icon}
            </span>
          )}
          <span className="station-card-title">{args.title}</span>
          {args.crew != null && <span className="station-card-crew">{args.crew}</span>}
          <span className="station-card-chevron" aria-hidden="true">
            {open ? "⌃" : "⌄"}
          </span>
        </button>
        {/* Which instruments sit at the top of the column is a matter of
            taste and of what this player is doing, so they can arrange it. */}
        {(args.onMoveUp != null || args.onMoveDown != null) && (
          <span className="station-card-move">
            <button
              type="button"
              className="station-card-move-button"
              title={`Move ${args.title} up`}
              aria-label={`Move ${args.title} up`}
              disabled={args.onMoveUp == null}
              onClick={args.onMoveUp}>
              ▲
            </button>
            <button
              type="button"
              className="station-card-move-button"
              title={`Move ${args.title} down`}
              aria-label={`Move ${args.title} down`}
              disabled={args.onMoveDown == null}
              onClick={args.onMoveDown}>
              ▼
            </button>
          </span>
        )}
      </div>
      {open && <div className="station-card-body">{args.children}</div>}
    </section>
  );
}

/**
 * One station's queued orders, shown inside its own card.
 *
 * The captain is the exception and has them all together in theirs: they are
 * the one person whose job is the whole round rather than one station, and
 * hunting five cards for the actions to inspire was unworkable.
 */
export function StationOrders(args: {children: React.ReactNode}) {
  return <div className="station-orders">{args.children}</div>;
}

export default StationCard;

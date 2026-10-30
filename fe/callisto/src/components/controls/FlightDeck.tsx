import * as React from "react";
import {useEffect, useMemo, useState} from "react";

import {Ship} from "lib/entities";
import {
  EmbarkedGroup,
  aboardCount,
  describeFacilities,
  embarkedGroups,
  splitLaunch,
} from "lib/carriers";
import {ViewMode, hasRole} from "lib/view";
import {useAppDispatch, useAppSelector} from "state/hooks";
import {templatesSelector} from "state/serverSlice";
import {setLaunch} from "state/actionsSlice";

/** "Light Fighter · Wind Wing 1", or just the design for craft in no squadron. */
const groupLabel = (group: EmbarkedGroup): string =>
  group.squadron == null ? group.design : `${group.design} · ${group.squadron}`;

/** "3 rearming, 1 damaged, 2 launching": how many of a group are not simply ready. */
const groupStates = (group: EmbarkedGroup): string => {
  const counts = new Map<string, number>();
  for (const {states} of group.craft) {
    for (const state of states) {
      const head = state.split(" ")[0];
      counts.set(head, (counts.get(head) ?? 0) + 1);
    }
  }
  return [...counts.entries()].map(([state, n]) => `${n} ${state}`).join(", ");
};

/**
 * The carrier's flight deck: who is in her berths, in what state, what the
 * deck can do this round, and the captain's Launch order (plan §4; FAQ,
 * *Carriers and small craft*).
 *
 * Craft aboard are not on the plot, so this is the only place they are
 * seen. Flight operations are the Captain's (IN p. 97): the card is theirs,
 * and the referee's and a fleet commander's when they open the ship.
 */
export function FlightDeck(args: {ship: Ship}) {
  const templates = useAppSelector(templatesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const queued = useAppSelector((state) => state.actions[args.ship.name]?.launch ?? null);
  const dispatch = useAppDispatch();
  const deck = args.ship.flight_deck;
  const design = templates[args.ship.design];

  const groups = useMemo(() => embarkedGroups(args.ship, templates), [args.ship, templates]);
  const ops = args.ship.flight_ops ?? [];
  // A craft already on her way out or in is not there to be picked.
  const inOps = useMemo(() => new Set(ops.map((op) => op.craft)), [ops]);
  const pickable = useMemo(
    () => groups.flatMap((group) => group.craft.filter((c) => !inOps.has(c.craft.name)).map((c) => c.craft.name)),
    [groups, inOps],
  );

  // What the captain has picked but not yet ordered. Starts from the queued
  // order, so a console opened mid-round shows what stands.
  const [picked, setPicked] = useState<string[]>(queued ?? []);
  // Keyed on the names rather than the list: every server snapshot is a new
  // list, and a half-made pick must survive a shipmate's order going up.
  const queuedKey = (queued ?? []).join("\u0000");
  useEffect(() => setPicked(queuedKey === "" ? [] : queuedKey.split("\u0000")), [queuedKey]);
  const [open, setOpen] = useState<string[]>([]);
  const [count, setCount] = useState<string>("");

  const canOrder = hasRole(roles, ViewMode.Captain);
  const rate = deck?.launch_per_round ?? 0;
  const split = splitLaunch(picked, rate);
  const sameAsQueued =
    queued != null && queued.length === picked.length && queued.every((name, i) => picked[i] === name);

  const pick = (names: string[]) => setPicked((was) => [...was, ...names.filter((n) => !was.includes(n))]);
  const unpick = (names: string[]) => setPicked((was) => was.filter((n) => !names.includes(n)));
  const fold = (key: string) => setOpen((was) => (was.includes(key) ? was.filter((k) => k !== key) : [...was, key]));

  if (deck == null) {
    return null;
  }

  const aboard = aboardCount(args.ship);
  const pinned = args.ship.pinned === true;

  return (
    <div className="flight-deck" data-tutorial="flight-deck">
      <p className="flight-deck-facilities" title={describeFacilities(design) || undefined}>
        Berths {deck.berths - deck.berths_free} of {deck.berths} used, {deck.berths_free} free.{" "}
        {rate > 0
          ? `Tubes launch ${rate} a round (craft to ${deck.tube_tons.toLocaleString()} t).`
          : "No working launch tube: craft go out through their berths, D3 rounds each, the carrier held still."}{" "}
        {deck.recover_per_round > 0
          ? `Decks recover ${deck.recover_per_round} a round (to ${deck.deck_tons.toLocaleString()} t).`
          : "No working recovery deck: craft dock through a berth, D3 rounds, both held still."}
      </p>
      {pinned && (
        <p className="flight-deck-pinned">
          Held still this round by a berth operation: no Thrust, no dodge, no attacks (HG p. 61).
        </p>
      )}
      {ops.length > 0 && (
        <ul className="flight-deck-ops">
          {ops.map((op) => (
            <li key={op.craft}>
              {op.craft}:{" "}
              {op.kind === "TubeLaunch"
                ? "launching through the tubes this round"
                : `${op.kind === "BerthLaunch" ? "leaving her berth" : "docking"}, ${op.rounds_left} round${op.rounds_left === 1 ? "" : "s"} to go`}
            </li>
          ))}
        </ul>
      )}
      {aboard === 0 ? (
        <p className="flight-deck-empty">Nothing aboard.</p>
      ) : (
        <table className="flight-deck-table">
          <tbody>
            {groups.map((group) => {
              const key = `${group.design}\u0000${group.squadron ?? ""}`;
              const isOpen = open.includes(key);
              const names = group.craft.filter((c) => !inOps.has(c.craft.name)).map((c) => c.craft.name);
              const pickedHere = names.filter((n) => picked.includes(n)).length;
              const states = groupStates(group);
              return (
                <React.Fragment key={key}>
                  <tr className="flight-deck-group">
                    <td className="flight-deck-name">
                      <button
                        type="button"
                        className="roster-fold"
                        aria-expanded={isOpen}
                        aria-label={`${isOpen ? "Fold" : "Show"} the craft of ${groupLabel(group)}`}
                        onClick={() => fold(key)}>
                        {isOpen ? "▾" : "▸"}
                      </button>{" "}
                      {groupLabel(group)}
                    </td>
                    <td className="flight-deck-count" title={states || "all ready"}>
                      {group.craft.length}
                      {states !== "" && <span className="flight-deck-states"> ({states})</span>}
                    </td>
                    <td className="flight-deck-actions">
                      {canOrder && names.length > 0 && (
                        pickedHere === names.length ? (
                          <button type="button" className="squadrons-link" onClick={() => unpick(names)}>
                            Unpick
                          </button>
                        ) : (
                          <button
                            type="button"
                            className="squadrons-link"
                            title={`Pick every craft of ${groupLabel(group)} for launch`}
                            onClick={() => pick(names)}>
                            Pick {group.squadron == null ? "all" : "wing"}
                          </button>
                        )
                      )}
                    </td>
                  </tr>
                  {isOpen &&
                    group.craft.map(({craft, states: own}) => (
                      <tr key={craft.name} className="flight-deck-craft">
                        <td className="flight-deck-name">
                          {canOrder && !inOps.has(craft.name) ? (
                            <label>
                              <input
                                type="checkbox"
                                aria-label={`Pick ${craft.name}`}
                                checked={picked.includes(craft.name)}
                                onChange={(e) => (e.target.checked ? pick([craft.name]) : unpick([craft.name]))}
                              />
                              {craft.name}
                            </label>
                          ) : (
                            craft.name
                          )}
                        </td>
                        <td className="flight-deck-count" colSpan={2}>
                          {own.length === 0 ? "ready" : own.join(", ")}
                        </td>
                      </tr>
                    ))}
                </React.Fragment>
              );
            })}
          </tbody>
        </table>
      )}
      {canOrder && aboard > 0 && (
        <div className="flight-deck-launch">
          <div className="flight-deck-pick-row">
            <input
              type="number"
              className="control-input flight-deck-count-input"
              min={1}
              max={pickable.length}
              placeholder="N"
              aria-label="How many to pick"
              value={count}
              onChange={(e) => setCount(e.target.value)}
            />
            <button
              type="button"
              className="squadrons-link"
              disabled={pickable.length === 0}
              title="Pick this many of the craft not yet picked, in the order listed"
              onClick={() => {
                const n = Math.max(0, Math.floor(Number(count) || 0));
                pick(pickable.filter((name) => !picked.includes(name)).slice(0, n));
              }}>
              Pick N
            </button>
            <button type="button" className="squadrons-link" disabled={pickable.length === 0} onClick={() => pick(pickable)}>
              Pick all
            </button>
            <button type="button" className="squadrons-link" disabled={picked.length === 0} onClick={() => setPicked([])}>
              Clear
            </button>
          </div>
          <p className="flight-deck-summary">
            {picked.length === 0
              ? "Nothing picked."
              : rate > 0
                ? `${picked.length} picked: ${split.now.length} launch this round${
                    split.waits.length > 0 ? `, ${split.waits.length} wait for next round's order` : ""
                  }.`
                : `${picked.length} picked: all through the berths, D3 rounds each, the carrier held still until the last is out.`}
          </p>
          <div className="flight-deck-buttons">
            <button
              type="button"
              className="control-input control-button blue-button"
              disabled={picked.length === 0 || sameAsQueued}
              title="Order the launch: the craft appear at the end of the round, ahead of the carrier, and act from the next"
              onClick={() => dispatch(setLaunch({shipName: args.ship.name, craft: picked}))}>
              Launch
            </button>
            {queued != null && (
              <button
                type="button"
                className="control-input control-button"
                title="Withdraw the launch order"
                onClick={() => dispatch(setLaunch({shipName: args.ship.name, craft: []}))}>
                Withdraw
              </button>
            )}
          </div>
          {queued != null && (
            <p className="flight-deck-queued">
              Launch queued: {queued.length} craft
              {rate > 0 && queued.length > rate ? ` (${rate} this round, ${queued.length - rate} wait)` : ""}.
            </p>
          )}
        </div>
      )}
      {!canOrder && queued != null && (
        <p className="flight-deck-queued">Launch queued: {queued.length} craft.</p>
      )}
    </div>
  );
}

export default FlightDeck;

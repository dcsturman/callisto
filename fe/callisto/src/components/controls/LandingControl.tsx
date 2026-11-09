import * as React from "react";
import {useMemo, useState} from "react";
import {GiAirplaneDeparture} from "react-icons/gi";

import {Ship} from "lib/entities";
import {carriersFor, landingCheck} from "lib/carriers";
import {ViewMode, hasRole} from "lib/view";
import {useAppDispatch, useAppSelector} from "state/hooks";
import {entitiesSelector, templatesSelector} from "state/serverSlice";
import {setLand} from "state/actionsSlice";
import {SectionTag} from "components/controls/SectionTag";

/**
 * The pilot's *Land on <carrier>* order (HG p. 104; FAQ, *Carriers and small
 * craft*), with the engine's gates worked out here so a landing that will be
 * turned away is not offered as if it would not be.
 *
 * What is checked here: own side, pilot's station, Short range and the
 * velocity limit at the end of the round on the committed plans, and a free
 * berth that takes her. What only the engine knows: how many craft the
 * decks have already taken this round (ten a deck), which it reports as a
 * line in Results, and the dice. The odds are the check's terms as the
 * results log itemises them.
 */
export function LandingControl(args: {ship: Ship}) {
  const entities = useAppSelector(entitiesSelector);
  const templates = useAppSelector(templatesSelector);
  const roles = useAppSelector((state) => state.user.roles);
  const actions = useAppSelector((state) => state.actions);
  const dispatch = useAppDispatch();
  const queued = actions[args.ship.name]?.land ?? null;

  const carriers = useMemo(() => carriersFor(args.ship, entities.ships), [args.ship, entities.ships]);
  const [chosen, setChosen] = useState<string | null>(null);
  const carrier =
    carriers.find((c) => c.name === (chosen ?? queued ?? args.ship.carrier)) ?? carriers[0] ?? null;

  // Other craft with Land orders on the same carrier this round, not of her
  // squadron: DM-1 each. Only the queues this console can see.
  const othersLanding = useMemo(
    () =>
      carrier == null
        ? 0
        : Object.entries(actions).filter(([name, slot]) => {
            if (name === args.ship.name || slot?.land !== carrier.name) {
              return false;
            }
            const other = entities.ships.find((s) => s.name === name);
            return args.ship.squadron == null || other?.squadron !== args.ship.squadron;
          }).length,
    [actions, carrier, args.ship, entities.ships],
  );

  const check = useMemo(
    () => (carrier == null ? null : landingCheck(args.ship, carrier, templates, args.ship.plan, othersLanding)),
    [args.ship, carrier, templates, othersLanding],
  );

  // Nothing to land on, and no order to withdraw: a fighter with no carrier
  // in the scenario has no use for this.
  if (carriers.length === 0 && queued == null) {
    return null;
  }
  const canOrder = hasRole(roles, ViewMode.Pilot);

  return (
    <div className="landing-control" data-tutorial="landing-control">
      <SectionTag icon={<GiAirplaneDeparture />}>Landing</SectionTag>
      {queued != null && (
        <p className="landing-queued">
          Landing on {queued}: stands until she is aboard, or the carrier is gone.
          {canOrder && (
            <>
              {" "}
              <button
                type="button"
                className="squadrons-link"
                title="Withdraw the landing order"
                onClick={() => dispatch(setLand({shipName: args.ship.name, carrier: null}))}>
                Withdraw
              </button>
            </>
          )}
        </p>
      )}
      {carrier != null && check != null && (
        <>
          <label className="control-label landing-carrier-row">
            Land on
            <select
              className="salvo-select"
              aria-label="Carrier to land on"
              value={carrier.name}
              onChange={(event) => setChosen(event.target.value)}>
              {carriers.map((c) => (
                <option key={c.name} value={c.name}>
                  {c.name}
                  {c.name === args.ship.carrier ? " (home)" : ""}
                  {c.flight_deck != null ? ` · ${c.flight_deck.berths_free} berth${c.flight_deck.berths_free === 1 ? "" : "s"} free` : ""}
                </option>
              ))}
            </select>
          </label>
          <ul className="landing-gates">
            {check.gates.map((gate) => (
              <li key={gate.what} className={gate.ok ? "landing-gate-ok" : "landing-gate-bad"}>
                {gate.ok ? "✓" : "✗"} {gate.what}
              </li>
            ))}
          </ul>
          {check.deck ? (
            <p className="landing-odds">
              <span className="odds-weapon">Landing check</span>{" "}
              <span
                className="odds-dm"
                title={
                  check.terms.filter(([, v]) => v !== 0).length === 0
                    ? "No modifiers: a plain 2D against 8."
                    : check.terms
                        .filter(([, v]) => v !== 0)
                        .map(([name, v]) => `${name} ${v >= 0 ? "+" : ""}${v}`)
                        .join(", ")
                }>
                2D {check.dm >= 0 ? "+" : ""}
                {check.dm}
              </span>{" "}
              <span className="odds-chance" title={`2D ${check.dm >= 0 ? "+" : ""}${check.dm} against 8 lands ${check.chance} times in 100`}>
                {check.chance}%
              </span>
              <span className="landing-failure" title="On a failure: 1D x (her G + the carrier's G) to both, ignoring armour">
                {" "}
                · failure hurts both
              </span>
            </p>
          ) : null}
          {check.notes.length > 0 && (
            <ul className="landing-notes">
              {check.notes.map((note) => (
                <li key={note}>{note}</li>
              ))}
            </ul>
          )}
          {canOrder && (
            <button
              type="button"
              className="control-input control-button blue-button"
              disabled={queued === carrier.name}
              title={
                check.ok
                  ? "Order the landing: resolved at the end of the round's movement"
                  : "The engine will turn this away as it stands; the order can still be given and stands from round to round"
              }
              onClick={() => dispatch(setLand({shipName: args.ship.name, carrier: carrier.name}))}>
              Land on {carrier.name}
              {check.ok ? "" : " (not yet)"}
            </button>
          )}
        </>
      )}
    </div>
  );
}

export default LandingControl;

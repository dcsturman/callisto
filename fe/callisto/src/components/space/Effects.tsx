import * as React from "react";
import { ShipNameTag, ShipRef } from "components/controls/ShipNameTag";
import {useCallback, useMemo} from "react";
import * as THREE from "three";
import { animated, useSpring } from "@react-spring/three";
import { scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import { findShip } from "lib/entities";
import { teamLabelColor } from "lib/teams";
import { Tooltip } from "react-tooltip";

import { useAppSelector, useAppDispatch } from "state/hooks";
import {
  setShowResults,
  removeEvent,
  clearMessageEvents,
  toggleResultShip,
  toggleResultCategory,
  clearResultFilters,
} from "state/uiSlice";
import { messageStyle, messageRole, roleStyle, MessageRole } from "lib/messages";
import {entitiesSelector} from "state/serverSlice";


const SHIP_IMPACT = "ShipImpact";
const EXHAUSTED_MISSILE = "ExhaustedMissile";
const SHIP_DESTROYED = "ShipDestroyed";
const BEAM_HIT = "BeamHit";
const MESSAGE_EVENT = "Message";

const MISSILE_HIT_COLOR: [number, number, number] = [1.0, 0, 0];
const MISSILE_EXHAUSTED_COLOR: [number, number, number] = [0.65, 0.68, 0.72];
// Orange, so a beam hit reads apart from a missile hit (red).
const BEAM_HIT_COLOR: [number, number, number] = [1.0, 0.55, 0];

// A ship coming apart: white-hot at the centre, fire behind it, and a shell
// that carries on outwards. Nothing else on the board is white, so it does
// not need the old flat blue to tell it from a missile hit.
const SHIP_DESTROYED_FLASH = "#fff4d6";
const SHIP_DESTROYED_FIRE = "#ff7a28";
const SHIP_DESTROYED_SHELL = "#ff4418";

/** The sphere a ship draws as, and the unit every effect is sized in. */
const SHIP_MARKER = 0.2;
/** A hit, in ship markers: big enough to see, small enough to leave the
 *  ship under it visible. */
const HIT_BURST = 3.0;
/** A beam's own hit is smaller, so the beam that caused it still reads. */
const BEAM_BURST = 2.0;
/** A missile going dark is not a hit, and should not flash like one. */
const EXHAUSTED_BURST = 1.5;

// A laser is instantaneous in fiction, but one that clears the screen in a few
// frames is one nobody sees -- particularly a referee watching the whole board.
// It holds at full strength, then fades.
const BEAM_HOLD_MS = 1800;
const BEAM_FADE_MS = 700;
// The beam's radius as a fraction of its own length, so it stays visible
// whether the shot crosses a screen or a pixel. A WebGL line is one pixel wide
// whatever `linewidth` says, which is why this is a solid body rather than one.
const BEAM_RADIUS_FRACTION = 0.004;

/**
 * A ship named in a result: who it was, and the side it was on when the line
 * was written.
 *
 * The team travels with the message because a ship destroyed this round is
 * gone from the entity list by the time its results are read -- and those are
 * the lines that matter most.
 */
export type { ShipRef } from "components/controls/ShipNameTag";

export interface Event {
  /** Set on receipt; unique across rounds. */
  id?: number,
  /** The round this result resolved. Set on receipt from its batch. */
  round?: number,
  kind: string,
  content: string | null,
  // Will only have one of position or target. Position is a concrete position
  // while target is the name of a target.
  position: [number, number, number] | null,
  /**
   * The visual kinds carry a ship's name here; a Message carries the ship its
   * sentence acts on, with the side it was on. One key, two shapes, because
   * both come up the same channel.
   */
  target: string | ShipRef | null,
  origin: [number, number, number] | null,
  // Message events only. `category` drives the colour in the results log;
  // `ship` is the subject of the sentence -- the attacker for an attack, the
  // ship taking it for damage. Both optional: the visual event kinds carry
  // neither, and an older server sends messages without them.
  category?: string,
  /** Message events: the subject of the sentence, substituted for `%s`. */
  ship?: ShipRef | null,
  /** Message events: a third ship the sentence names, substituted for `%c`. */
  contact?: ShipRef | null,
  /**
   * The arithmetic behind the line -- "roll 7 + leadership 4 = 11 vs 8" --
   * kept out of the sentence and shown on a hover.
   */
  roll?: string | null,
  /**
   * Whether the check this message reports passed. Absent on the many
   * messages that state something rather than resolve something -- a missile
   * launch, a range band -- which the log leaves unmarked.
   */
  succeeded?: boolean | null
}

export const createEvent = (kind: string, content: string | null, position: [number, number, number] | null, target: string | null, origin: [number, number, number] | null) => {
  return {kind, content, position, target, origin};
};

export const defaultEvent = () => {
  return createEvent("", null, null, null, null);
};

/**
 * A hit, as an expanding bubble that fades.
 *
 * `radius` is where it stops, in ship markers -- a ship draws as a sphere of
 * SHIP_MARKER units, and a burst is only legible against that. It used to
 * expand to 5 units whatever was happening, twenty-five times the ship it
 * was going off next to, which at the ranges a boarding action or an ambush
 * is fought at swallowed the whole engagement and hid the beam that caused
 * it.
 */
export function Explosion(args: {
  center: [number, number, number];
  color: [number, number, number];
  cleanupFn: () => void;
  radius?: number;
}) {
  const { scale, opacity } = useSpring({
    from: { scale: 0.0, opacity: 1.0 },
    to: [{ scale: args.radius ?? HIT_BURST, opacity: 0.0 }],
    onResolve: (result) => {
      if (result.finished) {
        args.cleanupFn();
      }
    },
    config: {
      mass: 50,
      tension: 280,
      friction: 180,
    },
  });

  return (
    <animated.mesh scale={scale} position={scaleVector(args.center, SCALE)}>
      <sphereGeometry args={[SHIP_MARKER]} />
      <animated.meshStandardMaterial transparent={true} color={args.color} opacity={opacity} />
    </animated.mesh>
  );
}

/**
 * A ship coming apart, in three layers that outlive each other: a white-hot
 * flash, a fireball behind it, and a shell that keeps going after both have
 * gone. It runs about three seconds, where a hit is gone in half of one, and
 * it is the one effect on the board worth stopping to watch.
 *
 * Additive blending on the first two, so where they overlap they burn out to
 * white the way an explosion does rather than turning muddy.
 */
export function ShipExplosion(args: {
  center: [number, number, number];
  cleanupFn: () => void;
}) {
  const at = scaleVector(args.center, SCALE);

  const flash = useSpring({
    from: { scale: 0.4, opacity: 1.0 },
    to: [
      { scale: 3.0, opacity: 0.9 },
      { scale: 4.0, opacity: 0.0 },
    ],
    config: { duration: 170 },
  });

  const fireball = useSpring({
    from: { scale: 0.3, opacity: 0.95 },
    to: [
      { scale: 5.0, opacity: 0.7 },
      { scale: 8.0, opacity: 0.0 },
    ],
    config: { duration: 650 },
  });

  // The last to go, and what clears the event.
  const shell = useSpring({
    from: { scale: 1.0, opacity: 0.45 },
    to: [{ scale: 14.0, opacity: 0.0 }],
    delay: 120,
    config: { duration: 2600 },
    onResolve: (result) => {
      if (result.finished) {
        args.cleanupFn();
      }
    },
  });

  return (
    <>
      <animated.mesh scale={shell.scale} position={at}>
        <sphereGeometry args={[SHIP_MARKER, 24, 16]} />
        <animated.meshBasicMaterial
          transparent={true}
          color={SHIP_DESTROYED_SHELL}
          opacity={shell.opacity}
          depthWrite={false}
        />
      </animated.mesh>
      <animated.mesh scale={fireball.scale} position={at}>
        <sphereGeometry args={[SHIP_MARKER, 24, 16]} />
        <animated.meshBasicMaterial
          transparent={true}
          color={SHIP_DESTROYED_FIRE}
          opacity={fireball.opacity}
          blending={THREE.AdditiveBlending}
          depthWrite={false}
        />
      </animated.mesh>
      <animated.mesh scale={flash.scale} position={at}>
        <sphereGeometry args={[SHIP_MARKER, 24, 16]} />
        <animated.meshBasicMaterial
          transparent={true}
          color={SHIP_DESTROYED_FLASH}
          opacity={flash.opacity}
          blending={THREE.AdditiveBlending}
          depthWrite={false}
        />
      </animated.mesh>
    </>
  );
}

export function Beam(args: {
  origin: [number, number, number];
  end: [number, number, number];
  color: [number, number, number];
  cleanupFn: () => void;
}) {
  // A cylinder from the firing ship to where the shot landed: built along Y,
  // then turned to point down the line of fire.
  const { center, quaternion, length } = useMemo(() => {
    const from = new THREE.Vector3(...scaleVector(args.origin, SCALE));
    const to = new THREE.Vector3(...scaleVector(args.end, SCALE));
    const along = new THREE.Vector3().subVectors(to, from);
    return {
      center: new THREE.Vector3().addVectors(from, to).multiplyScalar(0.5),
      quaternion: new THREE.Quaternion().setFromUnitVectors(
        new THREE.Vector3(0, 1, 0),
        along.clone().normalize()
      ),
      length: along.length(),
    };
  }, [args.origin, args.end]);

  const { opacity } = useSpring({
    from: { opacity: 1.0 },
    to: { opacity: 0.0 },
    delay: BEAM_HOLD_MS,
    config: { duration: BEAM_FADE_MS },
    onResolve: (result) => {
      if (result.finished) {
        args.cleanupFn();
      }
    },
  });

  const radius = length * BEAM_RADIUS_FRACTION;

  return (
    <animated.mesh position={center} quaternion={quaternion}>
      <cylinderGeometry args={[radius, radius, length, 8, 1, true]} />
      <animated.meshBasicMaterial
        color={args.color}
        transparent={true}
        opacity={opacity}
        side={THREE.DoubleSide}
        depthWrite={false}
      />
    </animated.mesh>
  );
}

export function Explosions() {
  const entities = useAppSelector(entitiesSelector);
  const events = useAppSelector(state => state.ui.events);
  const dispatch = useAppDispatch();

  return (
    <>
      {events?.map((event, index) => {
        let color: [number, number, number] = [0, 0, 0];
        let key: string = "";
        let removeMe: () => void = () => {};
        let position: [number, number, number] = [0, 0, 0];

        switch (event.kind) {
          case SHIP_IMPACT:
            // Use the current position of the target if we can find it; otherwise use the position (last known position actually) as a backup
            position =
              findShip(entities, typeof event.target === "string" ? event.target : (event.target?.name ?? ""))
                ?.position ??
              event.position ??
              [0, 0, 0];
            color = MISSILE_HIT_COLOR;
            key = "Impact-" + (event.id ?? index);
            removeMe = () => {
              if (event.id != null) {
                dispatch(removeEvent(event.id));
              }
            };
            console.log("(Explosions) key: " + key);
            return (
              <Explosion
                key={key}
                center={position}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case EXHAUSTED_MISSILE:
            color = MISSILE_EXHAUSTED_COLOR;
            key = "Gone-" + (event.id ?? index);
            removeMe = () => {
              if (event.id != null) {
                dispatch(removeEvent(event.id));
              }
            };
            return (
              <Explosion
                key={key}
                center={(event.position?? [0, 0, 0])}
                color={color}
                radius={EXHAUSTED_BURST}
                cleanupFn={removeMe}
              />
            );
          case SHIP_DESTROYED:
            key = "Destroyed-" + (event.id ?? index);
            removeMe = () => {
              if (event.id != null) {
                dispatch(removeEvent(event.id));
              }
            };
            return (
              <ShipExplosion
                key={key}
                center={(event.position?? [0, 0, 0])}
                cleanupFn={removeMe}
              />
            );
          case BEAM_HIT:
            color = BEAM_HIT_COLOR;
            key = "Beam-" + (event.id ?? index);
            removeMe = () => {
              if (event.id != null) {
                dispatch(removeEvent(event.id));
              }
            };
            // The beam and an explosion where it lands. The beam is on screen
            // the longest, so it is the one that clears the event.
            return (
              <React.Fragment key={key}>
                <Beam
                  origin={(event.origin?? [0, 0, 0])}
                  end={(event.position?? [0, 0, 0])}
                  color={color}
                  cleanupFn={removeMe}
                />
                <Explosion
                  center={event.position ?? [0, 0, 0]}
                  color={color}
                  radius={BEAM_BURST}
                  cleanupFn={() => {}}
                />
              </React.Fragment>
            );
          case MESSAGE_EVENT:
            // DamageEffects don't show up as explosions so skip.
            return null;
          default:
            console.error(
              `(Effects.Effects) Unknown effect kind: ${
                event.kind
              } (${JSON.stringify(event)})`
            );
            return null;
        }
      })}
    </>
  );
}

/**
 * A result's sentence, with the ships put back into it.
 *
 * The server writes `%s` for the subject, `%t` for the ship acted on and `%c`
 * for a third party, and sends each as a name and a side. Splitting on the
 * placeholders is what lets a ship be drawn in its own colour rather than
 * being a run of text in the middle of a sentence.
 *
 * A message with no placeholders -- anything not yet converted -- prints as
 * it always did.
 */
function ResultText(args: {event: Event}) {
  const {content, ship, target, contact} = args.event;
  const named: Record<string, ShipRef | null | undefined> = {
    "%s": ship,
    "%t": typeof target === "string" ? {name: target} : target,
    "%c": contact,
  };
  const pieces = (content ?? "").split(/(%[stc])/g);
  return (
    <>
      {pieces.map((piece, index) => {
        const who = named[piece];
        return who == null ? (
          <React.Fragment key={index}>{piece}</React.Fragment>
        ) : (
          <ShipNameTag key={index} ship={who} />
        );
      })}
    </>
  );
}

/** Every ship a message names, in the order the sentence names them. */
function shipsIn(event: Event): ShipRef[] {
  const target = typeof event.target === "string" ? {name: event.target} : event.target;
  return [event.ship, target, event.contact].filter((who): who is ShipRef => who != null);
}

/**
 * The filter: which ships' results to show, and which kinds.
 *
 * Chips rather than a dropdown, because the question is "these two ships" as
 * often as "this one" -- a referee watching a boarding action, a sensop
 * watching every sensor check on one hull. Only the ships and kinds actually
 * in this round's results get a chip, so the rows stay short.
 */
function ResultFilter(args: {events: Event[]}) {
  const dispatch = useAppDispatch();
  const ships = useAppSelector((state) => state.ui.resultShips);
  const categories = useAppSelector((state) => state.ui.resultCategories);

  const present = useMemo(() => {
    const byName = new Map<string, ShipRef>();
    const kinds = new Set<MessageRole>();
    for (const event of args.events) {
      for (const who of shipsIn(event)) {
        if (!byName.has(who.name)) {
          byName.set(who.name, who);
        }
      }
      kinds.add(messageRole(event.category));
    }
    return {
      ships: [...byName.values()].sort((a, b) => a.name.localeCompare(b.name)),
      kinds: [...kinds].sort(),
    };
  }, [args.events]);

  if (present.ships.length <= 1 && present.kinds.length <= 1) {
    return null;
  }

  const filtering = ships.length > 0 || categories.length > 0;

  return (
    <div className="result-filter">
      <div className="result-filter-row">
        {present.ships.map((who) => {
          const on = ships.includes(who.name);
          return (
            <button
              key={who.name}
              type="button"
              className={on ? "result-chip result-chip-on" : "result-chip"}
              style={{color: teamLabelColor(who.team ?? null)}}
              title={on ? `Showing ${who.name}` : `Show only ${who.name} and whoever else is chosen`}
              onClick={() => dispatch(toggleResultShip(who.name))}>
              {who.name}
            </button>
          );
        })}
      </div>
      <div className="result-filter-row">
        {present.kinds.map((role) => {
          const on = categories.includes(role);
          return (
            <button
              key={role}
              type="button"
              className={on ? "result-chip result-chip-on" : "result-chip"}
              style={roleStyle(role)}
              title={
                on ? `Showing the ${role}'s work` : `Show only the ${role}'s work, and whatever else is chosen`
              }
              onClick={() => dispatch(toggleResultCategory(role))}>
              {role}
            </button>
          );
        })}
        {filtering && (
          <button
            type="button"
            className="result-chip result-chip-all"
            title="Show everything again"
            onClick={() => dispatch(clearResultFilters())}>
            all
          </button>
        )}
      </div>
    </div>
  );
}

/**
 * The turn's results.
 *
 * Coloured by category rather than by keyword: the server says what each line
 * is, so rewording a message never silently changes how it looks.
 */
export function ResultsWindow() {
  const events = useAppSelector(state => state.ui.events);
  const dispatch = useAppDispatch();

  const closeWindow = useCallback(() => {
    dispatch(clearMessageEvents());
    dispatch(setShowResults(false));
  }, [dispatch]);

  const messages = useMemo(() => events?.filter((event) => event.kind === MESSAGE_EVENT) ?? [], [events]);
  const shipFilter = useAppSelector((state) => state.ui.resultShips);
  const categoryFilter = useAppSelector((state) => state.ui.resultCategories);

  // Chosen ships and chosen kinds, combining with and: "sensors, on this one
  // hull" is the case the two rows exist for. Nothing chosen means everything.
  const shown = useMemo(
    () =>
      messages.filter((event) => {
        const byShip =
          shipFilter.length === 0 || shipsIn(event).some((who) => shipFilter.includes(who.name));
        const byCategory =
          categoryFilter.length === 0 || categoryFilter.includes(messageRole(event.category));
        return byShip && byCategory;
      }),
    [messages, shipFilter, categoryFilter]
  );

  return (
    <div id="results-window" className="computer-window" data-tutorial="results">
      <h1>Results</h1>
      <br></br>
      {messages.length === 0 && <h2>No results</h2>}
      {messages.length > 0 && <ResultFilter events={messages} />}
      {messages.length > 0 && shown.length === 0 && (
        <p className="sensor-empty">Nothing matches the filter.</p>
      )}
      {/* A tick or a cross against everything that was a check, so a round
          can be read at a glance. Lines that merely report something are
          indented to the same text column rather than sitting under the
          marks. */}
      {shown.map((msg, index) => (
        <p key={"msg-" + index} className="result-line" style={messageStyle(msg.category)}>
          {msg.succeeded == null ? (
            <span className="result-mark result-mark-none" aria-hidden="true" />
          ) : (
            <span
              className={msg.succeeded ? "result-mark result-pass" : "result-mark result-fail"}
              role="img"
              aria-label={msg.succeeded ? "succeeded" : "failed"}>
              {msg.succeeded ? "\u2713" : "\u2715"}
            </span>
          )}
          <span
            className={msg.roll == null ? "result-text" : "result-text result-text-rolled"}
            data-tooltip-id="result-roll"
            data-tooltip-content={msg.roll ?? undefined}
            data-tooltip-delay-show={200}>
            <ResultText event={msg} />
          </span>
        </p>
      ))}
      {/* The arithmetic behind a line, on the line's own hover: a round is
          read for what happened, and the numbers are what you go back for. */}
      <Tooltip id="result-roll" className="console-tip console-tip-figures" />
      {/* Pinned, because the panel scrolls: a round with twenty checks in
          it should not hide the way to dismiss them. */}
      <button
        className="control-input control-button blue-button button-next-round results-dismiss"
        onClick={closeWindow}>
        Okay!
      </button>
    </div>
  )
}
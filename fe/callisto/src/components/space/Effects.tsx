import * as React from "react";
import {useCallback, useMemo} from "react";
import * as THREE from "three";
import { animated, useSpring } from "@react-spring/three";
import { scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import { findShip } from "lib/entities";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setShowResults, removeEvent, clearMessageEvents } from "state/uiSlice";
import { messageStyle } from "lib/messages";
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

export interface Event {
  /** Set on receipt; unique across rounds. */
  id?: number,
  kind: string,
  content: string | null,
  // Will only have one of position or target. Position is a concrete position
  // while target is the name of a target.
  position: [number, number, number] | null,
  target: string | null,
  origin: [number, number, number] | null,
  // Message events only. `category` drives the colour in the results log;
  // `ship` is the subject of the sentence -- the attacker for an attack, the
  // ship taking it for damage. Both optional: the visual event kinds carry
  // neither, and an older server sends messages without them.
  category?: string,
  ship?: string | null,
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
            position = findShip(entities, event.target)?.position ?? event.position ?? [0, 0, 0];
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

  return (
    <div id="results-window" className="computer-window">
      <h1>Results</h1>
      <br></br>
      {messages.length === 0 && <h2>No results</h2>}
      {/* A tick or a cross against everything that was a check, so a round
          can be read at a glance. Lines that merely report something are
          indented to the same text column rather than sitting under the
          marks. */}
      {messages.length > 0 && messages.map((msg, index) => (
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
          <span className="result-text">{msg.content}</span>
        </p>
      ))}
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
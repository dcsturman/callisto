import * as React from "react";
import { animated, useSpring } from "@react-spring/three";
import { scaleVector } from "lib/Util";
import { SCALE } from "lib/universal";
import { GrowLine } from "lib/Util";
import { findShip } from "lib/entities";

import { useAppSelector, useAppDispatch } from "state/hooks";
import { setShowResults, setEvents } from "state/uiSlice";
import {entitiesSelector} from "state/serverSlice";


const SHIP_IMPACT = "ShipImpact";
const EXHAUSTED_MISSILE = "ExhaustedMissile";
const SHIP_DESTROYED = "ShipDestroyed";
const BEAM_HIT = "BeamHit";
const MESSAGE_EVENT = "Message";

const MISSILE_HIT_COLOR: [number, number, number] = [1.0, 0, 0];
const MISSILE_EXHAUSTED_COLOR: [number, number, number] = [1.0, 1.0, 1.0];
const SHIP_DESTROYED_COLOR: [number, number, number] = [0.0, 0.0, 1.0];
const BEAM_HIT_COLOR: [number, number, number] = [1.0, 0, 0];

export interface Event {
  kind: string,
  content: string | null,
  // Will only have one of position or target. Position is a concrete position
  // while target is the name of a target.
  position: [number, number, number] | null,
  target: string | null,
  origin: [number, number, number] | null
}

export const createEvent = (kind: string, content: string | null, position: [number, number, number] | null, target: string | null, origin: [number, number, number] | null) => {
  return {kind, content, position, target, origin};
};

export const defaultEvent = () => {
  return createEvent("", null, null, null, null);
};

export function Explosion(args: {
  center: [number, number, number];
  color: [number, number, number];
  cleanupFn: () => void;
}) {
  const { scale, opacity } = useSpring({
    from: { scale: 0.0, opacity: 1.0 },
    to: [{ scale: 100.0, opacity: 0.0 }],
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
      <sphereGeometry args={[0.05]} />
      <animated.meshStandardMaterial transparent={true} color={args.color} opacity={opacity} />
    </animated.mesh>
  );
}

export function Beam(args: {
  origin: [number, number, number];
  end: [number, number, number];
  color: [number, number, number];
  cleanupFn: () => void;
}) {

  const AnimatedLine = animated(GrowLine);

  const { scale } = useSpring({
    from: { scale: 0.0 },
    to: [ { scale: 1.0 }],
    onResolve: (result) => {
      if (result.finished) {
        args.cleanupFn();
      }
    },
    config: {
      mass: 10,
      tension: 180,
      friction: 40,
    },
  });

  return (
    <AnimatedLine start={scaleVector(args.origin, SCALE)} end={scaleVector(args.end, SCALE)} scale={scale} color={args.color} />
  )
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
            key = "Impact-" + index;
            removeMe = () => {
              dispatch(setEvents(events.filter((e) => e !== event)));
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
            key = "Gone-" + index;
            removeMe = () => {
              dispatch(setEvents(events.filter((e) => e !== event)));
            };
            return (
              <Explosion
                key={key}
                center={(event.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case SHIP_DESTROYED:
            color = SHIP_DESTROYED_COLOR;
            key = "Destroyed-" + index;
            removeMe = () => {
              dispatch(setEvents(events.filter((e) => e !== event)));
            };
            return (
              <Explosion
                key={key}
                center={(event.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case BEAM_HIT:
            color = BEAM_HIT_COLOR;
            key = "Beam-" + index;
            removeMe = () => {
              dispatch(setEvents(events.filter((e) => e !== event)));
            };
            return (
              <Beam
                key={key}
                origin={(event.origin?? [0, 0, 0])}
                end={(event.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
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

export function ResultsWindow() {
  const events = useAppSelector(state => state.ui.events);
  const dispatch = useAppDispatch();

  function closeWindow() {
    if (events !== null) {
      dispatch(setEvents(events.filter((event) => event.kind !== MESSAGE_EVENT)));
    }
    dispatch(setShowResults(false));
  }

  let messages: Event[] = [];
  if (events !== null) {
    messages = events?.filter((event) => event.kind === MESSAGE_EVENT);
  }
  return (
    <div id="results-window" className="computer-window">
      <h1>Results</h1>
      <br></br>
      {messages.length === 0 && <h2>No results</h2>}
      {messages.length > 0 && messages.map((msg, index) => (<p key={"msg-" + index}>{msg.content}</p>))}
      <button className="control-input control-button blue-button button-next-round" onClick={closeWindow}>Okay!</button>
    </div>
  )
}
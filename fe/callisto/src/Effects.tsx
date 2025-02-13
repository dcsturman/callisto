import * as React from "react";
import { animated, useSpring } from "@react-spring/three";
import { scaleVector } from "./Util";
import { SCALE } from "./Universal";
import { GrowLine } from "./Util";

const SHIP_IMPACT = "ShipImpact";
const EXHAUSTED_MISSILE = "ExhaustedMissile";
const SHIP_DESTROYED = "ShipDestroyed";
const BEAM_HIT = "BeamHit";
const MESSAGE_EFFECT = "Message";

const MISSILE_HIT_COLOR: [number, number, number] = [1.0, 0, 0];
const MISSILE_EXHAUSTED_COLOR: [number, number, number] = [1.0, 1.0, 1.0];
const SHIP_DESTROYED_COLOR: [number, number, number] = [0.0, 0.0, 1.0];
const BEAM_HIT_COLOR: [number, number, number] = [1.0, 0, 0];

export class Effect {
  kind: string = "ShipImpact";
  content: string | null = null;
  position: [number, number, number] | null = [0, 0, 0];
  origin: [number, number, number] | null = [0, 0, 0];

  /* eslint-disable-next-line @typescript-eslint/no-explicit-any */
  static parse(json: any): Effect {
    const e = new Effect();
    e.kind = json.kind;
    e.content = json.content ?? null;
    e.position = json.position ?? null;
    e.origin = json.origin ?? null;
    return e;
  }
}

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
export function Explosions(args: {
  effects: Effect[];
  setEffects: (entities: Effect[] | null) => void;
}) {
  console.log("(Effects.Explosions) Effects: " + JSON.stringify(args.effects));

  return (
    <>
      {args.effects.map((effect, index) => {
        let color: [number, number, number] = [0, 0, 0];
        let key: string = "";
        let removeMe: () => void = () => {};

        switch (effect.kind) {
          case SHIP_IMPACT:
            color = MISSILE_HIT_COLOR;
            key = "Impact-" + index;
            removeMe = () => {
              args.setEffects(args.effects.filter((e) => e !== effect));
            };
            return (
              <Explosion
                key={key}
                center={effect.position?? [0, 0, 0]}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case EXHAUSTED_MISSILE:
            color = MISSILE_EXHAUSTED_COLOR;
            key = "Gone-" + index;
            removeMe = () => {
              args.setEffects(args.effects.filter((e) => e !== effect));
            };
            return (
              <Explosion
                key={key}
                center={(effect.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case SHIP_DESTROYED:
            color = SHIP_DESTROYED_COLOR;
            key = "Destroyed-" + index;
            removeMe = () => {
              args.setEffects(args.effects.filter((e) => e !== effect));
            };
            return (
              <Explosion
                key={key}
                center={(effect.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case BEAM_HIT:
            color = BEAM_HIT_COLOR;
            key = "Beam-" + index;
            removeMe = () => {
              args.setEffects(args.effects.filter((e) => e !== effect));
            };
            return (
              <Beam
                key={key}
                origin={(effect.origin?? [0, 0, 0])}
                end={(effect.position?? [0, 0, 0])}
                color={color}
                cleanupFn={removeMe}
              />
            );
          case MESSAGE_EFFECT:
            // DamageEffects don't show up as explosions so skip.
            return <></>;
          default:
            console.error(
              `(Effects.Effects) Unknown effect kind: ${
                effect.kind
              } (${JSON.stringify(effect)})`
            );
            return <></>;
        }
      })}
    </>
  );
}

export function ResultsWindow(args: {
  clearShowResults: () => void,
  effects: Effect[] | null,
  setEffects: (entities: Effect[] | null) => void
}) {
  function closeWindow() {
    if (args.effects !== null) {
      args.setEffects(args.effects.filter((effect) => effect.kind !== MESSAGE_EFFECT));
    }
    args.clearShowResults();
  }

  let messages: Effect[] = [];
  if (args.effects !== null) {
    messages = args.effects?.filter((effect) => effect.kind === MESSAGE_EFFECT);
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
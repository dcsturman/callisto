import { useRef, useState } from "react";
import * as THREE from "three";
import { create } from "zustand";
import { animated, useSpring, config } from "@react-spring/three";
import { scaleVector } from "./Util";
import { SCALE } from "./Contexts";

const UNKNOWN_KIND = "Unknown";
const SHIP_IMPACT_KIND = "ShipImpact";
const EXHAUSTED_MISSILE = "ExhaustedMissile";

const MISSILE_HIT_COLOR: [number, number, number] = [1.0, 0, 0];
const MISSILE_EXHAUSTED_COLOR: [number, number, number] = [1.0, 1.0, 1.0];

export class Effect {
  position: [number, number, number] = [0, 0, 0];
  kind: String = UNKNOWN_KIND;
}

export function Explosion(args: {
  center: [number, number, number];
  color: [number, number, number];
  cleanupFn: () => void;
}) {
  const myMat = useRef<THREE.MeshBasicMaterial>(null!);
  const { scale } = useSpring({
    from: { scale: 0.0 },
    to: [{ scale: 100.0 }, { scale: 0.0 }],
    onResolve: (result) => {
      if (result.finished) {
        args.cleanupFn();
      }
    },
    config: {
      tension: 180,
      friction: 60,
    },
  });

  return (
    <animated.mesh scale={scale} position={scaleVector(args.center, SCALE)}>
      <sphereGeometry args={[0.05]} />
      <meshBasicMaterial
        transparent={true}
        opacity={0.4}
        color={args.color}
        ref={myMat}
      />
    </animated.mesh>
  );
}

export function Effects(args: {
  effects: Effect[];
  setEffects: (entities: Effect[] | null) => void;
}) {
  console.log("(Effects.Effects) Effects: " + JSON.stringify(args.effects));

  return (
    <>
      {args.effects.map((effect, index) => {
        let color: [number, number, number] = [0.0, 0.0, 0.0];

        switch (effect.kind) {
          case SHIP_IMPACT_KIND:
            color = MISSILE_HIT_COLOR;
            break;
          case EXHAUSTED_MISSILE:
            color = MISSILE_EXHAUSTED_COLOR;
            break;
          default:
            console.log(
              "(Effects.Effects) Unknown effect kind: " + effect.kind
            );
            break;
        }

        let key = "Boom-" + index;
        let removeMe = () => {
          args.setEffects(args.effects.filter((e) => e !== effect));
        };
        return (
          <Explosion
            key={key}
            center={effect.position}
            color={color}
            cleanupFn={removeMe}
          />
        );
      })}
    </>
  );
}
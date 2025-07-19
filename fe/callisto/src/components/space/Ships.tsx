import React from "react";
import { useContext, useRef } from "react";

import { Group, Mesh, SphereGeometry } from "three";
import {
  extend,
  ReactThreeFiber,
  useThree,
  useFrame,
  Vector3,
} from "@react-three/fiber";
import { TextGeometry } from "three/examples/jsm/geometries/TextGeometry";
import { FontLoader, Font } from "three/examples/jsm/loaders/FontLoader";

import { EffectComposer, Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "lib/Util";

import {
  EntitiesServerContext,
  FlightPathResult,
  Ship as ShipType,
  Missile as MissileType,
  SCALE,
  TURN_IN_SECONDS,
  EntityToShowContext,
  RANGE_BANDS
} from "../../lib/universal";

import { addVector, scaleVector, RangeSphere } from "../../lib/Util";


extend({ TextGeometry });

// Needed for some reason to make textGeometry work.
declare module '@react-three/fiber' {
  interface ThreeElements {
    textGeometry: ReactThreeFiber.Object3DNode<TextGeometry, typeof TextGeometry>;
  }
}

let labelFont: Font | null = null;
new FontLoader().load(
  "/assets/Orbitron_Regular.json",
  (font) => {
    labelFont = font;
  },
  () => {},
  (error) => {
    console.log("Error loading Orbitron font: " + JSON.stringify(error));
  }
);

function Ship(args: {
  ship: ShipType;
  index: number;
  setComputerShipName: (ship_name: string | null) => void;
  showRange: string | null;
}) {
  const entityToShow = useContext(EntityToShowContext);
  const { camera } = useThree();
  const textRef = useRef<Mesh>(null);
  const shipRef = useRef<Mesh>(null);
  const textGeoRef = useRef<TextGeometry>(null);
  const shipGeoRef = useRef<SphereGeometry>(null);

  useFrame(() => {
    textRef.current?.lookAt(camera.position);
  });
  function handleShipClick() {
    args.setComputerShipName(args.ship.name);
  }

  const showRange = args.showRange && args.showRange === args.ship.name;

  return (
    <>
      {showRange && RANGE_BANDS.map(
          (distance, index) => (
              <RangeSphere
                pos={scaleVector(args.ship.position, SCALE)}
                distance={distance}
                order={2*index}
                key={showRange + "range" + index}
                color={"#5ba0ff"}
                opacity={0.18}
              />
          )
        )}
      {
        <EffectComposer>
          <Bloom
            mipmapBlur
            luminanceThreshold={1}
            luminanceSmoothing={1}
            intensity={1.0}
          />
        </EffectComposer>
      }
      <group position={scaleVector(args.ship.position, SCALE) as Vector3}>
        <mesh
          ref={shipRef}
          position={[0, 0, 0]}
          onPointerOver={() => entityToShow.setEntityToShow(args.ship)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}
          onClick={handleShipClick}>
          <sphereGeometry ref={shipGeoRef} args={[0.2]} />
          <meshBasicMaterial color={[3, 3, 8.0]} />
        </mesh>
        {/* vector showing a ships's velocity (so distance next turn) */}
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)}
          color="red"
        />
        {/* vector showing a ships's planed move in the next turn */}
        <Line
          start={scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)}
          end={addVector(
            scaleVector(
              args.ship.plan[0][0] as [number, number, number],
              SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS
            ),
            scaleVector(args.ship.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        {labelFont != null && (
          <mesh position={[0.0, -1.5, 0.0]} ref={textRef}>
            <textGeometry
              ref={textGeoRef}
              args={[
                args.ship.name,
                { font: labelFont, size: 0.7, depth: 0.05 },
              ]}
            />
            <meshBasicMaterial attach="material" color="#3dfc32" />
          </mesh>
        )}
      </group>
    </>
  );
}

export function Ships(args: {
  setComputerShipName: (ship_name: string | null) => void;
  showRange: string | null;
}) {
  const serverEntities = useContext(EntitiesServerContext);
  return (
    <>
      {serverEntities.entities.ships.map((ship, index) => (
        <Ship
          key={ship.name}
          ship={ship}
          index={index}
          setComputerShipName={args.setComputerShipName}
          showRange={args.showRange}
        />
      ))}
    </>
  );
}

export function Missile(args: { missile: MissileType; index: number }) {
  const entityToShow = useContext(EntityToShowContext);
  const labelRef = useRef<Group>(null);

  return (
    <>
      {/*
      <EffectComposer>
        <Bloom
          mipmapBlur
          luminanceThreshold={1}
          luminanceSmoothing={1}
          intensity={5.0}
      </EffectComposer>
      />*/}
      <group
        ref={labelRef}
        position={scaleVector(args.missile.position, SCALE) as Vector3}>
        <mesh
          position={[0, 0, 0]}
          onPointerOver={() => entityToShow.setEntityToShow(args.missile)}
          onPointerLeave={() => entityToShow.setEntityToShow(null)}>
          <sphereGeometry args={[0.1]} />
          <meshBasicMaterial color={[8.0, 0, 0]} />
        </mesh>
        {/* vector showing a missile's velocity (so distance next turn) */}
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)}
          color="grey"
        />
        {/* vector showing a missile's planed move in the next turn */}
        <Line
          start={scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)}
          end={addVector(
            scaleVector(
              args.missile.acceleration,
              SCALE * TURN_IN_SECONDS * TURN_IN_SECONDS
            ),
            scaleVector(args.missile.velocity, SCALE * TURN_IN_SECONDS)
          )}
          color="green"
        />
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]}>
          {args.missile.name}
        </Text>
      </group>
    </>
  );
}

export function Missiles() {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.entities.missiles.map((missile, index) => (
        <Missile key={missile.name} missile={missile} index={index} />
      ))}
    </>
  );
}

export function Route(args: { plan: FlightPathResult }) {
  const start = scaleVector(args.plan.path[0], -1.0 * SCALE);
  let prev = args.plan.path[0];
  const path = args.plan.path.slice(1);

  return (
    <group position={scaleVector(prev, SCALE) as Vector3}>
      {path.map((point, index) => {
        const oldPoint = prev;
        prev = point;
        return (
          <Line
            key={index}
            start={addVector(start, scaleVector(oldPoint, SCALE))}
            end={addVector(start, scaleVector(point, SCALE))}
            color={"orange"}
          />
        );
      })}
    </group>
  );
}

import * as THREE from "three";
import { useContext, useRef, useState } from "react";
import { Tooltip } from "react-tooltip";

import { Mesh, Group } from "three";
import { useFrame, useThree } from "@react-three/fiber";
import { Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import { Entity, EntitiesServerContext } from "./Contexts";
import { Vector3 } from "@react-three/fiber";

import { scale, timeUnit, G } from "./Contexts";

/* Entities come in from the server with all units in meters (m). Convert them to the units we can use on screen. */
function scaleVector(
  v: [number, number, number],
  scale: number
): [number, number, number] {
  return v.map((x) => x * scale) as [number, number, number];
}

function addVector(a: [number, number, number], b: [number, number, number]) {
  return a.map((x, i) => x + b[i]) as [number, number, number];
}

function vectorToString(v: [number, number, number]) {
  return `${v[0].toFixed(0)}, ${v[1].toFixed(0)}, ${v[2].toFixed(0)}`;
}

export function ShipInfoWindow(args: { ship: Entity}) {
  return (
    <div className="ship-info-window">
      <h2 className="ship-info-title">{args.ship.name}</h2>
      <div className="ship-info-content">
        <p>Position (km): {vectorToString(scaleVector(args.ship.position,1e-3))}</p>
        <p>Velocity (m/s): {vectorToString(args.ship.velocity)}</p>
        <p>Acceleration (G): {vectorToString(args.ship.acceleration)}</p>
      </div>
    </div>
  );
}

function Ship(args: { ship: Entity; index: number; setShipToShow: (ship: Entity | null) => void}) {
  const { camera } = useThree();
  const labelRef = useRef<Group>(null);

  useFrame(() => {
    // Attempt to push the label the right way, but moving everything
    //labelRef.current?.lookAt(camera.position);
  });

  {
    console.log("Ship: " + JSON.stringify(args.ship));
  }

  return (
    <>
      {
        <Bloom
          mipmapBlur
          luminanceThreshold={1}
          luminanceSmoothing={1}
          intensity={5.0}
        />
      }
      <group ref={labelRef} position={scaleVector(args.ship.position, scale) as Vector3}>
        <mesh position={[0, 0, 0]} onPointerOver={()=> args.setShipToShow(args.ship) }
        onPointerLeave={()=> args.setShipToShow(null) } >
          <sphereGeometry args={[1.0]} />
          <meshBasicMaterial color={[3, 3, 8.0]} />
        </mesh>
        <Line
          start={[0, 0, 0]}
          end={scaleVector(args.ship.velocity, scale * timeUnit)}
          color="red"
        />
        <Line
          start={scaleVector(args.ship.velocity, scale * timeUnit)}
          end={addVector(
            scaleVector(args.ship.acceleration, scale * timeUnit * timeUnit),
            scaleVector(args.ship.velocity, scale * timeUnit)
          )}
          color="green"
        />
        <Text color="grey" fontSize={0.08} position={[0, -0.1, 0]}>
          {args.ship.name}
        </Text>
      </group>
    </>
  );
}

export function Ships(args: { setShipToShow: (ship: Entity | null) => void }) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.map((entity, index) => (
        <Ship key={entity.name} ship={entity} index={index} setShipToShow={args.setShipToShow}/>
      ))}
    </>
  );
}
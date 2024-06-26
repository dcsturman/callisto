import { useContext, useRef, useState } from "react";

import { Group } from "three";
import { Bloom } from "@react-three/postprocessing";
import { Text } from "@react-three/drei";
import { Line } from "./Util";

import { Entity, EntitiesServerContext, FlightPlan } from "./Contexts";
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

function Ship(args: { ship: Entity; index: number; setShipToShow: (ship: Entity | null) => void; setComputerShip: (ship: Entity | null) => void}) {
  const labelRef = useRef<Group>(null);

  function handleShipClick() {
    args.setComputerShip(args.ship);
  }

  console.log("Ship: " + JSON.stringify(args.ship));

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
        onPointerLeave={()=> args.setShipToShow(null)
        } onClick={handleShipClick}>
          <sphereGeometry args={[0.05]} />
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
        <Text color="grey" fontSize={0.2} position={[0, -0.1, 0]} >
          {args.ship.name}
        </Text>
      </group>
    </>
  );
}

export function Ships(args: { setShipToShow: (ship: Entity | null) => void; setComputerShip: (ship: Entity | null) => void}) {
  const serverEntities = useContext(EntitiesServerContext);

  return (
    <>
      {serverEntities.map((entity, index) => (
        <Ship key={entity.name} ship={entity} index={index} setShipToShow={args.setShipToShow} setComputerShip={args.setComputerShip} />
      ))}
    </>
  );
}

export function Route(args: { plan: FlightPlan }) {
  console.log(`(Ships.Route) Display plan: ${JSON.stringify(args.plan)}`);
  console.log(`(Ships.Route) Display route: ${JSON.stringify(args.plan.path)}`);
  let start = scaleVector(args.plan.path[0], -1.0*scale);
  let prev = args.plan.path[0];
  let path = args.plan.path.slice(1);
  return (
    <group position={scaleVector(prev, scale) as Vector3}>
      {path.map((point, index) => {
        const oldPoint = prev;
        prev = point;
        console.log(`(Ships.Route) Displaying line from ${scaleVector(oldPoint,scale)} to ${scaleVector(point, scale)}`);
        return <Line key={index} start={addVector(start,scaleVector(oldPoint,scale))} end={addVector(start,scaleVector(point,scale))} color={"orange"} />;
      })}
    </group>
  );
}